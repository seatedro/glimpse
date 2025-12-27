use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;

use super::index::{Definition, Index};

pub trait WorkspaceDiscovery: Send + Sync {
    fn discover(root: &Path) -> Result<Option<Box<Self>>>
    where
        Self: Sized;

    fn resolve_module(&self, module_path: &str) -> Option<PathBuf>;

    fn root(&self) -> &Path;
}

#[derive(Debug, Clone)]
pub struct RustWorkspace {
    root: PathBuf,
    members: Vec<CrateMember>,
}

#[derive(Debug, Clone)]
pub struct CrateMember {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Deserialize)]
struct CargoToml {
    package: Option<CargoPackage>,
    workspace: Option<CargoWorkspace>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
}

#[derive(Deserialize)]
struct CargoWorkspace {
    members: Option<Vec<String>>,
}

impl WorkspaceDiscovery for RustWorkspace {
    fn discover(root: &Path) -> Result<Option<Box<Self>>> {
        let cargo_path = root.join("Cargo.toml");
        if !cargo_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&cargo_path)
            .with_context(|| format!("failed to read {}", cargo_path.display()))?;

        let cargo: CargoToml =
            toml::from_str(&content).with_context(|| "failed to parse Cargo.toml")?;

        let mut members = Vec::new();

        if let Some(ws) = cargo.workspace {
            if let Some(member_globs) = ws.members {
                for pattern in member_globs {
                    let expanded = expand_glob(root, &pattern)?;
                    for member_path in expanded {
                        if let Some(member) = parse_crate_member(&member_path)? {
                            members.push(member);
                        }
                    }
                }
            }
        }

        if let Some(pkg) = cargo.package {
            members.push(CrateMember {
                name: pkg.name,
                path: root.to_path_buf(),
            });
        }

        if members.is_empty() {
            return Ok(None);
        }

        Ok(Some(Box::new(RustWorkspace {
            root: root.to_path_buf(),
            members,
        })))
    }

    fn resolve_module(&self, module_path: &str) -> Option<PathBuf> {
        let parts: Vec<&str> = module_path.split("::").collect();
        if parts.is_empty() {
            return None;
        }

        let crate_name = parts[0];

        if crate_name == "crate" || crate_name == "self" || crate_name == "super" {
            return None;
        }

        let member = self.members.iter().find(|m| m.name == crate_name)?;

        let mut path = member.path.join("src");
        for part in &parts[1..] {
            path = path.join(part);
        }

        if path.with_extension("rs").exists() {
            return Some(path.with_extension("rs"));
        }

        let mod_path = path.join("mod.rs");
        if mod_path.exists() {
            return Some(mod_path);
        }

        None
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl RustWorkspace {
    pub fn members(&self) -> &[CrateMember] {
        &self.members
    }

    pub fn resolve_crate(&self, crate_name: &str) -> Option<&PathBuf> {
        self.members
            .iter()
            .find(|m| m.name == crate_name)
            .map(|m| &m.path)
    }
}

fn expand_glob(root: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
    let full_pattern = root.join(pattern);
    let pattern_str = full_pattern.to_string_lossy();

    let mut results = Vec::new();

    if pattern.contains('*') {
        for entry in glob::glob(&pattern_str).with_context(|| "invalid glob pattern")? {
            if let Ok(path) = entry {
                if path.is_dir() && path.join("Cargo.toml").exists() {
                    results.push(path);
                }
            }
        }
    } else {
        let path = root.join(pattern);
        if path.is_dir() && path.join("Cargo.toml").exists() {
            results.push(path);
        }
    }

    Ok(results)
}

fn parse_crate_member(path: &Path) -> Result<Option<CrateMember>> {
    let cargo_path = path.join("Cargo.toml");
    if !cargo_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&cargo_path)
        .with_context(|| format!("failed to read {}", cargo_path.display()))?;

    let cargo: CargoToml = toml::from_str(&content).with_context(|| "failed to parse Cargo.toml")?;

    let name = cargo
        .package
        .map(|p| p.name)
        .unwrap_or_else(|| path.file_name().unwrap().to_string_lossy().to_string());

    Ok(Some(CrateMember {
        name,
        path: path.to_path_buf(),
    }))
}

pub fn resolve_same_file(callee: &str, file: &Path, index: &Index) -> Option<Definition> {
    let record = index.get(file)?;
    record
        .definitions
        .iter()
        .find(|d| d.name == callee)
        .cloned()
}

pub fn resolve_by_index(callee: &str, index: &Index) -> Option<Definition> {
    index.definitions().find(|d| d.name == callee).cloned()
}

pub fn resolve_by_search(callee: &str, root: &Path) -> Result<Option<Definition>> {
    let output = Command::new("rg")
        .args([
            "--json",
            "-e",
            &format!(r"fn\s+{}\s*[\(<]", regex::escape(callee)),
            "--type",
            "rust",
            root.to_string_lossy().as_ref(),
        ])
        .output()
        .context("failed to run ripgrep")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(msg) = serde_json::from_str::<RgMessage>(line) {
            if let RgMessage::Match { data } = msg {
                return Ok(Some(Definition {
                    name: callee.to_string(),
                    kind: super::index::DefinitionKind::Function,
                    span: super::index::Span {
                        start_byte: 0,
                        end_byte: 0,
                        start_line: data.line_number.unwrap_or(1) as usize,
                        end_line: data.line_number.unwrap_or(1) as usize,
                    },
                    file: PathBuf::from(&data.path.text),
                }));
            }
        }
    }

    Ok(None)
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RgMessage {
    Match { data: RgMatchData },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct RgMatchData {
    path: RgText,
    line_number: Option<u64>,
}

#[derive(Deserialize)]
struct RgText {
    text: String,
}

pub struct Resolver<'a> {
    index: &'a Index,
    workspace: Option<Box<dyn WorkspaceDiscovery>>,
    root: PathBuf,
}

impl<'a> Resolver<'a> {
    pub fn new(
        index: &'a Index,
        workspace: Option<Box<dyn WorkspaceDiscovery>>,
        root: PathBuf,
    ) -> Self {
        Self {
            index,
            workspace,
            root,
        }
    }

    pub fn resolve(&self, callee: &str, from_file: &Path) -> Result<Option<Definition>> {
        if let Some(def) = resolve_same_file(callee, from_file, self.index) {
            return Ok(Some(def));
        }

        if let Some(ref ws) = self.workspace {
            if let Some(module_path) = ws.resolve_module(callee) {
                if let Some(record) = self.index.get(&module_path) {
                    if let Some(def) = record.definitions.first() {
                        return Ok(Some(def.clone()));
                    }
                }
            }
        }

        if let Some(def) = resolve_by_index(callee, self.index) {
            return Ok(Some(def));
        }

        resolve_by_search(callee, &self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_rust_workspace() -> TempDir {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/*"]

[package]
name = "root-crate"
version = "0.1.0"
"#,
        )
        .unwrap();

        let crate_a = dir.path().join("crates/crate-a");
        fs::create_dir_all(crate_a.join("src")).unwrap();
        fs::write(
            crate_a.join("Cargo.toml"),
            r#"
[package]
name = "crate-a"
version = "0.1.0"
"#,
        )
        .unwrap();
        fs::write(crate_a.join("src/lib.rs"), "pub fn foo() {}").unwrap();

        let crate_b = dir.path().join("crates/crate-b");
        fs::create_dir_all(crate_b.join("src")).unwrap();
        fs::write(
            crate_b.join("Cargo.toml"),
            r#"
[package]
name = "crate-b"
version = "0.1.0"
"#,
        )
        .unwrap();
        fs::write(crate_b.join("src/lib.rs"), "pub fn bar() {}").unwrap();

        dir
    }

    #[test]
    fn test_rust_workspace_discovery() {
        let dir = setup_rust_workspace();
        let ws = RustWorkspace::discover(dir.path()).unwrap().unwrap();

        assert_eq!(ws.root(), dir.path());
        assert_eq!(ws.members().len(), 3);

        let names: Vec<_> = ws.members().iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"root-crate"));
        assert!(names.contains(&"crate-a"));
        assert!(names.contains(&"crate-b"));
    }

    #[test]
    fn test_rust_workspace_resolve_crate() {
        let dir = setup_rust_workspace();
        let ws = RustWorkspace::discover(dir.path()).unwrap().unwrap();

        let path = ws.resolve_crate("crate-a").unwrap();
        assert!(path.ends_with("crates/crate-a"));

        assert!(ws.resolve_crate("nonexistent").is_none());
    }

    #[test]
    fn test_rust_workspace_resolve_module() {
        let dir = setup_rust_workspace();
        let ws = RustWorkspace::discover(dir.path()).unwrap().unwrap();

        assert!(ws.resolve_module("crate").is_none());
        assert!(ws.resolve_module("self").is_none());
        assert!(ws.resolve_module("super").is_none());
    }

    #[test]
    fn test_no_cargo_toml() {
        let dir = TempDir::new().unwrap();
        let ws = RustWorkspace::discover(dir.path()).unwrap();
        assert!(ws.is_none());
    }
}

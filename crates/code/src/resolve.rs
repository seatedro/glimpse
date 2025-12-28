use std::cell::RefCell;
use std::collections::HashSet;
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
        let src_dir = member.path.join("src");

        if parts.len() == 1 {
            let lib_rs = src_dir.join("lib.rs");
            if lib_rs.exists() {
                return Some(lib_rs);
            }
            let main_rs = src_dir.join("main.rs");
            if main_rs.exists() {
                return Some(main_rs);
            }
            return None;
        }

        let mut path = src_dir;
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


#[derive(Debug, Clone)]
pub struct GoWorkspace {
    root: PathBuf,
    module_path: String,
}

#[derive(Deserialize)]
struct GoMod {
    #[serde(rename = "Module")]
    module: GoModule,
}

#[derive(Deserialize)]
struct GoModule {
    #[serde(rename = "Path")]
    path: String,
}

impl WorkspaceDiscovery for GoWorkspace {
    fn discover(root: &Path) -> Result<Option<Box<Self>>> {
        let go_mod_path = root.join("go.mod");
        if !go_mod_path.exists() {
            return Ok(None);
        }

        let output = Command::new("go")
            .args(["mod", "edit", "-json"])
            .current_dir(root)
            .output()
            .context("failed to run go mod edit -json")?;

        if !output.status.success() {
            let content = fs::read_to_string(&go_mod_path)?;
            if let Some(module_path) = parse_go_mod_fallback(&content) {
                return Ok(Some(Box::new(GoWorkspace {
                    root: root.to_path_buf(),
                    module_path,
                })));
            }
            return Ok(None);
        }

        let go_mod: GoMod = serde_json::from_slice(&output.stdout)
            .context("failed to parse go mod output")?;

        Ok(Some(Box::new(GoWorkspace {
            root: root.to_path_buf(),
            module_path: go_mod.module.path,
        })))
    }

    fn resolve_module(&self, module_path: &str) -> Option<PathBuf> {
        if !module_path.starts_with(&self.module_path) {
            return None;
        }

        let relative = module_path
            .strip_prefix(&self.module_path)?
            .trim_start_matches('/');

        if relative.is_empty() {
            let main_go = self.root.join("main.go");
            if main_go.exists() {
                return Some(main_go);
            }
            for entry in fs::read_dir(&self.root).ok()? {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().map(|e| e == "go").unwrap_or(false) {
                    return Some(path);
                }
            }
            return None;
        }

        let pkg_dir = self.root.join(relative);
        if pkg_dir.is_dir() {
            for entry in fs::read_dir(&pkg_dir).ok()? {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().map(|e| e == "go").unwrap_or(false)
                    && !path
                        .file_name()
                        .map(|n| n.to_string_lossy().ends_with("_test.go"))
                        .unwrap_or(false)
                {
                    return Some(path);
                }
            }
        }

        None
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl GoWorkspace {
    pub fn module_path(&self) -> &str {
        &self.module_path
    }
}

fn parse_go_mod_fallback(content: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("module ") {
            return Some(line.strip_prefix("module ")?.trim().to_string());
        }
    }
    None
}


#[derive(Debug, Clone)]
pub struct TsWorkspace {
    root: PathBuf,
    name: String,
    paths: Vec<(String, PathBuf)>,
}

#[derive(Deserialize)]
struct PackageJson {
    name: Option<String>,
    #[allow(dead_code)]
    workspaces: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct TsConfig {
    #[serde(rename = "compilerOptions")]
    compiler_options: Option<TsCompilerOptions>,
}

#[derive(Deserialize)]
struct TsCompilerOptions {
    paths: Option<std::collections::HashMap<String, Vec<String>>>,
    #[serde(rename = "baseUrl")]
    base_url: Option<String>,
}

impl WorkspaceDiscovery for TsWorkspace {
    fn discover(root: &Path) -> Result<Option<Box<Self>>> {
        let pkg_json_path = root.join("package.json");
        if !pkg_json_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&pkg_json_path)
            .with_context(|| format!("failed to read {}", pkg_json_path.display()))?;

        let pkg: PackageJson =
            serde_json::from_str(&content).with_context(|| "failed to parse package.json")?;

        let name = pkg.name.unwrap_or_else(|| {
            root.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

        let mut paths = Vec::new();

        let tsconfig_path = root.join("tsconfig.json");
        if tsconfig_path.exists() {
            if let Ok(ts_content) = fs::read_to_string(&tsconfig_path) {
                if let Ok(tsconfig) = serde_json::from_str::<TsConfig>(&ts_content) {
                    if let Some(opts) = tsconfig.compiler_options {
                        let base = opts
                            .base_url
                            .map(|b| root.join(b))
                            .unwrap_or_else(|| root.to_path_buf());

                        if let Some(path_map) = opts.paths {
                            for (alias, targets) in path_map {
                                if let Some(target) = targets.first() {
                                    let clean_alias = alias.trim_end_matches("/*");
                                    let clean_target = target.trim_end_matches("/*");
                                    paths.push((clean_alias.to_string(), base.join(clean_target)));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(Some(Box::new(TsWorkspace { root: root.to_path_buf(), name, paths })))
    }

    fn resolve_module(&self, module_path: &str) -> Option<PathBuf> {
        if module_path.starts_with('.') {
            return None;
        }

        for (alias, target_dir) in &self.paths {
            if module_path.starts_with(alias) {
                let remainder = module_path.strip_prefix(alias)?.trim_start_matches('/');
                let base = if remainder.is_empty() {
                    target_dir.clone()
                } else {
                    target_dir.join(remainder)
                };

                for ext in &["ts", "tsx", "js", "jsx"] {
                    let with_ext = base.with_extension(ext);
                    if with_ext.exists() {
                        return Some(with_ext);
                    }
                }

                let index_path = base.join("index");
                for ext in &["ts", "tsx", "js", "jsx"] {
                    let with_ext = index_path.with_extension(ext);
                    if with_ext.exists() {
                        return Some(with_ext);
                    }
                }
            }
        }

        let node_modules = self.root.join("node_modules").join(module_path);
        if node_modules.exists() {
            let pkg_json = node_modules.join("package.json");
            if pkg_json.exists() {
                if let Ok(content) = fs::read_to_string(&pkg_json) {
                    if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(main) = pkg.get("main").and_then(|m| m.as_str()) {
                            let main_path = node_modules.join(main);
                            if main_path.exists() {
                                return Some(main_path);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl TsWorkspace {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn paths(&self) -> &[(String, PathBuf)] {
        &self.paths
    }
}


#[derive(Debug, Clone)]
pub struct PythonWorkspace {
    root: PathBuf,
    package_name: String,
    src_dir: PathBuf,
}

#[derive(Deserialize)]
struct PyProjectToml {
    project: Option<PyProject>,
    tool: Option<PyToolSection>,
}

#[derive(Deserialize)]
struct PyProject {
    name: Option<String>,
}

#[derive(Deserialize)]
struct PyToolSection {
    poetry: Option<PyPoetry>,
    setuptools: Option<PySetuptools>,
}

#[derive(Deserialize)]
struct PyPoetry {
    name: Option<String>,
}

#[derive(Deserialize)]
struct PySetuptools {
    #[serde(rename = "package-dir")]
    package_dir: Option<std::collections::HashMap<String, String>>,
}

impl WorkspaceDiscovery for PythonWorkspace {
    fn discover(root: &Path) -> Result<Option<Box<Self>>> {
        let pyproject_path = root.join("pyproject.toml");

        let (package_name, src_dir) = if pyproject_path.exists() {
            let content = fs::read_to_string(&pyproject_path)
                .with_context(|| format!("failed to read {}", pyproject_path.display()))?;

            let pyproject: PyProjectToml =
                toml::from_str(&content).with_context(|| "failed to parse pyproject.toml")?;

            let name = pyproject
                .project
                .and_then(|p| p.name)
                .or_else(|| pyproject.tool.as_ref().and_then(|t| t.poetry.as_ref()).and_then(|p| p.name.clone()))
                .unwrap_or_else(|| {
                    root.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                });

            let src = pyproject
                .tool
                .and_then(|t| t.setuptools)
                .and_then(|s| s.package_dir)
                .and_then(|dirs| dirs.get("").cloned())
                .map(|dir| root.join(dir))
                .unwrap_or_else(|| {
                    let src_layout = root.join("src");
                    if src_layout.exists() {
                        src_layout
                    } else {
                        root.to_path_buf()
                    }
                });

            (name, src)
        } else {
            let setup_py = root.join("setup.py");
            if !setup_py.exists() {
                return Ok(None);
            }

            let name = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let src = if root.join("src").exists() {
                root.join("src")
            } else {
                root.to_path_buf()
            };

            (name, src)
        };

        Ok(Some(Box::new(PythonWorkspace {
            root: root.to_path_buf(),
            package_name,
            src_dir,
        })))
    }

    fn resolve_module(&self, module_path: &str) -> Option<PathBuf> {
        if module_path.starts_with('.') {
            return None;
        }

        let parts: Vec<&str> = module_path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let mut path = self.src_dir.clone();
        for part in &parts {
            path = path.join(part);
        }

        let py_file = path.with_extension("py");
        if py_file.exists() {
            return Some(py_file);
        }

        let init_file = path.join("__init__.py");
        if init_file.exists() {
            return Some(init_file);
        }

        None
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl PythonWorkspace {
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    pub fn src_dir(&self) -> &Path {
        &self.src_dir
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

struct DefinitionPattern {
    extensions: &'static [&'static str],
    pattern: &'static str,
}

const DEFINITION_PATTERNS: &[DefinitionPattern] = &[
    DefinitionPattern {
        extensions: &["rs"],
        pattern: r"fn\s+{NAME}\s*[<(]",
    },
    DefinitionPattern {
        extensions: &["go"],
        pattern: r"func\s+(\([^)]*\)\s*)?{NAME}\s*[\[<(]",
    },
    DefinitionPattern {
        extensions: &["py"],
        pattern: r"def\s+{NAME}\s*\(",
    },
    DefinitionPattern {
        extensions: &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
        pattern: r"(function\s+{NAME}|const\s+{NAME}\s*=|let\s+{NAME}\s*=|{NAME}\s*\([^)]*\)\s*\{)",
    },
    DefinitionPattern {
        extensions: &["java", "scala"],
        pattern: r"(void|int|String|boolean|public|private|protected|static|def)\s+{NAME}\s*[<(]",
    },
    DefinitionPattern {
        extensions: &["c", "cpp", "cc", "cxx", "h", "hpp"],
        pattern: r"\b\w+[\s*]+{NAME}\s*\(",
    },
    DefinitionPattern {
        extensions: &["zig"],
        pattern: r"(fn|pub fn)\s+{NAME}\s*\(",
    },
    DefinitionPattern {
        extensions: &["sh", "bash"],
        pattern: r"(function\s+{NAME}|{NAME}\s*\(\s*\))",
    },
];

pub fn resolve_by_search(callee: &str, root: &Path) -> Result<Option<Definition>> {
    use grep::regex::RegexMatcher;
    use grep::searcher::sinks::UTF8;
    use grep::searcher::Searcher;

    let escaped = regex::escape(callee);

    for pattern_def in DEFINITION_PATTERNS {
        let pattern = pattern_def.pattern.replace("{NAME}", &escaped);

        let matcher = match RegexMatcher::new(&pattern) {
            Ok(m) => m,
            Err(_) => continue,
        };

        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if !pattern_def.extensions.contains(&ext) {
                continue;
            }

            let mut found: Option<(u64, PathBuf)> = None;

            let _ = Searcher::new().search_path(
                &matcher,
                path,
                UTF8(|line_num, _line| {
                    found = Some((line_num, path.to_path_buf()));
                    Ok(false)
                }),
            );

            if let Some((line_num, file_path)) = found {
                return Ok(Some(Definition {
                    name: callee.to_string(),
                    kind: super::index::DefinitionKind::Function,
                    span: super::index::Span {
                        start_byte: 0,
                        end_byte: 0,
                        start_line: line_num as usize,
                        end_line: line_num as usize,
                    },
                    file: file_path,
                }));
            }
        }
    }

    Ok(None)
}

pub struct Resolver<'a> {
    index: &'a Index,
    workspace: Option<Box<dyn WorkspaceDiscovery>>,
    root: PathBuf,
    discovered_files: RefCell<HashSet<PathBuf>>,
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
            discovered_files: RefCell::new(HashSet::new()),
        }
    }

    pub fn resolve(&self, callee: &str, from_file: &Path) -> Result<Option<Definition>> {
        if let Some(def) = resolve_same_file(callee, from_file, self.index) {
            return Ok(Some(def));
        }

        if let Some(def) = self.resolve_via_imports(callee, from_file) {
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

        if let Some(def) = resolve_by_search(callee, &self.root)? {
            self.discovered_files.borrow_mut().insert(def.file.clone());
            return Ok(Some(def));
        }

        Ok(None)
    }

    pub fn files_to_index(&self) -> Vec<PathBuf> {
        self.discovered_files.borrow().iter().cloned().collect()
    }

    pub fn clear_discovered(&self) {
        self.discovered_files.borrow_mut().clear();
    }

    fn resolve_via_imports(&self, callee: &str, from_file: &Path) -> Option<Definition> {
        let record = self.index.get(from_file)?;

        for import in &record.imports {
            let visible_name = import
                .alias
                .as_deref()
                .or_else(|| import.module_path.rsplit("::").next())
                .or_else(|| import.module_path.rsplit('.').next())?;

            if visible_name != callee {
                continue;
            }

            let original_name = import
                .module_path
                .rsplit("::")
                .next()
                .or_else(|| import.module_path.rsplit('.').next())
                .unwrap_or(callee);

            if let Some(ref ws) = self.workspace {
                if let Some(resolved_path) = ws.resolve_module(&import.module_path) {
                    if let Some(target_record) = self.index.get(&resolved_path) {
                        if let Some(def) = target_record
                            .definitions
                            .iter()
                            .find(|d| d.name == original_name)
                        {
                            return Some(def.clone());
                        }
                    }
                }
            }

            let module_path_normalized = import.module_path.replace('.', "::");
            let module_parts: Vec<&str> = module_path_normalized
                .split("::")
                .filter(|p| !p.is_empty() && *p != "crate" && *p != "self" && *p != "super")
                .collect();

            for def in self.index.definitions() {
                if def.name != original_name {
                    continue;
                }

                if module_parts.len() <= 1 {
                    return Some(def.clone());
                }

                let file_str = def.file.to_string_lossy();
                let path_parts = &module_parts[..module_parts.len() - 1];
                let matches = path_parts.iter().all(|part| file_str.contains(part));

                if matches {
                    return Some(def.clone());
                }
            }
        }

        None
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

    #[test]
    fn test_resolve_module_finds_file() {
        let dir = setup_rust_workspace();
        let ws = RustWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("crate-a");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("src/lib.rs"));
    }

    #[test]
    fn test_resolve_same_file() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Span};

        let mut index = Index::new();
        let file = PathBuf::from("src/main.rs");

        index.update(FileRecord {
            path: file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![
                Definition {
                    name: "foo".to_string(),
                    kind: DefinitionKind::Function,
                    span: Span {
                        start_byte: 0,
                        end_byte: 10,
                        start_line: 1,
                        end_line: 3,
                    },
                    file: file.clone(),
                },
                Definition {
                    name: "bar".to_string(),
                    kind: DefinitionKind::Function,
                    span: Span {
                        start_byte: 20,
                        end_byte: 30,
                        start_line: 5,
                        end_line: 7,
                    },
                    file: file.clone(),
                },
            ],
            calls: vec![],
            imports: vec![],
        });

        let found = resolve_same_file("foo", &file, &index);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo");

        let found = resolve_same_file("bar", &file, &index);
        assert!(found.is_some());

        let not_found = resolve_same_file("baz", &file, &index);
        assert!(not_found.is_none());

        let wrong_file = resolve_same_file("foo", Path::new("src/other.rs"), &index);
        assert!(wrong_file.is_none());
    }

    #[test]
    fn test_resolve_by_index() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Span};

        let mut index = Index::new();

        index.update(FileRecord {
            path: PathBuf::from("src/a.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "alpha".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 1,
                    end_line: 3,
                },
                file: PathBuf::from("src/a.rs"),
            }],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: PathBuf::from("src/b.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "beta".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 1,
                    end_line: 3,
                },
                file: PathBuf::from("src/b.rs"),
            }],
            calls: vec![],
            imports: vec![],
        });

        let found = resolve_by_index("alpha", &index);
        assert!(found.is_some());
        assert_eq!(found.as_ref().unwrap().file, PathBuf::from("src/a.rs"));

        let found = resolve_by_index("beta", &index);
        assert!(found.is_some());
        assert_eq!(found.as_ref().unwrap().file, PathBuf::from("src/b.rs"));

        let not_found = resolve_by_index("gamma", &index);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_resolver_prefers_same_file() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Span};

        let mut index = Index::new();
        let file_a = PathBuf::from("src/a.rs");
        let file_b = PathBuf::from("src/b.rs");

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "foo".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 1,
                    end_line: 3,
                },
                file: file_a.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "foo".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 10,
                    end_line: 12,
                },
                file: file_b.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index, None, PathBuf::from("."));

        let found = resolver.resolve("foo", &file_a).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_a);

        let found = resolver.resolve("foo", &file_b).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_b);
    }

    #[test]
    fn test_resolver_import_tracing() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Import, Span};

        let mut index = Index::new();
        let main_file = PathBuf::from("src/main.rs");
        let utils_file = PathBuf::from("src/utils.rs");

        index.update(FileRecord {
            path: utils_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "helper".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 50,
                    start_line: 1,
                    end_line: 5,
                },
                file: utils_file.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: main_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![Import {
                module_path: "crate::utils::helper".to_string(),
                alias: None,
                span: Span {
                    start_byte: 0,
                    end_byte: 25,
                    start_line: 1,
                    end_line: 1,
                },
                file: main_file.clone(),
            }],
        });

        let resolver = Resolver::new(&index, None, PathBuf::from("."));
        let found = resolver.resolve("helper", &main_file).unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "helper");
    }

    #[test]
    fn test_resolver_import_tracing_with_alias() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Import, Span};

        let mut index = Index::new();
        let main_file = PathBuf::from("src/main.rs");
        let utils_file = PathBuf::from("src/utils.rs");

        index.update(FileRecord {
            path: utils_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "long_function_name".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 50,
                    start_line: 1,
                    end_line: 5,
                },
                file: utils_file.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: main_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![Import {
                module_path: "crate::utils::long_function_name".to_string(),
                alias: Some("short".to_string()),
                span: Span {
                    start_byte: 0,
                    end_byte: 40,
                    start_line: 1,
                    end_line: 1,
                },
                file: main_file.clone(),
            }],
        });

        let resolver = Resolver::new(&index, None, PathBuf::from("."));

        let found = resolver.resolve("short", &main_file).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "long_function_name");

        let not_found = resolver.resolve("long_function_name", &main_file).unwrap();
        assert!(not_found.is_none() || not_found.unwrap().file != main_file);
    }

    fn setup_go_workspace() -> TempDir {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("go.mod"),
            "module github.com/example/myproject\n\ngo 1.21\n",
        )
        .unwrap();

        fs::write(dir.path().join("main.go"), "package main\n\nfunc main() {}\n").unwrap();

        let pkg_dir = dir.path().join("pkg/utils");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("helpers.go"), "package utils\n\nfunc Helper() {}\n").unwrap();

        let internal_dir = dir.path().join("internal/core");
        fs::create_dir_all(&internal_dir).unwrap();
        fs::write(internal_dir.join("core.go"), "package core\n\nfunc Process() {}\n").unwrap();

        dir
    }

    #[test]
    fn test_go_workspace_discovery() {
        let dir = setup_go_workspace();
        let ws = GoWorkspace::discover(dir.path()).unwrap();

        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.root(), dir.path());
        assert_eq!(ws.module_path(), "github.com/example/myproject");
    }

    #[test]
    fn test_go_workspace_no_go_mod() {
        let dir = TempDir::new().unwrap();
        let ws = GoWorkspace::discover(dir.path()).unwrap();
        assert!(ws.is_none());
    }

    #[test]
    fn test_go_workspace_resolve_root() {
        let dir = setup_go_workspace();
        let ws = GoWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("github.com/example/myproject");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("main.go"));
    }

    #[test]
    fn test_go_workspace_resolve_package() {
        let dir = setup_go_workspace();
        let ws = GoWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("github.com/example/myproject/pkg/utils");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("helpers.go"));
    }

    #[test]
    fn test_go_workspace_resolve_external() {
        let dir = setup_go_workspace();
        let ws = GoWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("github.com/other/package");
        assert!(resolved.is_none());
    }

    #[test]
    fn test_go_mod_fallback_parsing() {
        let content = "module github.com/foo/bar\n\ngo 1.21\n";
        let module = parse_go_mod_fallback(content);
        assert_eq!(module, Some("github.com/foo/bar".to_string()));

        let content = "// comment\nmodule   example.com/test  \n";
        let module = parse_go_mod_fallback(content);
        assert_eq!(module, Some("example.com/test".to_string()));
    }

    fn setup_ts_workspace() -> TempDir {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "my-app", "version": "1.0.0"}"#,
        )
        .unwrap();

        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": {
                        "@/*": ["src/*"],
                        "@utils/*": ["src/utils/*"]
                    }
                }
            }"#,
        )
        .unwrap();

        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("index.ts"), "export const main = () => {};\n").unwrap();

        let utils_dir = src_dir.join("utils");
        fs::create_dir_all(&utils_dir).unwrap();
        fs::write(utils_dir.join("helpers.ts"), "export const helper = () => {};\n").unwrap();

        let components_dir = src_dir.join("components");
        fs::create_dir_all(&components_dir).unwrap();
        fs::write(components_dir.join("Button.tsx"), "export const Button = () => null;\n")
            .unwrap();
        fs::write(components_dir.join("index.ts"), "export * from './Button';\n").unwrap();

        dir
    }

    #[test]
    fn test_ts_workspace_discovery() {
        let dir = setup_ts_workspace();
        let ws = TsWorkspace::discover(dir.path()).unwrap();

        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.root(), dir.path());
        assert_eq!(ws.name(), "my-app");
        assert!(!ws.paths().is_empty());
    }

    #[test]
    fn test_ts_workspace_no_package_json() {
        let dir = TempDir::new().unwrap();
        let ws = TsWorkspace::discover(dir.path()).unwrap();
        assert!(ws.is_none());
    }

    #[test]
    fn test_ts_workspace_resolve_alias() {
        let dir = setup_ts_workspace();
        let ws = TsWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("@/index");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("src/index.ts"));
    }

    #[test]
    fn test_ts_workspace_resolve_utils_alias() {
        let dir = setup_ts_workspace();
        let ws = TsWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("@utils/helpers");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("src/utils/helpers.ts"));
    }

    #[test]
    fn test_ts_workspace_resolve_index_file() {
        let dir = setup_ts_workspace();
        let ws = TsWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("@/components");
        assert!(resolved.is_some());
        let path = resolved.unwrap();
        assert!(path.ends_with("components/index.ts"));
    }

    #[test]
    fn test_ts_workspace_relative_ignored() {
        let dir = setup_ts_workspace();
        let ws = TsWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("./local");
        assert!(resolved.is_none());

        let resolved = ws.resolve_module("../parent");
        assert!(resolved.is_none());
    }

    fn setup_python_workspace_src_layout() -> TempDir {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("pyproject.toml"),
            r#"
[project]
name = "mypackage"
version = "0.1.0"

[tool.setuptools]
package-dir = {"" = "src"}
"#,
        )
        .unwrap();

        let pkg_dir = dir.path().join("src/mypackage");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("__init__.py"), "").unwrap();
        fs::write(pkg_dir.join("main.py"), "def main(): pass\n").unwrap();

        let utils_dir = pkg_dir.join("utils");
        fs::create_dir_all(&utils_dir).unwrap();
        fs::write(utils_dir.join("__init__.py"), "").unwrap();
        fs::write(utils_dir.join("helpers.py"), "def helper(): pass\n").unwrap();

        dir
    }

    fn setup_python_workspace_flat_layout() -> TempDir {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("pyproject.toml"),
            r#"
[project]
name = "flatpkg"
version = "0.1.0"
"#,
        )
        .unwrap();

        let pkg_dir = dir.path().join("flatpkg");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join("__init__.py"), "").unwrap();
        fs::write(pkg_dir.join("core.py"), "def process(): pass\n").unwrap();

        dir
    }

    #[test]
    fn test_python_workspace_discovery_src_layout() {
        let dir = setup_python_workspace_src_layout();
        let ws = PythonWorkspace::discover(dir.path()).unwrap();

        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.root(), dir.path());
        assert_eq!(ws.package_name(), "mypackage");
        assert!(ws.src_dir().ends_with("src"));
    }

    #[test]
    fn test_python_workspace_discovery_flat_layout() {
        let dir = setup_python_workspace_flat_layout();
        let ws = PythonWorkspace::discover(dir.path()).unwrap();

        assert!(ws.is_some());
        let ws = ws.unwrap();
        assert_eq!(ws.package_name(), "flatpkg");
    }

    #[test]
    fn test_python_workspace_no_pyproject() {
        let dir = TempDir::new().unwrap();
        let ws = PythonWorkspace::discover(dir.path()).unwrap();
        assert!(ws.is_none());
    }

    #[test]
    fn test_python_workspace_resolve_module() {
        let dir = setup_python_workspace_src_layout();
        let ws = PythonWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("mypackage.main");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("mypackage/main.py"));
    }

    #[test]
    fn test_python_workspace_resolve_package() {
        let dir = setup_python_workspace_src_layout();
        let ws = PythonWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("mypackage.utils");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("utils/__init__.py"));
    }

    #[test]
    fn test_python_workspace_resolve_submodule() {
        let dir = setup_python_workspace_src_layout();
        let ws = PythonWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module("mypackage.utils.helpers");
        assert!(resolved.is_some());
        assert!(resolved.unwrap().ends_with("utils/helpers.py"));
    }

    #[test]
    fn test_python_workspace_relative_ignored() {
        let dir = setup_python_workspace_src_layout();
        let ws = PythonWorkspace::discover(dir.path()).unwrap().unwrap();

        let resolved = ws.resolve_module(".relative");
        assert!(resolved.is_none());
    }

    #[test]
    fn test_python_workspace_poetry_project() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("pyproject.toml"),
            r#"
[tool.poetry]
name = "poetry-project"
version = "0.1.0"
"#,
        )
        .unwrap();

        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();

        let ws = PythonWorkspace::discover(dir.path()).unwrap().unwrap();
        assert_eq!(ws.package_name(), "poetry-project");
    }

    #[test]
    fn test_resolve_by_search_rust() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("lib.rs"),
            "pub fn my_function() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("my_function", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "my_function");
        assert_eq!(def.span.start_line, 1);
    }

    #[test]
    fn test_resolve_by_search_python() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("utils.py"),
            "def helper_func():\n    pass\n",
        )
        .unwrap();

        let found = resolve_by_search("helper_func", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "helper_func");
    }

    #[test]
    fn test_resolve_by_search_go() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.go"),
            "package main\n\nfunc ProcessData() {\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("ProcessData", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "ProcessData");
    }

    #[test]
    fn test_resolve_by_search_typescript() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("index.ts"),
            "export function fetchData() {\n  return null;\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("fetchData", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "fetchData");
    }

    #[test]
    fn test_resolve_by_search_not_found() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join("lib.rs"), "pub fn other() {}\n").unwrap();

        let found = resolve_by_search("nonexistent", dir.path()).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_resolver_tracks_discovered_files() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("utils.rs"),
            "pub fn discovered_func() {}\n",
        )
        .unwrap();

        fs::write(
            dir.path().join("helpers.rs"),
            "pub fn another_func() {}\n",
        )
        .unwrap();

        let index = Index::new();
        let resolver = Resolver::new(&index, None, dir.path().to_path_buf());

        assert!(resolver.files_to_index().is_empty());

        let _ = resolver.resolve("discovered_func", Path::new("main.rs"));
        let files = resolver.files_to_index();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("utils.rs"));

        let _ = resolver.resolve("another_func", Path::new("main.rs"));
        let files = resolver.files_to_index();
        assert_eq!(files.len(), 2);

        resolver.clear_discovered();
        assert!(resolver.files_to_index().is_empty());
    }
}

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use lsp_types::{
    ClientCapabilities, DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse,
    InitializeParams, InitializedParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkspaceFolder,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, info, trace, warn};

use super::grammar::{lsp_dir, LspConfig, Registry};
use super::index::{Call, Index, ResolvedCall};

fn current_target() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "unknown"
    }
}

fn binary_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

fn lsp_binary_path(lsp: &LspConfig) -> PathBuf {
    let dir = lsp_dir();
    dir.join(format!("{}{}", lsp.binary, binary_extension()))
}

fn path_to_uri(path: &Path) -> Result<Uri> {
    let url = url::Url::from_file_path(path)
        .map_err(|_| anyhow::anyhow!("invalid path: {}", path.display()))?;
    url.as_str().parse().context("failed to convert URL to URI")
}

fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let url = url::Url::parse(uri.as_str()).ok()?;
    url.to_file_path().ok()
}

fn is_declaration_file_uri(uri: &str) -> bool {
    let path = uri.rsplit('/').next().unwrap_or(uri);
    if path.ends_with(".d.ts") || path.ends_with(".d.mts") || path.ends_with(".d.cts") {
        return true;
    }
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(ext, "h" | "hpp" | "hxx" | "hh")
}

fn should_skip_for_lsp(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    
    const SKIP_DIRS: &[&str] = &[
        "vendor/",
        "vendors/",
        "third_party/",
        "third-party/",
        "thirdparty/",
        "external/",
        "externals/",
        "deps/",
        "node_modules/",
        ".git/",
        "dist/",
        "build/",
        "out/",
        "__pycache__/",
    ];
    
    for dir in SKIP_DIRS {
        if path_str.contains(dir) {
            return true;
        }
    }
    
    false
}

fn detect_zig_version_from_zon(root: &Path) -> Option<String> {
    let zon_path = root.join("build.zig.zon");
    let content = fs::read_to_string(zon_path).ok()?;
    let re = regex::Regex::new(r#"\.minimum_zig_version\s*=\s*"([^"]+)""#).ok()?;
    let caps = re.captures(&content)?;
    Some(caps.get(1)?.as_str().to_string())
}

fn detect_zig_version(root: &Path) -> Option<String> {
    let zig_version = if let Ok(output) = Command::new("zig").arg("version").output() {
        if output.status.success() {
            let version_str = String::from_utf8_lossy(&output.stdout);
            let version = version_str.trim();
            version.split('-').next().map(|s| s.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let zig_version = zig_version.or_else(|| detect_zig_version_from_zon(root))?;

    // zls releases may lag behind zig - try to find matching major.minor
    // e.g., zig 0.15.2 -> try 0.15.2, 0.15.1, 0.15.0
    let parts: Vec<&str> = zig_version.split('.').collect();
    if parts.len() >= 2 {
        let major_minor = format!("{}.{}", parts[0], parts[1]);
        // Try decreasing patch versions
        for patch in (0..=10).rev() {
            let version = format!("{}.{}", major_minor, patch);
            let url = format!(
                "https://github.com/zigtools/zls/releases/download/{}/zls-x86_64-linux.tar.xz",
                version
            );
            if let Ok(resp) = reqwest::blocking::Client::new().head(&url).send() {
                if resp.status().is_success() || resp.status().as_u16() == 302 {
                    debug!(zig_version = %zig_version, zls_version = %version, "found matching zls version");
                    return Some(version);
                }
            }
        }
    }

    Some(zig_version)
}

#[allow(clippy::literal_string_with_formatting_args)]
fn download_and_extract(lsp: &LspConfig, root: &Path) -> Result<PathBuf> {
    let Some(ref url_template) = lsp.url_template else {
        bail!("no download URL configured for {}", lsp.binary);
    };

    let version = if lsp.binary == "zls" {
        detect_zig_version(root)
            .with_context(|| "failed to detect zig version. Install zig or install zls manually")?
    } else {
        lsp.version
            .clone()
            .with_context(|| format!("no version configured for {}", lsp.binary))?
    };

    let target = current_target();

    let url = if let Some(ref latest_txt_url) = lsp.latest_txt_url {
        let latest_url = latest_txt_url.replace("{version}", &version);
        let filename = reqwest::blocking::get(&latest_url)
            .with_context(|| format!("failed to fetch {}", latest_url))?
            .text()
            .with_context(|| "failed to read latest.txt")?
            .trim()
            .to_string();
        url_template
            .replace("{version}", &version)
            .replace("{filename}", &filename)
    } else {
        let Some(target_name) = lsp.targets.get(target) else {
            bail!(
                "no pre-built binary available for {} on {}",
                lsp.binary,
                target
            );
        };
        url_template
            .replace("{version}", &version)
            .replace("{target}", target_name)
    };

    let dir = lsp_dir();
    fs::create_dir_all(&dir)?;

    let response =
        reqwest::blocking::get(&url).with_context(|| format!("failed to download {}", url))?;

    if !response.status().is_success() {
        bail!("download failed with status: {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);
    info!(binary = %lsp.binary, size_mb = total_size as f64 / 1024.0 / 1024.0, "downloading LSP server");

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .expect("valid template")
            .progress_chars("━━─"),
    );

    let mut bytes = Vec::new();
    let mut reader = response;
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
        pb.set_position(bytes.len() as u64);
    }
    pb.finish_and_clear();
    info!(binary = %lsp.binary, "extracting LSP server");

    let archive_type = lsp.archive.as_deref().unwrap_or("gz");

    let final_path = lsp_binary_path(lsp);

    match archive_type {
        "gz" => {
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut output = File::create(&final_path)?;
            std::io::copy(&mut decoder, &mut output)?;
        }
        "tar.xz" => {
            let decoder = xz2::read::XzDecoder::new(&bytes[..]);
            let mut archive = tar::Archive::new(decoder);

            let binary_name = format!("{}{}", lsp.binary, binary_extension());
            let mut found = false;

            for entry in archive.entries()? {
                let mut entry = entry?;
                let path = entry.path()?;
                if let Some(name) = path.file_name() {
                    if name == binary_name.as_str() {
                        let mut output = File::create(&final_path)?;
                        std::io::copy(&mut entry, &mut output)?;
                        found = true;
                        break;
                    }
                }
            }

            if !found {
                bail!("binary {} not found in tar.xz archive", binary_name);
            }
        }
        "tar.gz" => {
            let decoder = flate2::read::GzDecoder::new(&bytes[..]);
            let mut archive = tar::Archive::new(decoder);

            if let Some(ref binary_path_template) = lsp.binary_path {
                let binary_path = if let Some(ref v) = lsp.version {
                    binary_path_template.replace("{version}", v)
                } else {
                    binary_path_template.clone()
                };

                let extract_dir = lsp_dir().join(format!("{}-extract", lsp.binary));
                fs::create_dir_all(&extract_dir)?;
                archive.unpack(&extract_dir)?;

                let full_binary_path = extract_dir.join(&binary_path);
                if full_binary_path.exists() {
                    fs::copy(&full_binary_path, &final_path)?;
                } else {
                    bail!(
                        "binary {} not found at {} in tar.gz archive",
                        lsp.binary,
                        binary_path
                    );
                }
            } else {
                let binary_name = format!("{}{}", lsp.binary, binary_extension());
                let mut found = false;

                for entry in archive.entries()? {
                    let mut entry = entry?;
                    let path = entry.path()?;
                    if let Some(name) = path.file_name() {
                        if name == binary_name.as_str() {
                            let mut output = File::create(&final_path)?;
                            std::io::copy(&mut entry, &mut output)?;
                            found = true;
                            break;
                        }
                    }
                }

                if !found {
                    bail!("binary {} not found in tar.gz archive", binary_name);
                }
            }
        }
        "zip" => {
            let cursor = std::io::Cursor::new(&bytes);
            let mut archive = zip::ZipArchive::new(cursor)?;

            let binary_path = if let Some(ref path) = lsp.binary_path {
                path.replace("{version}", &version)
            } else {
                lsp.binary.clone()
            };

            let mut found = false;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                let name = file.name().to_string();

                if name.ends_with(&binary_path)
                    || name.ends_with(&format!("{}{}", binary_path, binary_extension()))
                {
                    let mut output = File::create(&final_path)?;
                    std::io::copy(&mut file, &mut output)?;
                    found = true;
                    break;
                }
            }

            if !found {
                bail!("binary {} not found in archive", binary_path);
            }
        }
        other => bail!("unsupported archive type: {}", other),
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&final_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&final_path, perms)?;
    }

    info!(binary = %lsp.binary, path = %final_path.display(), "installed LSP server");
    Ok(final_path)
}

fn install_npm_package(lsp: &LspConfig) -> Result<PathBuf> {
    let Some(ref package) = lsp.npm_package else {
        bail!("no npm package configured for {}", lsp.binary);
    };

    let (pkg_manager, pkg_manager_path) = if let Ok(bun) = which::which("bun") {
        ("bun", bun)
    } else if let Ok(npm) = which::which("npm") {
        ("npm", npm)
    } else {
        bail!("neither bun nor npm found. Install one of them or install the LSP manually");
    };

    info!(binary = %lsp.binary, pkg_manager = %pkg_manager, "installing LSP server via package manager");

    let pkg_dir = lsp_dir().join(format!("{}-pkg", &lsp.binary));
    fs::create_dir_all(&pkg_dir)?;

    let init_status = Command::new(&pkg_manager_path)
        .args(["init", "--yes"])
        .current_dir(&pkg_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to run {} init", pkg_manager))?;

    if !init_status.success() {
        bail!("{} init failed", pkg_manager);
    }

    let packages: Vec<&str> = package.split_whitespace().collect();
    let mut install_args = vec!["add"];
    install_args.extend(packages.iter());

    let install_status = Command::new(&pkg_manager_path)
        .args(&install_args)
        .current_dir(&pkg_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to run {} add", pkg_manager))?;

    if !install_status.success() {
        bail!("{} add failed for {}", pkg_manager, package);
    }

    let bin_path = pkg_dir.join("node_modules").join(".bin").join(&lsp.binary);
    if !bin_path.exists() {
        bail!(
            "installed {} but binary not found at {}",
            package,
            bin_path.display()
        );
    }

    let wrapper_path = lsp_binary_path(lsp);
    create_wrapper_script(&wrapper_path, &bin_path)?;

    Ok(wrapper_path)
}

fn create_wrapper_script(wrapper_path: &Path, target_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let script = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", target_path.display());
        fs::write(wrapper_path, script)?;

        let mut perms = fs::metadata(wrapper_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(wrapper_path, perms)?;
    }

    #[cfg(windows)]
    {
        let script = format!("@echo off\r\n\"{}\" %*\r\n", target_path.display());
        let wrapper_cmd = wrapper_path.with_extension("cmd");
        fs::write(&wrapper_cmd, script)?;
    }

    Ok(())
}

fn install_go_package(lsp: &LspConfig) -> Result<PathBuf> {
    let Some(ref package) = lsp.go_package else {
        bail!("no go package configured for {}", lsp.binary);
    };

    let go_path =
        which::which("go").context("go not found. Install Go or install the LSP manually")?;

    info!(binary = %lsp.binary, "installing LSP server via go install");

    let install_dir = lsp_dir();
    fs::create_dir_all(&install_dir)?;

    let status = Command::new(&go_path)
        .args(["install", package])
        .env("GOBIN", &install_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run go install")?;

    if !status.success() {
        bail!("go install failed for {}", package);
    }

    let binary_path = install_dir.join(&lsp.binary);
    if binary_path.exists() {
        return Ok(binary_path);
    }

    bail!(
        "go install succeeded but binary {} not found at {}",
        lsp.binary,
        binary_path.display()
    );
}

fn install_cargo_crate(lsp: &LspConfig) -> Result<PathBuf> {
    let Some(ref crate_name) = lsp.cargo_crate else {
        bail!("no cargo crate configured for {}", lsp.binary);
    };

    let cargo_path = which::which("cargo")
        .context("cargo not found. Install Rust/Cargo or install the LSP manually")?;

    info!(binary = %lsp.binary, "installing LSP server via cargo install");

    let install_dir = lsp_dir();
    fs::create_dir_all(&install_dir)?;

    let status = Command::new(&cargo_path)
        .args(["install", crate_name, "--root"])
        .arg(&install_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to run cargo install")?;

    if !status.success() {
        bail!("cargo install failed for {}", crate_name);
    }

    let binary_path = install_dir.join("bin").join(&lsp.binary);
    if binary_path.exists() {
        let final_path = lsp_binary_path(lsp);
        if binary_path != final_path {
            fs::copy(&binary_path, &final_path)?;
        }
        return Ok(final_path);
    }

    bail!(
        "cargo install succeeded but binary {} not found at {}",
        lsp.binary,
        binary_path.display()
    );
}

fn find_lsp_binary(lsp: &LspConfig, root: &Path) -> Result<PathBuf> {
    let local_path = lsp_binary_path(lsp);
    if local_path.exists() {
        debug!(binary = %lsp.binary, path = %local_path.display(), "using cached LSP binary");
        return Ok(local_path);
    }

    if let Ok(system_path) = which::which(&lsp.binary) {
        debug!(binary = %lsp.binary, path = %system_path.display(), "using system LSP binary");
        return Ok(system_path);
    }

    debug!(binary = %lsp.binary, "LSP not found, attempting install");

    if lsp.url_template.is_some() {
        return download_and_extract(lsp, root);
    }

    if lsp.npm_package.is_some() {
        return install_npm_package(lsp);
    }

    if lsp.go_package.is_some() {
        return install_go_package(lsp);
    }

    if lsp.cargo_crate.is_some() {
        return install_cargo_crate(lsp);
    }

    bail!(
        "LSP server '{}' not found. Install it manually.",
        lsp.binary
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum LspId {
    Int(i32),
    String(String),
}

impl LspId {
    fn as_i32(&self) -> Option<i32> {
        match self {
            LspId::Int(n) => Some(*n),
            LspId::String(_) => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LspMessage {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<LspId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

fn extract_marked_string(marked: &lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s.clone(),
        lsp_types::MarkedString::LanguageString(ls) => ls.value.clone(),
    }
}

pub fn language_id_for_ext(ext: &str) -> &'static str {
    match ext {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "java" => "java",
        "zig" => "zig",
        "sh" | "bash" => "shellscript",
        "scala" | "sc" => "scala",
        _ => "text",
    }
}

#[derive(Debug, Default, Clone)]
pub struct LspServerStats {
    pub resolved: usize,
    pub no_definition: usize,
    pub external: usize,
    pub not_indexed: usize,
    pub no_match: usize,
}

#[derive(Debug, Default)]
pub struct LspTimingStats {
    pub wait_ready_ms: AtomicU64,
    pub open_source_files_ms: AtomicU64,
    pub open_source_files_count: AtomicU64,
    pub open_def_files_ms: AtomicU64,
    pub open_def_files_count: AtomicU64,
    pub goto_definition_ms: AtomicU64,
    pub goto_definition_count: AtomicU64,
    pub hover_ms: AtomicU64,
    pub hover_count: AtomicU64,
    pub declaration_chase_ms: AtomicU64,
    pub declaration_chase_count: AtomicU64,
}

impl LspTimingStats {
    pub fn add_wait_ready(&self, ms: u64) {
        self.wait_ready_ms.fetch_add(ms, Ordering::Relaxed);
    }

    pub fn add_open_source_file(&self, ms: u64) {
        self.open_source_files_ms.fetch_add(ms, Ordering::Relaxed);
        self.open_source_files_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_open_def_file(&self, ms: u64) {
        self.open_def_files_ms.fetch_add(ms, Ordering::Relaxed);
        self.open_def_files_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_goto_definition(&self, ms: u64) {
        self.goto_definition_ms.fetch_add(ms, Ordering::Relaxed);
        self.goto_definition_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_hover(&self, ms: u64) {
        self.hover_ms.fetch_add(ms, Ordering::Relaxed);
        self.hover_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_declaration_chase(&self, ms: u64) {
        self.declaration_chase_ms.fetch_add(ms, Ordering::Relaxed);
        self.declaration_chase_count.fetch_add(1, Ordering::Relaxed);
    }
}

impl std::fmt::Display for LspTimingStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let wait_ready = self.wait_ready_ms.load(Ordering::Relaxed);
        let open_src_ms = self.open_source_files_ms.load(Ordering::Relaxed);
        let open_src_count = self.open_source_files_count.load(Ordering::Relaxed);
        let open_def_ms = self.open_def_files_ms.load(Ordering::Relaxed);
        let open_def_count = self.open_def_files_count.load(Ordering::Relaxed);
        let goto_ms = self.goto_definition_ms.load(Ordering::Relaxed);
        let goto_count = self.goto_definition_count.load(Ordering::Relaxed);
        let hover_ms = self.hover_ms.load(Ordering::Relaxed);
        let hover_count = self.hover_count.load(Ordering::Relaxed);
        let decl_ms = self.declaration_chase_ms.load(Ordering::Relaxed);
        let decl_count = self.declaration_chase_count.load(Ordering::Relaxed);

        writeln!(f, "LSP Timing Breakdown:")?;
        writeln!(f, "  wait_for_ready:     {:>7}ms", wait_ready)?;
        writeln!(
            f,
            "  open source files:  {:>7}ms ({} files, {:.1}ms avg)",
            open_src_ms,
            open_src_count,
            if open_src_count > 0 { open_src_ms as f64 / open_src_count as f64 } else { 0.0 }
        )?;
        writeln!(
            f,
            "  open def files:     {:>7}ms ({} files, {:.1}ms avg)",
            open_def_ms,
            open_def_count,
            if open_def_count > 0 { open_def_ms as f64 / open_def_count as f64 } else { 0.0 }
        )?;
        writeln!(
            f,
            "  goto_definition:    {:>7}ms ({} calls, {:.1}ms avg)",
            goto_ms,
            goto_count,
            if goto_count > 0 { goto_ms as f64 / goto_count as f64 } else { 0.0 }
        )?;
        writeln!(
            f,
            "  hover:              {:>7}ms ({} calls, {:.1}ms avg)",
            hover_ms,
            hover_count,
            if hover_count > 0 { hover_ms as f64 / hover_count as f64 } else { 0.0 }
        )?;
        writeln!(
            f,
            "  declaration chase:  {:>7}ms ({} calls, {:.1}ms avg)",
            decl_ms,
            decl_count,
            if decl_count > 0 { decl_ms as f64 / decl_count as f64 } else { 0.0 }
        )?;

        let total = wait_ready + open_src_ms + open_def_ms + goto_ms + hover_ms + decl_ms;
        writeln!(f, "  ─────────────────────────────")?;
        writeln!(f, "  total accounted:    {:>7}ms", total)
    }
}

#[derive(Debug, Default, Clone)]
pub struct LspStats {
    pub by_server: HashMap<String, LspServerStats>,
}

impl LspStats {
    pub fn total_resolved(&self) -> usize {
        self.by_server.values().map(|s| s.resolved).sum()
    }
}

impl std::fmt::Display for LspStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut servers: Vec<_> = self.by_server.iter().collect();
        servers.sort_by_key(|(name, _)| name.as_str());

        let parts: Vec<String> = servers
            .iter()
            .map(|(name, stats)| {
                let total = stats.resolved
                    + stats.external
                    + stats.no_definition
                    + stats.not_indexed
                    + stats.no_match;
                format!(
                    "{}: {}/{} resolved ({} external, {} no-def, {} not-indexed, {} no-match)",
                    name,
                    stats.resolved,
                    total,
                    stats.external,
                    stats.no_definition,
                    stats.not_indexed,
                    stats.no_match
                )
            })
            .collect();

        write!(f, "{}", parts.join("\n     "))
    }
}

fn extract_signature(hover_content: &str) -> Option<String> {
    let lines: Vec<&str> = hover_content.lines().collect();
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("fn ")
            || trimmed.starts_with("pub fn ")
            || trimmed.starts_with("async fn ")
            || trimmed.starts_with("pub async fn ")
            || trimmed.starts_with("def ")
            || trimmed.starts_with("function ")
            || trimmed.starts_with("func ")
        {
            return Some(trimmed.to_string());
        }
        if trimmed.contains("->") || trimmed.contains("=>") {
            return Some(trimmed.to_string());
        }
    }
    lines.first().map(|s| s.trim().to_string())
}

fn extract_type(hover_content: &str) -> Option<String> {
    let content = hover_content.trim();
    if content.is_empty() {
        return None;
    }

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("let ") || trimmed.starts_with("const ") {
            if let Some(colon_pos) = trimmed.find(':') {
                let type_part = trimmed[colon_pos + 1..].trim();
                let type_end = type_part.find('=').unwrap_or(type_part.len());
                return Some(type_part[..type_end].trim().to_string());
            }
        }
        if !trimmed.starts_with("fn ") && !trimmed.starts_with("def ") {
            if let Some(first_line) = trimmed.split('\n').next() {
                return Some(first_line.to_string());
            }
        }
    }
    Some(content.lines().next()?.to_string())
}

#[derive(Debug, Clone)]
pub struct LspAvailability {
    pub available: bool,
    pub location: Option<String>,
    pub can_auto_install: bool,
    pub install_method: Option<String>,
}

pub fn check_lsp_availability() -> HashMap<String, LspAvailability> {
    let registry = Registry::global();
    let mut result = HashMap::new();

    for lang in registry.languages() {
        if let Some(ref lsp) = lang.lsp {
            let local_path = lsp_binary_path(lsp);
            let system_available = which::which(&lsp.binary).is_ok();
            let local_available = local_path.exists();
            let available = system_available || local_available;

            let location = if local_available {
                Some(local_path.display().to_string())
            } else if system_available {
                which::which(&lsp.binary)
                    .ok()
                    .map(|p| p.display().to_string())
            } else {
                None
            };

            let (can_auto_install, install_method) = if lsp.url_template.is_some() {
                (true, Some("download".to_string()))
            } else if lsp.npm_package.is_some() {
                let bun_available = which::which("bun").is_ok();
                let npm_available = which::which("npm").is_ok();
                if bun_available {
                    (true, Some("bun".to_string()))
                } else if npm_available {
                    (true, Some("npm".to_string()))
                } else {
                    (false, Some("npm/bun".to_string()))
                }
            } else if lsp.go_package.is_some() {
                let go_available = which::which("go").is_ok();
                (go_available, Some("go".to_string()))
            } else if lsp.cargo_crate.is_some() {
                let cargo_available = which::which("cargo").is_ok();
                (cargo_available, Some("cargo".to_string()))
            } else {
                (false, None)
            };

            result.insert(
                lang.name.clone(),
                LspAvailability {
                    available,
                    location,
                    can_auto_install: can_auto_install && !available,
                    install_method,
                },
            );
        }
    }

    result
}

pub fn ensure_lsp_for_extension(ext: &str, root: &Path) -> Result<PathBuf> {
    let registry = Registry::global();
    let lang_entry = registry
        .get_by_extension(ext)
        .with_context(|| format!("no language for extension: {}", ext))?;

    let lsp_config = lang_entry
        .lsp
        .as_ref()
        .with_context(|| format!("no LSP config for language: {}", lang_entry.name))?;

    find_lsp_binary(lsp_config, root)
}

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::process::{Child as TokioChild, Command as TokioCommand};
use tokio::sync::{oneshot, Mutex, Notify};

#[derive(Debug)]
struct AsyncLspClientInner {
    writer: Mutex<tokio::process::ChildStdin>,
    pending: Mutex<HashMap<i32, oneshot::Sender<Result<Value, String>>>>,
    request_id: AtomicI32,
    root_uri: Uri,
    opened_files: Mutex<HashMap<PathBuf, i32>>,
    is_ready: std::sync::atomic::AtomicBool,
    active_progress: Mutex<HashSet<String>>,
    progress_notify: Notify,
    _process: Mutex<TokioChild>,
}

#[derive(Debug, Clone)]
struct AsyncLspClient {
    inner: Arc<AsyncLspClientInner>,
    _shutdown_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl AsyncLspClient {
    async fn new(lsp: &LspConfig, root: &Path) -> Result<Self> {
        let binary_path = find_lsp_binary(lsp, root)?;

        let mut process = TokioCommand::new(&binary_path)
            .args(&lsp.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn {}", binary_path.display()))?;

        let stdin = process.stdin.take().context("failed to get stdin")?;
        let stdout = process.stdout.take().context("failed to get stdout")?;

        let root_uri = path_to_uri(&root.canonicalize().unwrap_or_else(|_| root.to_path_buf()))?;

        let inner = Arc::new(AsyncLspClientInner {
            writer: Mutex::new(stdin),
            pending: Mutex::new(HashMap::new()),
            request_id: AtomicI32::new(1),
            root_uri,
            opened_files: Mutex::new(HashMap::new()),
            is_ready: std::sync::atomic::AtomicBool::new(false),
            active_progress: Mutex::new(HashSet::new()),
            progress_notify: Notify::new(),
            _process: Mutex::new(process),
        });

        let pending_clone = Arc::clone(&inner);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            Self::response_reader_task(stdout, pending_clone, shutdown_rx).await;
        });

        Ok(Self {
            inner,
            _shutdown_tx: Arc::new(Mutex::new(Some(shutdown_tx))),
        })
    }

    async fn response_reader_task(
        stdout: tokio::process::ChildStdout,
        inner: Arc<AsyncLspClientInner>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        let mut reader = TokioBufReader::new(stdout);
        let mut header_buf = String::new();

        loop {
            tokio::select! {
                _ = &mut shutdown_rx => {
                    trace!("async LSP reader shutting down");
                    break;
                }
                result = Self::read_one_message(&mut reader, &mut header_buf) => {
                    match result {
                        Ok(Some(msg)) => {
                            if let Some(ref method) = msg.method {
                                Self::handle_server_message(&inner, &msg, method).await;
                            }
                            if let Some(ref id) = msg.id {
                                if msg.method.is_none() {
                                    if let Some(int_id) = id.as_i32() {
                                        let mut pending_guard = inner.pending.lock().await;
                                        if let Some(tx) = pending_guard.remove(&int_id) {
                                            let result = if let Some(ref error) = msg.error {
                                                Err(format!("LSP error: {}", error))
                                            } else {
                                                Ok(msg.result.clone().unwrap_or(Value::Null))
                                            };
                                            let _ = tx.send(result);
                                        }
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            warn!("LSP process closed stdout");
                            break;
                        }
                        Err(e) => {
                            warn!(error = ?e, "error reading LSP message");
                            break;
                        }
                    }
                }
            }
        }
        
        let mut pending = inner.pending.lock().await;
        let count = pending.len();
        if count > 0 {
            warn!(count, "LSP died, failing pending requests");
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err("LSP process died".to_string()));
            }
        }
    }

    async fn handle_server_message(inner: &Arc<AsyncLspClientInner>, msg: &LspMessage, method: &str) {
        match method {
            "window/workDoneProgress/create" => {
                if let Some(ref id) = msg.id {
                    if let Some(ref params) = msg.params {
                        if let Some(token) = params.get("token").and_then(|t| t.as_str()) {
                            let mut progress = inner.active_progress.lock().await;
                            progress.insert(token.to_string());
                            trace!(token = %token, "progress token created");
                        }
                    }
                    let response = LspMessage {
                        jsonrpc: "2.0".to_string(),
                        id: Some(id.clone()),
                        method: None,
                        params: None,
                        result: Some(Value::Null),
                        error: None,
                    };
                    let content = serde_json::to_string(&response).unwrap_or_default();
                    let header = format!("Content-Length: {}\r\n\r\n", content.len());
                    let mut writer = inner.writer.lock().await;
                    let _ = writer.write_all(header.as_bytes()).await;
                    let _ = writer.write_all(content.as_bytes()).await;
                    let _ = writer.flush().await;
                }
            }
            "$/progress" => {
                if let Some(ref params) = msg.params {
                    let token = params
                        .get("token")
                        .and_then(|t| t.as_str().map(|s| s.to_string()).or_else(|| t.as_i64().map(|n| n.to_string())));
                    let kind = params
                        .get("value")
                        .and_then(|v| v.get("kind"))
                        .and_then(|k| k.as_str());

                    if let (Some(token), Some(kind)) = (token, kind) {
                        match kind {
                            "begin" => {
                                let mut progress = inner.active_progress.lock().await;
                                progress.insert(token.clone());
                                trace!(token = %token, "progress begin");
                            }
                            "end" => {
                                let mut progress = inner.active_progress.lock().await;
                                progress.remove(&token);
                                trace!(token = %token, remaining = progress.len(), "progress end");
                                if progress.is_empty() {
                                    inner.progress_notify.notify_waiters();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    async fn read_one_message(
        reader: &mut TokioBufReader<tokio::process::ChildStdout>,
        header_buf: &mut String,
    ) -> Result<Option<LspMessage>> {
        let mut content_length: Option<usize> = None;

        loop {
            header_buf.clear();
            let bytes_read = reader.read_line(header_buf).await?;
            if bytes_read == 0 {
                return Ok(None);
            }

            if header_buf == "\r\n" || header_buf.is_empty() {
                break;
            }

            if let Some(len_str) = header_buf.strip_prefix("Content-Length: ") {
                content_length = Some(len_str.trim().parse()?);
            }
        }

        let len = content_length.context("missing Content-Length header")?;
        let mut body = vec![0u8; len];
        reader.read_exact(&mut body).await?;

        let msg: LspMessage = serde_json::from_slice(&body)?;
        Ok(Some(msg))
    }

    fn next_id(&self) -> i32 {
        self.inner.request_id.fetch_add(1, Ordering::SeqCst)
    }

    async fn send_message(&self, msg: &LspMessage) -> Result<()> {
        let content = serde_json::to_string(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let mut writer = self.inner.writer.lock().await;
        writer.write_all(header.as_bytes()).await?;
        writer.write_all(content.as_bytes()).await?;
        writer.flush().await?;

        Ok(())
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.inner.pending.lock().await;
            pending.insert(id, tx);
        }

        let msg = LspMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(LspId::Int(id)),
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        };

        self.send_message(&msg).await?;

        self.await_response(rx, std::time::Duration::from_secs(30)).await
    }

    async fn send_request_with_timeout(&self, method: &str, params: Value, timeout: std::time::Duration) -> Result<Value> {
        let id = self.next_id();
        let (tx, rx) = oneshot::channel();

        {
            let mut pending = self.inner.pending.lock().await;
            pending.insert(id, tx);
        }

        let msg = LspMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(LspId::Int(id)),
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        };

        self.send_message(&msg).await?;
        self.await_response(rx, timeout).await
    }

    async fn await_response(&self, rx: oneshot::Receiver<Result<Value, String>>, timeout: std::time::Duration) -> Result<Value> {
        let result = tokio::time::timeout(timeout, rx).await;
        match result {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(e))) => bail!("{}", e),
            Ok(Err(_)) => bail!("LSP response channel closed"),
            Err(_) => bail!("LSP request timed out after {}s", timeout.as_secs()),
        }
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let msg = LspMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        };

        self.send_message(&msg).await
    }

    async fn initialize(&self) -> Result<()> {
        let text_document_caps = lsp_types::TextDocumentClientCapabilities {
            definition: Some(lsp_types::GotoCapability {
                dynamic_registration: Some(false),
                link_support: Some(true),
            }),
            synchronization: Some(lsp_types::TextDocumentSyncClientCapabilities {
                dynamic_registration: Some(false),
                will_save: Some(false),
                will_save_wait_until: Some(false),
                did_save: Some(false),
            }),
            ..Default::default()
        };

        let window_caps = lsp_types::WindowClientCapabilities {
            work_done_progress: Some(true),
            ..Default::default()
        };

        let capabilities = ClientCapabilities {
            text_document: Some(text_document_caps),
            window: Some(window_caps),
            ..Default::default()
        };

        let params = InitializeParams {
            capabilities,
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: self.inner.root_uri.clone(),
                name: "root".to_string(),
            }]),
            ..Default::default()
        };

        self.send_request_with_timeout(
            "initialize",
            serde_json::to_value(params)?,
            std::time::Duration::from_secs(300),
        ).await?;
        self.send_notification("initialized", serde_json::to_value(InitializedParams {})?)
            .await?;

        Ok(())
    }

    async fn open_file(&self, path: &Path, content: &str, language_id: &str) -> Result<()> {
        {
            let files = self.inner.opened_files.lock().await;
            if files.contains_key(path) {
                return Ok(());
            }
        }

        let uri = path_to_uri(path)?;
        let version = 1;

        {
            let mut files = self.inner.opened_files.lock().await;
            files.insert(path.to_path_buf(), version);
        }

        let params = DidOpenTextDocumentParams {
            text_document: lsp_types::TextDocumentItem {
                uri,
                language_id: language_id.to_string(),
                version,
                text: content.to_string(),
            },
        };

        self.send_notification("textDocument/didOpen", serde_json::to_value(params)?)
            .await
    }

    async fn goto_definition(
        &self,
        path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<lsp_types::Location>> {
        let uri = path_to_uri(path)?;

        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let result = self
            .send_request("textDocument/definition", serde_json::to_value(params)?)
            .await?;

        if result.is_null() {
            return Ok(None);
        }

        let response: GotoDefinitionResponse = serde_json::from_value(result)?;

        match response {
            GotoDefinitionResponse::Scalar(loc) => Ok(Some(loc)),
            GotoDefinitionResponse::Array(locs) => Ok(locs.into_iter().next()),
            GotoDefinitionResponse::Link(links) => {
                Ok(links.into_iter().next().map(|l| lsp_types::Location {
                    uri: l.target_uri,
                    range: l.target_selection_range,
                }))
            }
        }
    }

    async fn goto_implementation(
        &self,
        path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<lsp_types::Location>> {
        let uri = path_to_uri(path)?;

        let params = json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": line, "character": character }
        });

        let result = self
            .send_request("textDocument/implementation", params)
            .await?;

        if result.is_null() {
            return Ok(None);
        }

        let response: GotoDefinitionResponse = serde_json::from_value(result)?;

        match response {
            GotoDefinitionResponse::Scalar(loc) => Ok(Some(loc)),
            GotoDefinitionResponse::Array(locs) => Ok(locs.into_iter().next()),
            GotoDefinitionResponse::Link(links) => {
                Ok(links.into_iter().next().map(|l| lsp_types::Location {
                    uri: l.target_uri,
                    range: l.target_selection_range,
                }))
            }
        }
    }

    async fn hover(&self, path: &Path, line: u32, character: u32) -> Result<Option<String>> {
        let uri = path_to_uri(path)?;

        let params = json!({
            "textDocument": { "uri": uri.as_str() },
            "position": { "line": line, "character": character }
        });

        let result = self.send_request("textDocument/hover", params).await?;

        if result.is_null() {
            return Ok(None);
        }

        let hover: lsp_types::Hover = serde_json::from_value(result)?;

        let content = match hover.contents {
            lsp_types::HoverContents::Scalar(marked) => extract_marked_string(&marked),
            lsp_types::HoverContents::Array(arr) => arr
                .into_iter()
                .map(|m| extract_marked_string(&m))
                .collect::<Vec<_>>()
                .join("\n"),
            lsp_types::HoverContents::Markup(markup) => markup.value,
        };

        Ok(Some(content))
    }

    async fn shutdown(&self) -> Result<()> {
        let _ = self.send_request("shutdown", json!(null)).await;
        let _ = self.send_notification("exit", json!(null)).await;
        Ok(())
    }

    async fn wait_for_progress(&self, timeout: std::time::Duration) -> bool {
        let start = Instant::now();
        let mut stable_since: Option<Instant> = None;
        let stable_duration = std::time::Duration::from_millis(500);

        loop {
            let is_empty = {
                let progress = self.inner.active_progress.lock().await;
                progress.is_empty()
            };

            if is_empty {
                match stable_since {
                    Some(since) if since.elapsed() >= stable_duration => return true,
                    None => stable_since = Some(Instant::now()),
                    _ => {}
                }
            } else {
                stable_since = None;
            }

            if start.elapsed() > timeout {
                return false;
            }

            tokio::select! {
                _ = self.inner.progress_notify.notified() => {
                    stable_since = None;
                }
                _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
            }
        }
    }
}

fn is_ready(client: &AsyncLspClient) -> bool {
    client.inner.is_ready.load(Ordering::SeqCst)
}

fn set_ready(client: &AsyncLspClient, ready: bool) {
    client.inner.is_ready.store(ready, Ordering::SeqCst);
}

pub struct AsyncLspResolver {
    clients: HashMap<String, AsyncLspClient>,
    failed_servers: HashSet<String>,
    root: PathBuf,

    opened_files: HashSet<PathBuf>,
    stats: LspStats,
    timing: Arc<LspTimingStats>,
}

impl AsyncLspResolver {
    pub fn new(root: &Path) -> Self {
        Self {
            clients: HashMap::new(),
            failed_servers: HashSet::new(),
            root: root.to_path_buf(),

            opened_files: HashSet::new(),
            stats: LspStats::default(),
            timing: Arc::new(LspTimingStats::default()),
        }
    }

    pub fn stats(&self) -> &LspStats {
        &self.stats
    }

    pub fn timing_stats(&self) -> &LspTimingStats {
        &self.timing
    }

    fn language_id_for_ext(ext: &str) -> &'static str {
        language_id_for_ext(ext)
    }

    fn server_name_for_ext(&self, ext: &str) -> Option<String> {
        let registry = Registry::global();
        let lang_entry = registry.get_by_extension(ext)?;
        lang_entry.lsp.as_ref().map(|l| l.binary.clone())
    }

    fn get_server_stats(&mut self, server: &str) -> &mut LspServerStats {
        self.stats.by_server.entry(server.to_string()).or_default()
    }

    async fn get_or_create_client(&mut self, ext: &str) -> Result<&AsyncLspClient> {
        let registry = Registry::global();
        let lang_entry = registry
            .get_by_extension(ext)
            .with_context(|| format!("no language for extension: {}", ext))?;

        let lsp_config = lang_entry
            .lsp
            .as_ref()
            .with_context(|| format!("no LSP config for language: {}", lang_entry.name))?;

        let key = lsp_config.binary.clone();

        if self.failed_servers.contains(&key) {
            bail!("{} previously failed to initialize", key);
        }

        if !self.clients.contains_key(&key) {
            let client = match AsyncLspClient::new(lsp_config, &self.root).await {
                Ok(c) => {
                    if let Err(e) = c.initialize().await {
                        self.failed_servers.insert(key.clone());
                        warn!(server = %lsp_config.binary, error = ?e, "LSP initialization failed");
                        return Err(e);
                    }
                    c
                }
                Err(e) => {
                    self.failed_servers.insert(key.clone());
                    warn!(server = %lsp_config.binary, error = ?e, "LSP server failed to start");
                    return Err(e);
                }
            };

            self.clients.insert(key.clone(), client);
        }

        Ok(self.clients.get(&key).unwrap())
    }

    pub async fn resolve_calls_batch<F>(
        &mut self,
        calls: &[&Call],
        index: &Index,
        concurrency: usize,
        skip_hover: bool,
        mut on_progress: F,
    ) -> Vec<(usize, ResolvedCall)>
    where
        F: FnMut(&str, &Path, &str),
    {
        use futures::stream::{FuturesUnordered, StreamExt};
        use std::pin::Pin;

        let mut results = Vec::new();

        let mut requests_by_server: HashMap<String, Vec<(usize, &Call, PathBuf)>> = HashMap::new();

        for (i, call) in calls.iter().enumerate() {
            if should_skip_for_lsp(&call.file) {
                continue;
            }

            let ext = match call.file.extension().and_then(|e| e.to_str()) {
                Some(e) => e.to_string(),
                None => continue,
            };

            let server_name = match self.server_name_for_ext(&ext) {
                Some(s) => s,
                None => continue,
            };

            let abs_path = self.root.join(&call.file);

            requests_by_server
                .entry(server_name)
                .or_default()
                .push((i, *call, abs_path));
        }



        let opened_files: Arc<Mutex<HashSet<PathBuf>>> =
            Arc::new(Mutex::new(std::mem::take(&mut self.opened_files)));

        type FutResult = (
            usize,
            PathBuf,
            String,
            Option<lsp_types::Location>,
            Option<String>,
            Option<String>,
            String,
        );
        type BoxFuture = Pin<Box<dyn std::future::Future<Output = Option<FutResult>> + Send>>;

        for (server_name, server_calls) in requests_by_server {
            let ext = match server_calls.first() {
                Some((_, call, _)) => call
                    .file
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_string(),
                None => continue,
            };

            let client = match self.get_or_create_client(&ext).await {
                Ok(c) => c.clone(),
                Err(_) => continue,
            };

            let language_id = Self::language_id_for_ext(&ext);
            let file_count = server_calls.len();
            info!(
                server = %server_name,
                language = %language_id,
                file_count,
                "resolving calls via LSP (lazy loading)"
            );



            if !is_ready(&client) {
                info!(server = %server_name, "waiting for LSP to index project");
                let timeout = std::time::Duration::from_secs(300);
                let t_wait = Instant::now();
                if client.wait_for_progress(timeout).await {
                    info!(server = %server_name, "LSP ready");
                } else {
                    warn!(server = %server_name, "LSP still indexing, proceeding anyway");
                }
                self.timing.add_wait_ready(t_wait.elapsed().as_millis() as u64);
                set_ready(&client, true);
            }

            info!(
                server = %server_name,
                call_count = server_calls.len(),
                "resolving calls via LSP"
            );

            let mut in_flight: FuturesUnordered<BoxFuture> = FuturesUnordered::new();

            for (call_idx, call, abs_path) in server_calls {
                while in_flight.len() >= concurrency {
                    if let Some(result) = in_flight.next().await {
                        if let Some(ref r) = result {
                            on_progress(&r.6, &r.1, &r.2);
                        }
                        if let Some(r) = result {
                            self.process_resolution_result(r, index, &mut results);
                        }
                    }
                }

                let client_clone = client.clone();
                let abs_path_clone = abs_path.clone();
                let callee = call.callee.clone();
                let qualifier = call.qualifier.clone();
                let timing = self.timing.clone();
                let server_name_clone = server_name.clone();
                let start_line_idx = call.span.start_line.saturating_sub(1);
                let opened_files_clone = opened_files.clone();
                let language_id_owned = language_id.to_string();

                let fut: BoxFuture = Box::pin(async move {
                    let path_for_read = abs_path_clone.clone();
                    let content: String = match tokio::task::spawn_blocking(move || {
                        std::fs::read_to_string(&path_for_read)
                    })
                    .await
                    {
                        Ok(Ok(c)) => c,
                        _ => return None,
                    };

                    {
                        let mut opened = opened_files_clone.lock().await;
                        if !opened.contains(&abs_path_clone) {
                            let _ = client_clone
                                .open_file(&abs_path_clone, &content, &language_id_owned)
                                .await;
                            opened.insert(abs_path_clone.clone());
                        }
                    }

                    let lines: Vec<&str> = content.lines().collect();
                    if start_line_idx >= lines.len() {
                        return None;
                    }

                    let line_content = lines[start_line_idx];
                    let col = line_content.find(&callee).unwrap_or(0) as u32;

                    let (signature, receiver_type) = if skip_hover {
                        (None, None)
                    } else {
                        let t_hover = Instant::now();
                        let sig = client_clone
                            .hover(&abs_path_clone, start_line_idx as u32, col)
                            .await
                            .ok()
                            .flatten()
                            .and_then(|h| extract_signature(&h));
                        timing.add_hover(t_hover.elapsed().as_millis() as u64);

                        let recv = if let Some(ref q) = qualifier {
                            if let Some(qualifier_col) = line_content.find(q.as_str()) {
                                let t_hover2 = Instant::now();
                                let result = client_clone
                                    .hover(
                                        &abs_path_clone,
                                        start_line_idx as u32,
                                        qualifier_col as u32,
                                    )
                                    .await
                                    .ok()
                                    .flatten()
                                    .and_then(|h| extract_type(&h));
                                timing.add_hover(t_hover2.elapsed().as_millis() as u64);
                                result
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        (sig, recv)
                    };

                    let t_goto = Instant::now();
                    let mut location = client_clone
                        .goto_definition(&abs_path_clone, start_line_idx as u32, col)
                        .await
                        .ok()
                        .flatten();
                    timing.add_goto_definition(t_goto.elapsed().as_millis() as u64);

                    if let Some(loc) = location.take() {
                        let uri_str = loc.uri.as_str();
                        let is_declaration_file = is_declaration_file_uri(uri_str);

                        if is_declaration_file {
                            let t_decl = Instant::now();
                            if let Some(decl_path) = uri_to_path(&loc.uri) {
                                if let Ok(decl_content) = std::fs::read_to_string(&decl_path) {
                                    let ext = decl_path
                                        .extension()
                                        .and_then(|e| e.to_str())
                                        .unwrap_or("");
                                    let lang_id = language_id_for_ext(ext);
                                    let _ = client_clone
                                        .open_file(&decl_path, &decl_content, lang_id)
                                        .await;

                                    let decl_line = loc.range.start.line;
                                    let decl_char = loc.range.start.character;

                                    if let Ok(Some(impl_loc)) = client_clone
                                        .goto_implementation(&decl_path, decl_line, decl_char)
                                        .await
                                    {
                                        location = Some(impl_loc);
                                    } else if let Ok(Some(def_loc)) = client_clone
                                        .goto_definition(&decl_path, decl_line, decl_char)
                                        .await
                                    {
                                        if !is_declaration_file_uri(def_loc.uri.as_str()) {
                                            location = Some(def_loc);
                                        } else {
                                            location = Some(loc);
                                        }
                                    } else {
                                        location = Some(loc);
                                    }
                                } else {
                                    location = Some(loc);
                                }
                            } else {
                                location = Some(loc);
                            }
                            timing.add_declaration_chase(t_decl.elapsed().as_millis() as u64);
                        } else {
                            location = Some(loc);
                        }
                    }

                    Some((
                        call_idx,
                        abs_path_clone,
                        callee,
                        location,
                        signature,
                        receiver_type,
                        server_name_clone,
                    ))
                });

                in_flight.push(fut);
            }

            while let Some(result) = in_flight.next().await {
                if let Some(ref r) = result {
                    on_progress(&r.6, &r.1, &r.2);
                }
                if let Some(r) = result {
                    self.process_resolution_result(r, index, &mut results);
                }
            }
        }

        self.opened_files = Arc::try_unwrap(opened_files)
            .map(|m| m.into_inner())
            .unwrap_or_else(|arc| arc.blocking_lock().clone());

        results
    }

    fn process_resolution_result(
        &mut self,
        result: (
            usize,
            PathBuf,
            String,
            Option<lsp_types::Location>,
            Option<String>,
            Option<String>,
            String,
        ),
        index: &Index,
        results: &mut Vec<(usize, ResolvedCall)>,
    ) {
        let (call_idx, _file, callee, location, signature, receiver_type, server_name) = result;

        let location = match location {
            Some(loc) => loc,
            None => {
                trace!(callee = %callee, "no definition found");
                self.get_server_stats(&server_name).no_definition += 1;
                return;
            }
        };

        let def_path = match uri_to_path(&location.uri) {
            Some(p) => p,
            None => return,
        };

        let rel_path = match def_path.strip_prefix(&self.root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => {
                trace!(callee = %callee, path = %def_path.display(), "definition is external");
                self.get_server_stats(&server_name).external += 1;
                return;
            }
        };

        let start_line = location.range.start.line as usize + 1;
        let end_line = location.range.end.line as usize + 1;

        let record = match index.get(&rel_path) {
            Some(r) => r,
            None => {
                trace!(callee = %callee, path = %rel_path.display(), "definition file not indexed");
                self.get_server_stats(&server_name).not_indexed += 1;
                return;
            }
        };

        let def = match record
            .definitions
            .iter()
            .find(|d| d.span.start_line <= start_line && d.span.end_line >= end_line)
        {
            Some(d) => d,
            None => {
                self.get_server_stats(&server_name).no_match += 1;
                return;
            }
        };

        self.get_server_stats(&server_name).resolved += 1;
        results.push((
            call_idx,
            ResolvedCall {
                target_file: rel_path,
                target_name: def.name.clone(),
                target_span: def.span.clone(),
                signature,
                receiver_type,
            },
        ));
    }

    pub async fn shutdown_all(&mut self) {
        for (name, client) in self.clients.drain() {
            if let Err(e) = client.shutdown().await {
                debug!(server = %name, error = ?e, "error shutting down LSP");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_target() {
        let target = current_target();
        assert!(!target.is_empty());
        assert_ne!(target, "unknown");
    }

    #[test]
    fn test_lsp_binary_path() {
        let lsp = LspConfig {
            binary: "rust-analyzer".to_string(),
            args: vec![],
            version: None,
            url_template: None,
            archive: None,
            binary_path: None,
            targets: HashMap::new(),
            npm_package: None,
            go_package: None,
            cargo_crate: None,
            latest_txt_url: None,
        };

        let path = lsp_binary_path(&lsp);
        assert!(path.to_string_lossy().contains("rust-analyzer"));
        assert!(path.to_string_lossy().contains("lsp"));
    }

    #[test]
    fn test_language_id_for_ext() {
        assert_eq!(language_id_for_ext("rs"), "rust");
        assert_eq!(language_id_for_ext("ts"), "typescript");
        assert_eq!(language_id_for_ext("py"), "python");
        assert_eq!(language_id_for_ext("go"), "go");
        assert_eq!(language_id_for_ext("c"), "c");
        assert_eq!(language_id_for_ext("cpp"), "cpp");
    }

    #[test]
    fn test_check_lsp_availability() {
        let availability = check_lsp_availability();
        assert!(!availability.is_empty());
    }
}

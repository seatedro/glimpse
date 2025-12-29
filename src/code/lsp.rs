use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use lsp_types::{
    ClientCapabilities, DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse,
    InitializeParams, InitializedParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkspaceFolder,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::grammar::{lsp_dir, LspConfig, Registry};
use super::index::{Call, Definition, Index};

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

fn detect_zig_version_from_zon(root: &Path) -> Option<String> {
    let zon_path = root.join("build.zig.zon");
    let content = fs::read_to_string(zon_path).ok()?;
    let re = regex::Regex::new(r#"\.minimum_zig_version\s*=\s*"([^"]+)""#).ok()?;
    let caps = re.captures(&content)?;
    Some(caps.get(1)?.as_str().to_string())
}

fn detect_zig_version(root: &Path) -> Option<String> {
    if let Ok(output) = Command::new("zig").arg("version").output() {
        if output.status.success() {
            let version_str = String::from_utf8_lossy(&output.stdout);
            let version = version_str.trim();
            if let Some(base) = version.split('-').next() {
                return Some(base.to_string());
            }
        }
    }

    detect_zig_version_from_zon(root)
}

fn download_and_extract(lsp: &LspConfig, root: &Path) -> Result<PathBuf> {
    let Some(ref url_template) = lsp.url_template else {
        bail!("no download URL configured for {}", lsp.binary);
    };

    let version = if lsp.binary == "zls" {
        detect_zig_version(root).with_context(|| {
            "failed to detect zig version. Install zig or install zls manually"
        })?
    } else {
        lsp.version
            .clone()
            .with_context(|| format!("no version configured for {}", lsp.binary))?
    };

    let target = current_target();
    let Some(target_name) = lsp.targets.get(target) else {
        bail!(
            "no pre-built binary available for {} on {}",
            lsp.binary,
            target
        );
    };

    let url = url_template
        .replace("{version}", &version)
        .replace("{target}", target_name);

    eprintln!("Downloading {} from {}...", lsp.binary, url);

    let dir = lsp_dir();
    fs::create_dir_all(&dir)?;

    let response =
        reqwest::blocking::get(&url).with_context(|| format!("failed to download {}", url))?;

    if !response.status().is_success() {
        bail!("download failed with status: {}", response.status());
    }

    let bytes = response.bytes()?;
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

    eprintln!("Installed {} to {}", lsp.binary, final_path.display());
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

    let pkg_dir = lsp_dir().join(&lsp.binary);
    fs::create_dir_all(&pkg_dir)?;

    eprintln!("Installing {} via {} (local)...", package, pkg_manager);

    let init_status = Command::new(&pkg_manager_path)
        .args(["init", "--yes"])
        .current_dir(&pkg_dir)
        .status()
        .with_context(|| format!("failed to run {} init", pkg_manager))?;

    if !init_status.success() {
        bail!("{} init failed", pkg_manager);
    }

    let packages: Vec<&str> = package.split_whitespace().collect();
    let mut install_args = vec!["install"];
    install_args.extend(packages.iter());

    let install_status = Command::new(&pkg_manager_path)
        .args(&install_args)
        .current_dir(&pkg_dir)
        .status()
        .with_context(|| format!("failed to run {} install", pkg_manager))?;

    if !install_status.success() {
        bail!("{} install failed for {}", pkg_manager, package);
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

    eprintln!("Installed {} to {}", lsp.binary, wrapper_path.display());
    Ok(wrapper_path)
}

fn create_wrapper_script(wrapper_path: &Path, target_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let script = format!(
            "#!/bin/sh\nexec \"{}\" \"$@\"\n",
            target_path.display()
        );
        fs::write(wrapper_path, script)?;

        let mut perms = fs::metadata(wrapper_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(wrapper_path, perms)?;
    }

    #[cfg(windows)]
    {
        let script = format!(
            "@echo off\r\n\"{}\" %*\r\n",
            target_path.display()
        );
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

    let install_dir = lsp_dir();
    fs::create_dir_all(&install_dir)?;

    eprintln!("Installing {} via go install...", package);

    let status = Command::new(&go_path)
        .args(["install", package])
        .env("GOBIN", &install_dir)
        .status()
        .context("failed to run go install")?;

    if !status.success() {
        bail!("go install failed for {}", package);
    }

    let binary_path = install_dir.join(&lsp.binary);
    if binary_path.exists() {
        eprintln!("Installed {} to {}", lsp.binary, binary_path.display());
        return Ok(binary_path);
    }

    bail!(
        "go install succeeded but binary {} not found at {}",
        lsp.binary,
        binary_path.display()
    );
}

fn find_lsp_binary(lsp: &LspConfig, root: &Path) -> Result<PathBuf> {
    let local_path = lsp_binary_path(lsp);
    if local_path.exists() {
        return Ok(local_path);
    }

    if let Ok(system_path) = which::which(&lsp.binary) {
        return Ok(system_path);
    }

    if lsp.url_template.is_some() {
        return download_and_extract(lsp, root);
    }

    if lsp.npm_package.is_some() {
        return install_npm_package(lsp);
    }

    if lsp.go_package.is_some() {
        return install_go_package(lsp);
    }

    bail!(
        "LSP server '{}' not found. Install it manually.",
        lsp.binary
    );
}

#[derive(Debug)]
struct LspClient {
    process: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    request_id: AtomicI32,
    root_uri: Uri,
    opened_files: HashMap<PathBuf, i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LspMessage {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

impl LspClient {
    fn new(lsp: &LspConfig, root: &Path) -> Result<Self> {
        let binary_path = find_lsp_binary(lsp, root)?;

        let mut process = Command::new(&binary_path)
            .args(&lsp.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn {}", binary_path.display()))?;

        let stdin = process.stdin.take().context("failed to get stdin")?;
        let stdout = process.stdout.take().context("failed to get stdout")?;

        let root_uri = path_to_uri(&root.canonicalize().unwrap_or_else(|_| root.to_path_buf()))?;

        Ok(Self {
            process,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            request_id: AtomicI32::new(1),
            root_uri,
            opened_files: HashMap::new(),
        })
    }

    fn next_id(&self) -> i32 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    fn send_message(&mut self, msg: &LspMessage) -> Result<()> {
        let content = serde_json::to_string(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin.write_all(header.as_bytes())?;
        self.stdin.write_all(content.as_bytes())?;
        self.stdin.flush()?;

        Ok(())
    }

    fn read_message(&mut self) -> Result<LspMessage> {
        let mut content_length: Option<usize> = None;
        let mut header_line = String::new();

        loop {
            header_line.clear();
            self.stdout.read_line(&mut header_line)?;

            if header_line == "\r\n" || header_line.is_empty() {
                break;
            }

            if let Some(len_str) = header_line.strip_prefix("Content-Length: ") {
                content_length = Some(len_str.trim().parse()?);
            }
        }

        let len = content_length.context("missing Content-Length header")?;
        let mut body = vec![0u8; len];
        self.stdout.read_exact(&mut body)?;

        let msg: LspMessage = serde_json::from_slice(&body)?;
        Ok(msg)
    }

    fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id();
        let msg = LspMessage {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        };

        self.send_message(&msg)?;

        loop {
            let response = self.read_message()?;

            if response.id == Some(id) {
                if let Some(error) = response.error {
                    bail!("LSP error: {}", error);
                }
                return Ok(response.result.unwrap_or(Value::Null));
            }
        }
    }

    fn send_notification(&mut self, method: &str, params: Value) -> Result<()> {
        let msg = LspMessage {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        };

        self.send_message(&msg)
    }

    fn wait_for_ready(&mut self, path: &Path, max_attempts: u32) -> Result<bool> {
        use std::thread;
        use std::time::Duration;

        let uri = path_to_uri(path)?;

        // First wait for basic syntax analysis (documentSymbol)
        for _ in 0..10 {
            let params = lsp_types::DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };

            match self.send_request("textDocument/documentSymbol", serde_json::to_value(params)?) {
                Ok(Value::Array(arr)) if !arr.is_empty() => break,
                _ => thread::sleep(Duration::from_millis(200)),
            }
        }

        // Then wait for semantic analysis (hover on a known symbol)
        // This indicates rust-analyzer has finished loading the project
        for attempt in 0..max_attempts {
            let hover_params = json!({
                "textDocument": { "uri": uri.as_str() },
                "position": { "line": 0, "character": 4 }  // "mod" keyword
            });

            match self.send_request("textDocument/hover", hover_params) {
                Ok(result) if !result.is_null() => return Ok(true),
                _ => {}
            }

            if attempt < max_attempts - 1 {
                thread::sleep(Duration::from_millis(500));
            }
        }

        Ok(false)
    }

    fn initialize(&mut self) -> Result<()> {
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

        let capabilities = ClientCapabilities {
            text_document: Some(text_document_caps),
            ..Default::default()
        };

        let params = InitializeParams {
            root_uri: Some(self.root_uri.clone()),
            capabilities,
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: self.root_uri.clone(),
                name: "root".to_string(),
            }]),
            ..Default::default()
        };

        self.send_request("initialize", serde_json::to_value(params)?)?;
        self.send_notification("initialized", serde_json::to_value(InitializedParams {})?)?;

        Ok(())
    }

    fn open_file(&mut self, path: &Path, content: &str, language_id: &str) -> Result<()> {
        if self.opened_files.contains_key(path) {
            return Ok(());
        }

        let uri = path_to_uri(path)?;

        let version = 1;
        self.opened_files.insert(path.to_path_buf(), version);

        let params = DidOpenTextDocumentParams {
            text_document: lsp_types::TextDocumentItem {
                uri,
                language_id: language_id.to_string(),
                version,
                text: content.to_string(),
            },
        };

        self.send_notification("textDocument/didOpen", serde_json::to_value(params)?)
    }

    fn goto_definition(
        &mut self,
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

        let result = self.send_request("textDocument/definition", serde_json::to_value(params)?)?;

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

    fn shutdown(&mut self) -> Result<()> {
        self.send_request("shutdown", json!(null))?;
        self.send_notification("exit", json!(null))?;
        let _ = self.process.wait();
        Ok(())
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

pub struct LspResolver {
    clients: HashMap<String, LspClient>,
    root: PathBuf,
    file_cache: HashMap<PathBuf, String>,
}

impl LspResolver {
    pub fn new(root: &Path) -> Self {
        Self {
            clients: HashMap::new(),
            root: root.to_path_buf(),
            file_cache: HashMap::new(),
        }
    }

    fn get_or_create_client(&mut self, ext: &str) -> Result<&mut LspClient> {
        let registry = Registry::global();
        let lang_entry = registry
            .get_by_extension(ext)
            .with_context(|| format!("no language for extension: {}", ext))?;

        let lsp_config = lang_entry
            .lsp
            .as_ref()
            .with_context(|| format!("no LSP config for language: {}", lang_entry.name))?;

        let key = lsp_config.binary.clone();

        if !self.clients.contains_key(&key) {
            let mut client = LspClient::new(lsp_config, &self.root)?;
            client.initialize()?;
            self.clients.insert(key.clone(), client);
        }

        Ok(self.clients.get_mut(&key).unwrap())
    }

    fn read_file(&mut self, path: &Path) -> Result<String> {
        if let Some(content) = self.file_cache.get(path) {
            return Ok(content.clone());
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        self.file_cache.insert(path.to_path_buf(), content.clone());
        Ok(content)
    }

    fn language_id_for_ext(ext: &str) -> &'static str {
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

    pub fn resolve_call(&mut self, call: &Call, index: &Index) -> Option<Definition> {
        let ext = call.file.extension()?.to_str()?.to_string();
        let abs_path = self.root.join(&call.file);
        let language_id = Self::language_id_for_ext(&ext);
        let callee = call.callee.clone();
        let start_line_idx = call.span.start_line.saturating_sub(1);

        let content = self.read_file(&abs_path).ok()?;

        let lines: Vec<&str> = content.lines().collect();
        if start_line_idx >= lines.len() {
            return None;
        }

        let line_content = lines[start_line_idx];
        let col = line_content.find(&callee).unwrap_or(0) as u32;

        let client = self.get_or_create_client(&ext).ok()?;

        if client.open_file(&abs_path, &content, language_id).is_err() {
            return None;
        }

        let location = client
            .goto_definition(&abs_path, start_line_idx as u32, col)
            .ok()??;

        let def_path = uri_to_path(&location.uri)?;
        let root = self.root.clone();
        let rel_path = def_path.strip_prefix(&root).ok()?.to_path_buf();

        let start_line = location.range.start.line as usize + 1;
        let end_line = location.range.end.line as usize + 1;

        let record = index.get(&rel_path)?;
        record
            .definitions
            .iter()
            .find(|d| d.span.start_line <= start_line && d.span.end_line >= end_line)
            .cloned()
    }

    pub fn resolve_calls_batch(
        &mut self,
        calls: &[&Call],
        index: &Index,
    ) -> HashMap<usize, Definition> {
        let mut results = HashMap::new();

        for (i, call) in calls.iter().enumerate() {
            if let Some(def) = self.resolve_call(call, index) {
                results.insert(i, def);
            }
        }

        results
    }
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
        };

        let path = lsp_binary_path(&lsp);
        assert!(path.to_string_lossy().contains("rust-analyzer"));
        assert!(path.to_string_lossy().contains("lsp"));
    }

    #[test]
    fn test_language_id_for_ext() {
        assert_eq!(LspResolver::language_id_for_ext("rs"), "rust");
        assert_eq!(LspResolver::language_id_for_ext("ts"), "typescript");
        assert_eq!(LspResolver::language_id_for_ext("py"), "python");
        assert_eq!(LspResolver::language_id_for_ext("go"), "go");
        assert_eq!(LspResolver::language_id_for_ext("c"), "c");
        assert_eq!(LspResolver::language_id_for_ext("cpp"), "cpp");
    }

    #[test]
    fn test_check_lsp_availability() {
        let availability = check_lsp_availability();
        assert!(!availability.is_empty());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::env;
    use std::thread;
    use std::time::Duration;

    #[test]
    #[ignore] // Run with: cargo test --release -- --ignored test_lsp_client_rust
    fn test_lsp_client_rust() {
        let root = env::current_dir().expect("failed to get current dir");
        let registry = Registry::global();
        let rust_entry = registry.get("rust").expect("rust not in registry");
        let lsp_config = rust_entry.lsp.as_ref().expect("rust has no LSP config");

        let mut client = LspClient::new(lsp_config, &root).expect("failed to create LSP client");
        client.initialize().expect("failed to initialize LSP");

        let test_file = root.join("src/main.rs");
        let content = fs::read_to_string(&test_file).expect("failed to read test file");

        client
            .open_file(&test_file, &content, "rust")
            .expect("failed to open file");

        client
            .wait_for_ready(&test_file, 30)
            .expect("wait_for_ready failed");

        // Line 61: ".filter(|path| is_url_or_git(path))"
        let line = content.lines().nth(60).unwrap();
        let col = line.find("is_url_or_git").unwrap_or(0);

        // Retry a few times in case of "content modified" errors
        for _ in 0..5 {
            match client.goto_definition(&test_file, 60, col as u32) {
                Ok(Some(loc)) => {
                    let path = uri_to_path(&loc.uri).expect("invalid uri");
                    assert!(path.ends_with("main.rs"));
                    assert_eq!(loc.range.start.line, 25); // fn is_url_or_git definition
                    return;
                }
                Ok(None) | Err(_) => thread::sleep(Duration::from_secs(2)),
            }
        }
        panic!("Failed to resolve definition after all attempts");
    }
}

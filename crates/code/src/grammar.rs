use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use anyhow::{bail, Context, Result};
use git2::Repository;
use libloading::{Library, Symbol};
use once_cell::sync::Lazy;
use serde::Deserialize;
use tree_sitter::ffi::TSLanguage;
use tree_sitter::Language;

type LanguageFn = unsafe extern "C" fn() -> *const TSLanguage;

static REGISTRY: OnceLock<Registry> = OnceLock::new();
static LOADED_LANGUAGES: Lazy<Mutex<HashMap<String, Language>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static LOADED_LIBRARIES: Lazy<Mutex<Vec<Library>>> = Lazy::new(|| Mutex::new(Vec::new()));

#[derive(Debug, Clone, Deserialize)]
pub struct LanguageEntry {
    pub name: String,
    pub extensions: Vec<String>,
    pub repo: String,
    pub branch: String,
    pub symbol: String,
    pub subpath: Option<String>,
    pub definition_query: String,
    pub call_query: String,
    pub import_query: String,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    language: Vec<LanguageEntry>,
}

pub struct Registry {
    languages: Vec<LanguageEntry>,
    by_name: HashMap<String, usize>,
    by_extension: HashMap<String, usize>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        let registry_toml = include_str!("../../../registry.toml");
        Self::from_str(registry_toml)
    }

    fn from_str(content: &str) -> Result<Self> {
        let file: RegistryFile =
            toml::from_str(content).context("failed to parse registry.toml")?;

        let mut by_name = HashMap::new();
        let mut by_extension = HashMap::new();

        for (idx, lang) in file.language.iter().enumerate() {
            by_name.insert(lang.name.clone(), idx);
            for ext in &lang.extensions {
                by_extension.insert(ext.clone(), idx);
            }
        }

        Ok(Self {
            languages: file.language,
            by_name,
            by_extension,
        })
    }

    pub fn global() -> &'static Registry {
        REGISTRY.get_or_init(|| Self::load().expect("failed to load registry"))
    }

    pub fn get(&self, name: &str) -> Option<&LanguageEntry> {
        self.by_name.get(name).map(|&idx| &self.languages[idx])
    }

    pub fn get_by_extension(&self, ext: &str) -> Option<&LanguageEntry> {
        self.by_extension.get(ext).map(|&idx| &self.languages[idx])
    }

    pub fn languages(&self) -> &[LanguageEntry] {
        &self.languages
    }
}

fn sources_dir() -> PathBuf {
    cache_dir().join("sources")
}

pub fn fetch_grammar(lang: &LanguageEntry) -> Result<PathBuf> {
    let sources = sources_dir();
    fs::create_dir_all(&sources)?;

    let dest = sources.join(&lang.name);

    if dest.exists() {
        return Ok(dest);
    }

    Repository::clone(&lang.repo, &dest)
        .with_context(|| format!("failed to clone grammar repo: {}", lang.repo))?;

    let repo = Repository::open(&dest)?;
    let (object, reference) = repo.revparse_ext(&lang.branch)?;
    repo.checkout_tree(&object, None)?;
    match reference {
        Some(r) => repo.set_head(r.name().unwrap())?,
        None => repo.set_head_detached(object.id())?,
    }

    Ok(dest)
}

pub fn compile_grammar(lang: &LanguageEntry, grammar_dir: &Path) -> Result<PathBuf> {
    let output_dir = cache_dir();
    fs::create_dir_all(&output_dir)?;

    let lib_name = format!("tree-sitter-{}", lang.name);
    let output_path = output_dir.join(lib_filename(&lib_name));

    if output_path.exists() {
        return Ok(output_path);
    }

    let src_dir = match &lang.subpath {
        Some(subpath) => grammar_dir.join(subpath).join("src"),
        None => grammar_dir.join("src"),
    };

    let parser_c = src_dir.join("parser.c");
    if !parser_c.exists() {
        bail!("parser.c not found at: {}", parser_c.display());
    }

    let temp_dir = tempfile::tempdir()?;
    let mut objects = Vec::new();

    objects.push(compile_c_file(&parser_c, &src_dir, temp_dir.path())?);

    let scanner_c = src_dir.join("scanner.c");
    if scanner_c.exists() {
        objects.push(compile_c_file(&scanner_c, &src_dir, temp_dir.path())?);
    }

    let scanner_cc = src_dir.join("scanner.cc");
    if scanner_cc.exists() {
        objects.push(compile_cpp_file(&scanner_cc, &src_dir, temp_dir.path())?);
    }

    link_shared_library(&objects, &output_path)?;

    Ok(output_path)
}

fn compile_c_file(source: &Path, include_dir: &Path, out_dir: &Path) -> Result<PathBuf> {
    let obj_name = source.file_stem().unwrap().to_string_lossy();
    let obj_path = out_dir.join(format!("{}.o", obj_name));

    let status = Command::new("cc")
        .args(["-c", "-O3", "-fPIC", "-w"])
        .arg("-I")
        .arg(include_dir)
        .arg("-o")
        .arg(&obj_path)
        .arg(source)
        .status()
        .context("failed to run cc")?;

    if !status.success() {
        bail!("failed to compile: {}", source.display());
    }

    Ok(obj_path)
}

fn compile_cpp_file(source: &Path, include_dir: &Path, out_dir: &Path) -> Result<PathBuf> {
    let obj_name = source.file_stem().unwrap().to_string_lossy();
    let obj_path = out_dir.join(format!("{}.o", obj_name));

    let status = Command::new("c++")
        .args(["-c", "-O3", "-fPIC", "-w"])
        .arg("-I")
        .arg(include_dir)
        .arg("-o")
        .arg(&obj_path)
        .arg(source)
        .status()
        .context("failed to run c++")?;

    if !status.success() {
        bail!("failed to compile: {}", source.display());
    }

    Ok(obj_path)
}

fn link_shared_library(objects: &[PathBuf], output: &Path) -> Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("cc");
        c.args(["-dynamiclib", "-undefined", "dynamic_lookup"]);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("link");
        c.arg("/DLL");
        c
    } else {
        let mut c = Command::new("cc");
        c.arg("-shared");
        c
    };

    for obj in objects {
        cmd.arg(obj);
    }

    if cfg!(target_os = "windows") {
        cmd.arg(format!("/OUT:{}", output.display()));
    } else {
        cmd.arg("-o").arg(output);
    }

    let status = cmd.status().context("failed to link shared library")?;

    if !status.success() {
        bail!("failed to link shared library: {}", output.display());
    }

    Ok(())
}

fn lib_filename(name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{}.dylib", name)
    } else if cfg!(target_os = "windows") {
        format!("{}.dll", name)
    } else {
        format!("lib{}.so", name)
    }
}

pub fn cache_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glimpse")
        .join("grammars")
}

pub fn load_language(name: &str) -> Result<Language> {
    {
        let cache = LOADED_LANGUAGES.lock().unwrap();
        if let Some(lang) = cache.get(name) {
            return Ok(lang.clone());
        }
    }

    let registry = Registry::global();
    let entry = registry
        .get(name)
        .with_context(|| format!("unknown language: {}", name))?;

    load_language_entry(entry)
}

pub fn load_language_by_extension(ext: &str) -> Result<Language> {
    let registry = Registry::global();
    let entry = registry
        .get_by_extension(ext)
        .with_context(|| format!("no language for extension: {}", ext))?;

    {
        let cache = LOADED_LANGUAGES.lock().unwrap();
        if let Some(lang) = cache.get(&entry.name) {
            return Ok(lang.clone());
        }
    }

    load_language_entry(entry)
}

fn load_language_entry(entry: &LanguageEntry) -> Result<Language> {
    let lib_path = compiled_lib_path(entry);

    if !lib_path.exists() {
        let grammar_dir = fetch_grammar(entry)?;
        compile_grammar(entry, &grammar_dir)?;
    }

    if !lib_path.exists() {
        bail!("compiled grammar not found: {}", lib_path.display());
    }

    let language = unsafe { load_language_from_lib(&lib_path, &entry.symbol) }?;

    {
        let mut cache = LOADED_LANGUAGES.lock().unwrap();
        cache.insert(entry.name.clone(), language.clone());
    }

    Ok(language)
}

fn compiled_lib_path(entry: &LanguageEntry) -> PathBuf {
    let lib_name = format!("tree-sitter-{}", entry.name);
    cache_dir().join(lib_filename(&lib_name))
}

unsafe fn load_language_from_lib(lib_path: &PathBuf, symbol: &str) -> Result<Language> {
    let lib = Library::new(lib_path)
        .with_context(|| format!("failed to load library: {}", lib_path.display()))?;

    let func: Symbol<LanguageFn> = lib
        .get(symbol.as_bytes())
        .with_context(|| format!("symbol not found: {}", symbol))?;

    let lang_ptr = func();
    let language = Language::from_raw(lang_ptr);

    LOADED_LIBRARIES.lock().unwrap().push(lib);

    Ok(language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_registry() {
        let registry = Registry::load().expect("failed to load registry");
        assert!(!registry.languages.is_empty());
    }

    #[test]
    fn test_get_rust() {
        let registry = Registry::load().unwrap();
        let rust = registry.get("rust").expect("rust language not found");
        assert_eq!(rust.name, "rust");
        assert!(rust.extensions.contains(&"rs".to_string()));
        assert_eq!(rust.symbol, "tree_sitter_rust");
    }

    #[test]
    fn test_get_by_extension() {
        let registry = Registry::load().unwrap();
        let rust = registry
            .get_by_extension("rs")
            .expect("rs extension not found");
        assert_eq!(rust.name, "rust");
    }

    #[test]
    fn test_typescript_subpath() {
        let registry = Registry::load().unwrap();
        let ts = registry.get("typescript").expect("typescript not found");
        assert_eq!(ts.subpath, Some("typescript".to_string()));
    }

    #[test]
    fn test_all_languages_have_queries() {
        let registry = Registry::load().unwrap();
        for lang in registry.languages() {
            assert!(!lang.definition_query.is_empty(), "{} missing definition_query", lang.name);
            assert!(!lang.call_query.is_empty(), "{} missing call_query", lang.name);
            assert!(!lang.import_query.is_empty(), "{} missing import_query", lang.name);
        }
    }

    #[test]
    fn test_cache_dir() {
        let dir = cache_dir();
        assert!(dir.to_string_lossy().contains("glimpse"));
        assert!(dir.ends_with("grammars"));
    }

    #[test]
    fn test_sources_dir() {
        let dir = sources_dir();
        assert!(dir.ends_with("grammars/sources"));
    }

    #[test]
    fn test_lib_filename() {
        let name = "tree-sitter-rust";
        let filename = lib_filename(name);
        if cfg!(target_os = "macos") {
            assert_eq!(filename, "libtree-sitter-rust.dylib");
        } else if cfg!(target_os = "windows") {
            assert_eq!(filename, "tree-sitter-rust.dll");
        } else {
            assert_eq!(filename, "libtree-sitter-rust.so");
        }
    }

    #[test]
    fn test_compiled_lib_path() {
        let registry = Registry::global();
        let rust = registry.get("rust").unwrap();
        let path = compiled_lib_path(rust);
        assert!(path.to_string_lossy().contains("tree-sitter-rust"));
    }

    #[test]
    #[ignore]
    fn test_load_rust_grammar() {
        let language = load_language("rust").expect("failed to load rust grammar");
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).expect("failed to set language");

        let source = "fn main() { println!(\"Hello\"); }";
        let tree = parser.parse(source, None).expect("failed to parse");
        let root = tree.root_node();

        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0);
    }

    #[test]
    #[ignore]
    fn test_load_by_extension() {
        let language = load_language_by_extension("rs").expect("failed to load by extension");
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).expect("failed to set language");

        let tree = parser.parse("fn foo() {}", None).expect("failed to parse");
        assert_eq!(tree.root_node().kind(), "source_file");
    }

    #[test]
    #[ignore]
    fn test_language_caching() {
        let lang1 = load_language("rust").expect("first load failed");
        let lang2 = load_language("rust").expect("second load failed");
        assert_eq!(lang1.node_kind_count(), lang2.node_kind_count());
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use libloading::{Library, Symbol};
use once_cell::sync::Lazy;
use tree_sitter::ffi::TSLanguage;
use tree_sitter::Language;

use super::compile::{compile_grammar, fetch_grammar};
use super::registry::{LanguageEntry, Registry};

type LanguageFn = unsafe extern "C" fn() -> *const TSLanguage;

static LOADED_LANGUAGES: Lazy<Mutex<HashMap<String, Language>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static LOADED_LIBRARIES: Lazy<Mutex<Vec<Library>>> = Lazy::new(|| Mutex::new(Vec::new()));

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

fn lib_filename(name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{}.dylib", name)
    } else if cfg!(target_os = "windows") {
        format!("{}.dll", name)
    } else {
        format!("lib{}.so", name)
    }
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
    fn test_cache_dir() {
        let dir = cache_dir();
        assert!(dir.to_string_lossy().contains("glimpse"));
        assert!(dir.ends_with("grammars"));
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
        parser
            .set_language(&language)
            .expect("failed to set language");

        let source = r#"
fn main() {
    println!("Hello, world!");
}
"#;

        let tree = parser.parse(source, None).expect("failed to parse");
        let root = tree.root_node();

        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0);

        let func = root.child(0).expect("expected function");
        assert_eq!(func.kind(), "function_item");
    }

    #[test]
    #[ignore]
    fn test_load_by_extension() {
        let language = load_language_by_extension("rs").expect("failed to load by extension");

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .expect("failed to set language");

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

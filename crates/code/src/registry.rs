use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::Deserialize;

static REGISTRY: OnceLock<Registry> = OnceLock::new();

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
            assert!(
                !lang.definition_query.is_empty(),
                "{} missing definition_query",
                lang.name
            );
            assert!(
                !lang.call_query.is_empty(),
                "{} missing call_query",
                lang.name
            );
            assert!(
                !lang.import_query.is_empty(),
                "{} missing import_query",
                lang.name
            );
        }
    }
}

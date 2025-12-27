use anyhow::Result;

#[derive(Debug, Clone)]
pub struct LanguageEntry {
    pub name: String,
    pub grammar_repo: String,
    pub import_query: Option<String>,
}

pub struct Registry {
    languages: Vec<LanguageEntry>,
}

impl Registry {
    pub fn load() -> Result<Self> {
        todo!("parse registry.toml")
    }

    pub fn get_language(&self, _name: &str) -> Option<&LanguageEntry> {
        todo!("lookup language by name")
    }
}

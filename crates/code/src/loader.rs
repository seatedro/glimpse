use std::path::PathBuf;

use anyhow::Result;
use tree_sitter::Language;

pub fn cache_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glimpse")
        .join("grammars")
}

pub fn load_language(_name: &str) -> Result<Language> {
    todo!("load compiled grammar via libloading")
}

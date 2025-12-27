use std::path::Path;

use anyhow::Result;
use tree_sitter::{Language, Query, Tree};

use super::index::{Call, Definition, Import};

pub struct QuerySet {
    pub definitions: Query,
    pub calls: Query,
    pub imports: Option<Query>,
}

impl QuerySet {
    pub fn load(_language: Language, _lang_name: &str) -> Result<Self> {
        todo!("load and compile queries for language")
    }
}

pub struct Extractor {
    _language: String,
}

impl Extractor {
    pub fn new(_language: &str) -> Result<Self> {
        todo!("initialize extractor with language queries")
    }

    pub fn extract_definitions(&self, _tree: &Tree, _source: &[u8], _path: &Path) -> Vec<Definition> {
        todo!("extract definitions from parsed tree")
    }

    pub fn extract_calls(&self, _tree: &Tree, _source: &[u8], _path: &Path) -> Vec<Call> {
        todo!("extract call sites from parsed tree")
    }

    pub fn extract_imports(&self, _tree: &Tree, _source: &[u8], _path: &Path) -> Vec<Import> {
        todo!("extract imports from parsed tree")
    }
}

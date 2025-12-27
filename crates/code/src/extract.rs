use std::path::Path;

use anyhow::Result;
use tree_sitter::Tree;

use super::schema::{Call, Definition, Import};

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

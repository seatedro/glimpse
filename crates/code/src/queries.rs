use anyhow::Result;
use tree_sitter::{Language, Query};

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

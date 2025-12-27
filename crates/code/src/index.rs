use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub const INDEX_DIR: &str = ".glimpse-index";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub kind: DefinitionKind,
    pub span: Span,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefinitionKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Module,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub callee: String,
    pub span: Span,
    pub file: PathBuf,
    pub caller: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub module_path: String,
    pub alias: Option<String>,
    pub span: Span,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub mtime: u64,
    pub size: u64,
    pub definitions: Vec<Definition>,
    pub calls: Vec<Call>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Index {
    pub files: HashMap<PathBuf, FileRecord>,
    pub version: u32,
}

impl Index {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            version: 1,
        }
    }

    pub fn is_stale(&self, _path: &PathBuf, _mtime: u64, _size: u64) -> bool {
        todo!("check if file needs re-indexing")
    }

    pub fn update(&mut self, _record: FileRecord) -> Result<()> {
        todo!("add or update file record")
    }
}

pub fn save_index(_index: &Index, _root: &Path) -> Result<()> {
    todo!("serialize index with bincode")
}

pub fn load_index(_root: &Path) -> Result<Option<Index>> {
    todo!("deserialize index from .glimpse-index/")
}

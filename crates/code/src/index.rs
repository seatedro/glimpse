use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::schema::FileRecord;

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

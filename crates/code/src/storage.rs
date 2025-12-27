use std::path::Path;

use anyhow::Result;

use super::index::Index;

pub const INDEX_DIR: &str = ".glimpse-index";

pub fn save_index(_index: &Index, _root: &Path) -> Result<()> {
    todo!("serialize index with bincode")
}

pub fn load_index(_root: &Path) -> Result<Option<Index>> {
    todo!("deserialize index from .glimpse-index/")
}

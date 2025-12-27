use std::path::PathBuf;

use anyhow::Result;

use super::index::Index;
use super::schema::Definition;

pub fn resolve_same_file(
    _callee: &str,
    _file: &PathBuf,
    _index: &Index,
) -> Option<Definition> {
    todo!("look for definition in same file")
}

pub fn resolve_by_index(
    _callee: &str,
    _index: &Index,
) -> Option<Definition> {
    todo!("search index for definition by name")
}

pub fn resolve_by_search(_callee: &str, _root: &PathBuf) -> Result<Option<Definition>> {
    todo!("fallback to ripgrep search")
}

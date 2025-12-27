use std::path::PathBuf;

use anyhow::Result;

use super::index::Index;
use super::schema::Definition;

pub fn resolve_import(
    _import_path: &str,
    _from_file: &PathBuf,
    _index: &Index,
) -> Result<Option<Definition>> {
    todo!("resolve use statement to definition")
}

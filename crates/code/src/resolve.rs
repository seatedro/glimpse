use std::path::{Path, PathBuf};

use anyhow::Result;

#[allow(unused_imports)]
use super::index::{Call, Definition, Index};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub members: Vec<PathBuf>,
}

impl Workspace {
    pub fn discover(_root: &Path) -> Result<Self> {
        todo!("parse Cargo.toml and discover workspace members")
    }

    pub fn resolve_crate(&self, _crate_name: &str) -> Option<PathBuf> {
        todo!("resolve crate name to path within workspace")
    }
}

pub fn resolve_import(
    _import_path: &str,
    _from_file: &Path,
    _index: &Index,
) -> Result<Option<Definition>> {
    todo!("resolve use statement to definition")
}

pub fn resolve_same_file(_callee: &str, _file: &Path, _index: &Index) -> Option<Definition> {
    todo!("look for definition in same file")
}

pub fn resolve_by_index(_callee: &str, _index: &Index) -> Option<Definition> {
    todo!("search index for definition by name")
}

pub fn resolve_by_search(_callee: &str, _root: &Path) -> Result<Option<Definition>> {
    todo!("fallback to ripgrep search")
}

pub struct Resolver {
    _index: Index,
    _workspace: Option<Workspace>,
    _root: PathBuf,
}

impl Resolver {
    pub fn new(_index: Index, _workspace: Option<Workspace>, _root: PathBuf) -> Self {
        todo!("initialize resolver")
    }

    pub fn resolve(&self, _call: &Call) -> Result<Option<Definition>> {
        todo!("resolve call to definition using all strategies")
    }
}

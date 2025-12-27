use std::path::{Path, PathBuf};

use anyhow::Result;

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

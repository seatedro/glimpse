use std::path::PathBuf;

use anyhow::Result;

use super::index::Index;
use super::schema::{Call, Definition};
use super::workspace::Workspace;

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

use std::path::PathBuf;

use anyhow::Result;

pub fn fetch_grammar(_repo_url: &str, _dest: &PathBuf) -> Result<()> {
    todo!("clone grammar repository")
}

pub fn compile_grammar(_grammar_dir: &PathBuf, _output: &PathBuf) -> Result<()> {
    todo!("compile grammar with cc")
}

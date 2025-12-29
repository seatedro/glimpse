use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Tree,
    Files,
    Both,
}

#[derive(Debug, Clone)]
pub enum TokenizerType {
    Tiktoken,
    HuggingFace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Exclude {
    File(PathBuf),
    Pattern(String),
}

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub size: u64,
}

use anyhow::Result;
use std::path::PathBuf;
use tiktoken_rs::o200k_base;

pub struct TokenCount {
    pub total_tokens: usize,
    pub breakdown: Vec<(PathBuf, usize)>, // (file_path, token_count)
}

pub struct TokenCounter {
    bpe: tiktoken_rs::CoreBPE,
}

impl TokenCounter {
    pub fn new() -> Result<Self> {
        Ok(Self { bpe: o200k_base()? })
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        self.bpe.encode_with_special_tokens(text).len()
    }

    pub fn count_files(&self, entries: &[super::output::FileEntry]) -> TokenCount {
        let mut total_tokens = 0;
        let mut breakdown = Vec::new();

        for entry in entries {
            let count = self.count_tokens(&entry.content);
            total_tokens += count;
            breakdown.push((entry.path.clone(), count));
        }

        TokenCount {
            total_tokens,
            breakdown,
        }
    }
}

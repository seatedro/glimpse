use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tiktoken_rs::get_bpe_from_model;
use tokenizers::Tokenizer as HfTokenizer;

pub enum TokenizerBackend {
    Tiktoken(tiktoken_rs::CoreBPE),
    HuggingFace(Box<HfTokenizer>),
}

pub struct TokenCount {
    pub total_tokens: usize,
    pub breakdown: Vec<(PathBuf, usize)>, // (file_path, token_count)
}

pub struct TokenCounter {
    backend: TokenizerBackend,
}

impl TokenCounter {
    pub fn new(model_name: &str) -> Result<Self> {
        let bpe = get_bpe_from_model(model_name)
            .map_err(|e| anyhow!("Failed to initialize tiktoken tokenizer: {}", e))?;

        Ok(Self {
            backend: TokenizerBackend::Tiktoken(bpe),
        })
    }

    pub fn with_hf_tokenizer(model_name: &str) -> Result<Self> {
        let tokenizer = HfTokenizer::from_pretrained(model_name, None).map_err(|e| {
            anyhow!(
                "Failed to load HuggingFace tokenizer '{}': {}",
                model_name,
                e
            )
        })?;

        Ok(Self {
            backend: TokenizerBackend::HuggingFace(Box::new(tokenizer)),
        })
    }

    pub fn from_hf_file(path: &str) -> Result<Self> {
        let tokenizer = HfTokenizer::from_file(path).map_err(|e| {
            anyhow!(
                "Failed to load HuggingFace tokenizer from file '{}': {}",
                path,
                e
            )
        })?;

        Ok(Self {
            backend: TokenizerBackend::HuggingFace(Box::new(tokenizer)),
        })
    }

    pub fn count_tokens(&self, text: &str) -> Result<usize> {
        match &self.backend {
            TokenizerBackend::Tiktoken(bpe) => {
                // tiktoken's encode_with_special_tokens is infallible
                Ok(bpe.encode_with_special_tokens(text).len())
            }
            TokenizerBackend::HuggingFace(tokenizer) => tokenizer
                .encode(text, false)
                .map_err(|e| anyhow!("Failed to encode text with HuggingFace tokenizer: {}", e))
                .map(|encoding| encoding.len()),
        }
    }

    pub fn count_files(&self, entries: &[super::output::FileEntry]) -> Result<TokenCount> {
        let mut total_tokens = 0;
        let mut breakdown = Vec::new();

        for entry in entries {
            let count = self.count_tokens(&entry.content).map_err(|e| {
                anyhow!(
                    "Failed to count tokens for file '{}': {}",
                    entry.path.display(),
                    e
                )
            })?;
            total_tokens += count;
            breakdown.push((entry.path.clone(), count));
        }

        Ok(TokenCount {
            total_tokens,
            breakdown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiktoken_counter() -> Result<()> {
        let counter = TokenCounter::new("gpt-4o")?;
        let text = "Hello, world!";
        let count = counter.count_tokens(text)?;
        assert!(count > 0);
        Ok(())
    }

    #[test]
    fn test_hf_counter() -> Result<()> {
        let counter = TokenCounter::with_hf_tokenizer("gpt2")?;
        let text = "Hello, world!";
        let count = counter.count_tokens(text)?;
        assert!(count > 0);
        Ok(())
    }
}

mod analyzer;
mod cli;
mod config;
mod output;
mod source_detection;
mod tokenizer;

use crate::analyzer::process_directory;
use crate::cli::Cli;
use crate::config::load_config;

fn main() -> anyhow::Result<()> {
    // Load config first
    let config = load_config()?;

    // Parse CLI args with config as context
    let args = Cli::parse_with_config(&config)?;

    process_directory(&args)
}

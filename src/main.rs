mod analyzer;
mod cli;
mod config;
mod file_picker;
mod output;
mod source_detection;
mod tokenizer;

use crate::analyzer::process_directory;
use crate::cli::Cli;
use crate::config::{get_config_path, load_config};

fn main() -> anyhow::Result<()> {
    let config = load_config()?;

    let args = Cli::parse_with_config(&config)?;

    if args.config_path {
        let path = get_config_path()?;
        println!("{}", path.display());
        return Ok(());
    }

    process_directory(&args)
}

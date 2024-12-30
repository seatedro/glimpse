use clap::Parser;

mod analyzer;
mod cli;
mod output;
mod patterns;
mod source_detection;

use crate::analyzer::process_directory;
use crate::cli::Cli;

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    process_directory(&args)
}

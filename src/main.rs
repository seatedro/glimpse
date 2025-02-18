mod analyzer;
mod cli;
mod config;
mod file_picker;
mod output;
mod source_detection;
mod tokenizer;
mod url_processor;

use crate::analyzer::process_directory;
use crate::cli::Cli;
use crate::config::{get_config_path, load_config};
use crate::url_processor::UrlProcessor;
use std::fs;

fn main() -> anyhow::Result<()> {
    let config = load_config()?;
    let args = Cli::parse_with_config(&config)?;

    if args.config_path {
        let path = get_config_path()?;
        println!("{}", path.display());
        return Ok(());
    }

    for path in &args.paths {
        if path.starts_with("http://") || path.starts_with("https://") {
            let link_depth = args.link_depth.unwrap_or(config.default_link_depth);
            let traverse = args.traverse_links || config.traverse_links;

            let mut processor = UrlProcessor::new(link_depth);
            let content = processor.process_url(path, traverse)?;

            if let Some(output_file) = &args.file {
                fs::write(output_file, content)?;
            } else if args.print {
                println!("{}", content);
            }
        } else {
            process_directory(&args)?;
        }
    }

    Ok(())
}

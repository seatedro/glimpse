mod analyzer;
mod cli;
mod config;
mod file_picker;
mod git_processor;
mod output;
mod source_detection;
mod tokenizer;
mod url_processor;

use crate::analyzer::process_directory;
use crate::cli::Cli;
use crate::config::{get_config_path, load_config};
use crate::git_processor::GitProcessor;
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

    let url_paths: Vec<_> = args
        .paths
        .iter()
        .filter(|path| {
            GitProcessor::is_git_url(path)
                || path.starts_with("http://")
                || path.starts_with("https://")
        })
        .collect();

    if url_paths.len() > 1 {
        return Err(anyhow::anyhow!(
            "Only one URL or git repository can be processed at a time"
        ));
    }

    if let Some(url_path) = url_paths.first() {
        if GitProcessor::is_git_url(url_path) {
            let git_processor = GitProcessor::new()?;
            let repo_path = git_processor.process_repo(url_path)?;
            process_directory(&args.with_path(repo_path.to_str().unwrap()))?;
        } else if url_path.starts_with("http://") || url_path.starts_with("https://") {
            let link_depth = args.link_depth.unwrap_or(config.default_link_depth);
            let traverse = args.traverse_links || config.traverse_links;

            let mut processor = UrlProcessor::new(link_depth);
            let content = processor.process_url(url_path, traverse)?;

            if let Some(output_file) = &args.file {
                fs::write(output_file, content)?;
            } else if args.print {
                println!("{}", content);
            }
        }
    } else {
        process_directory(&args)?;
    }

    Ok(())
}

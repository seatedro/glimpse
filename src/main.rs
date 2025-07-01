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
use crate::config::{
    get_config_path, load_config, load_repo_config, save_config, save_repo_config, RepoConfig,
};
use crate::git_processor::GitProcessor;
use crate::url_processor::UrlProcessor;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

fn is_url_or_git(path: &str) -> bool {
    GitProcessor::is_git_url(path) || path.starts_with("http://") || path.starts_with("https://")
}

fn has_custom_options(args: &Cli) -> bool {
    args.include.is_some()
        || args.exclude.is_some()
        || args.max_size.is_some()
        || args.max_depth.is_some()
        || args.output.is_some()
        || args.file.is_some()
        || args.hidden
        || args.no_ignore
}

fn main() -> anyhow::Result<()> {
    let mut config = load_config()?;
    let mut args = Cli::parse_with_config(&config)?;

    if args.config_path {
        let path = get_config_path()?;
        println!("{}", path.display());
        return Ok(());
    }

    let url_paths: Vec<_> = args
        .paths
        .iter()
        .filter(|path| is_url_or_git(path))
        .take(1)
        .cloned()
        .collect();

    if url_paths.is_empty() && !args.paths.is_empty() {
        let base_path = PathBuf::from(&args.paths[0]);
        let root_dir = find_containing_dir_with_glimpse(&base_path)?;
        let glimpse_file = root_dir.join(".glimpse");

        if args.config {
            let repo_config = create_repo_config_from_args(&args);
            save_repo_config(&glimpse_file, &repo_config)?;
            println!("Configuration saved to {}", glimpse_file.display());

            // If the user explicitly saved a config, remove this directory from the skipped list
            if let Ok(canonical_root) = std::fs::canonicalize(&root_dir) {
                let root_str = canonical_root.to_string_lossy().to_string();
                if let Some(pos) = config
                    .skipped_prompt_repos
                    .iter()
                    .position(|p| p == &root_str)
                {
                    config.skipped_prompt_repos.remove(pos);
                    save_config(&config)?;
                }
            }
        } else if glimpse_file.exists() {
            println!("Loading configuration from {}", glimpse_file.display());
            let repo_config = load_repo_config(&glimpse_file)?;
            apply_repo_config(&mut args, &repo_config);
        } else if has_custom_options(&args) {
            // Determine canonical root directory path for consistent tracking
            let canonical_root = std::fs::canonicalize(&root_dir).unwrap_or(root_dir.clone());
            let root_str = canonical_root.to_string_lossy().to_string();

            if !config.skipped_prompt_repos.contains(&root_str) {
                print!(
                    "Would you like to save these options as defaults for this directory? (y/n): "
                );
                io::stdout().flush()?;
                let mut response = String::new();
                io::stdin().read_line(&mut response)?;

                if response.trim().to_lowercase() == "y" {
                    let repo_config = create_repo_config_from_args(&args);
                    save_repo_config(&glimpse_file, &repo_config)?;
                    println!("Configuration saved to {}", glimpse_file.display());

                    // In case it was previously skipped, remove from skipped list
                    if let Some(pos) = config
                        .skipped_prompt_repos
                        .iter()
                        .position(|p| p == &root_str)
                    {
                        config.skipped_prompt_repos.remove(pos);
                        save_config(&config)?;
                    }
                } else {
                    // Record that user declined for this project
                    config.skipped_prompt_repos.push(root_str);
                    save_config(&config)?;
                }
            }
        }
    }

    if url_paths.len() > 1 {
        return Err(anyhow::anyhow!(
            "Only one URL or git repository can be processed at a time"
        ));
    }

    if let Some(url_path) = url_paths.first() {
        if GitProcessor::is_git_url(url_path) {
            let git_processor = GitProcessor::new()?;
            let repo_path = git_processor.process_repo(url_path)?;
            args.validate_args(true)?;

            let mut subpaths: Vec<String> = vec![];
            let mut found_url = false;
            for p in &args.paths {
                if !found_url && p.as_str() == url_path.as_str() {
                    found_url = true;
                    continue;
                }
                if found_url {
                    subpaths.push(p.clone());
                }
            }

            let process_args = if subpaths.is_empty() {
                // No subpaths specified, process the whole repo
                args.with_path(repo_path.to_str().unwrap())
            } else {
                // Process only the specified subpaths inside the repo
                let mut new_args = args.clone();
                new_args.paths = subpaths
                    .iter()
                    .map(|sub| {
                        // Join with repo_path
                        let mut joined = std::path::PathBuf::from(&repo_path);
                        joined.push(sub);
                        joined.to_string_lossy().to_string()
                    })
                    .collect();
                new_args
            };
            process_directory(&process_args)?;
        } else if url_path.starts_with("http://") || url_path.starts_with("https://") {
            args.validate_args(true)?;
            let link_depth = args.link_depth.unwrap_or(config.default_link_depth);
            let traverse = args.traverse_links || config.traverse_links;

            let mut processor = UrlProcessor::new(link_depth);
            let content = processor.process_url(url_path, traverse)?;

            if let Some(output_file) = &args.file {
                fs::write(output_file, content)?;
                println!("Output written to: {}", output_file.display());
            } else if args.print {
                println!("{content}");
            } else {
                // Default behavior for URLs if no -f or --print: copy to clipboard
                match arboard::Clipboard::new()
                    .and_then(|mut clipboard| clipboard.set_text(content))
                {
                    Ok(_) => println!("URL content copied to clipboard"),
                    Err(_) => {
                        println!("Failed to copy to clipboard, use -f to save to a file instead")
                    }
                }
            }
        }
    } else {
        args.validate_args(false)?;
        process_directory(&args)?;
    }

    Ok(())
}

fn find_containing_dir_with_glimpse(path: &Path) -> anyhow::Result<PathBuf> {
    let mut current = if path.is_file() {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        path.to_path_buf()
    };

    // Try to find a .glimpse file or go up until we reach the root
    loop {
        if current.join(".glimpse").exists() {
            return Ok(current);
        }

        if !current.pop() {
            // If we can't go up anymore, just use the original path
            return Ok(if path.is_file() {
                path.parent().unwrap_or(Path::new(".")).to_path_buf()
            } else {
                path.to_path_buf()
            });
        }
    }
}

fn create_repo_config_from_args(args: &Cli) -> RepoConfig {
    use crate::config::BackwardsCompatOutputFormat;

    RepoConfig {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        max_size: args.max_size,
        max_depth: args.max_depth,
        output: args.output.clone().map(BackwardsCompatOutputFormat::from),
        file: args.file.clone(),
        hidden: Some(args.hidden),
        no_ignore: Some(args.no_ignore),
    }
}

fn apply_repo_config(args: &mut Cli, repo_config: &RepoConfig) {
    if let Some(ref include) = repo_config.include {
        args.include = Some(include.clone());
    }

    if let Some(ref exclude) = repo_config.exclude {
        args.exclude = Some(exclude.clone());
    }

    if let Some(max_size) = repo_config.max_size {
        args.max_size = Some(max_size);
    }

    if let Some(max_depth) = repo_config.max_depth {
        args.max_depth = Some(max_depth);
    }

    if let Some(ref output) = repo_config.output {
        args.output = Some((*output).clone().into());
    }

    if let Some(ref file) = repo_config.file {
        args.file = Some(file.clone());
    }

    if let Some(hidden) = repo_config.hidden {
        args.hidden = hidden;
    }

    if let Some(no_ignore) = repo_config.no_ignore {
        args.no_ignore = no_ignore;
    }
}

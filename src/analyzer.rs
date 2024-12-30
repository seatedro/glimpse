use crate::cli::{Cli, TokenizerType};
use crate::output::{display_token_counts, generate_output, handle_output, FileEntry};
use crate::source_detection;
use crate::tokenizer::TokenCounter;
use anyhow::Result;
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::Path;

pub fn process_directory(args: &Cli) -> Result<()> {
    // Configure thread pool if specified
    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()?;
    }

    // Set up progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Scanning files...");

    let max_size = args.max_size.expect("max_size should be set from config");
    let max_depth = args.max_depth.expect("max_depth should be set from config");
    let output_format = args
        .output
        .as_deref()
        .expect("output format should be set from config");

    // Build the walker with ignore patterns
    let mut builder = WalkBuilder::new(&args.path);
    builder
        .max_depth(Some(max_depth))
        .hidden(!args.hidden)
        .git_ignore(!args.no_ignore)
        .ignore(!args.no_ignore);

    // Add custom ignore/include patterns
    if let Some(ref excludes) = args.exclude {
        for pattern in excludes {
            builder.add_ignore(pattern);
        }
    }
    if let Some(ref includes) = args.include {
        for pattern in includes {
            builder.add_custom_ignore_filename(pattern);
        }
    }

    // Collect all valid files
    let entries: Vec<FileEntry> = builder
        .build()
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                && source_detection::is_source_file(entry.path())
                && entry
                    .metadata()
                    .map(|m| m.len() <= max_size)
                    .unwrap_or(false)
        })
        .filter_map(|entry| process_file(&entry, &args.path).ok())
        .collect();

    pb.finish();

    // Generate output
    let output = generate_output(&entries, output_format)?;

    // Handle output (print/copy/save)
    handle_output(output, args)?;

    if args.tokens {
        let counter = create_token_counter(args)?;
        display_token_counts(counter, &entries)?;
    }

    Ok(())
}

pub fn create_token_counter(args: &Cli) -> Result<TokenCounter> {
    match args.tokenizer.as_ref().unwrap_or(&TokenizerType::Tiktoken) {
        TokenizerType::Tiktoken => {
            if let Some(model) = &args.model {
                TokenCounter::new(model)
            } else {
                TokenCounter::new("gpt-4o")
            }
        }
        TokenizerType::HuggingFace => {
            if let Some(path) = &args.tokenizer_file {
                TokenCounter::from_hf_file(path.to_str().unwrap())
            } else if let Some(model) = &args.model {
                TokenCounter::with_hf_tokenizer(model)
            } else {
                anyhow::bail!("HuggingFace tokenizer requires either a model name or file path")
            }
        }
    }
}

fn process_file(entry: &ignore::DirEntry, base_path: &Path) -> Result<FileEntry> {
    let relative_path = entry.path().strip_prefix(base_path)?;
    let content = fs::read_to_string(entry.path())?;

    Ok(FileEntry {
        path: relative_path.to_path_buf(),
        content,
        size: entry.metadata()?.len(),
    })
}

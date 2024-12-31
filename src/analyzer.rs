use crate::cli::{Cli, Exclude, TokenizerType};
use crate::file_picker::FilePicker;
use crate::output::{display_token_counts, generate_output, handle_output, FileEntry};
use crate::source_detection;
use crate::tokenizer::TokenCounter;
use anyhow::Result;
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

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
    let mut builder = WalkBuilder::new(&args.paths[0]);
    builder
        .max_depth(Some(max_depth))
        .hidden(!args.hidden)
        .git_ignore(!args.no_ignore)
        .ignore(!args.no_ignore);

    if let Some(ref includes) = args.include {
        for pattern in includes {
            builder.add_custom_ignore_filename(pattern);
        }
    }

    // Collect all valid files
    let entries = if args.interactive {
        let mut picker =
            FilePicker::new(args.paths[0].clone(), max_size, args.hidden, args.no_ignore);
        let selected_paths = picker.run()?;

        // Process selected files
        selected_paths
            .into_iter()
            .filter_map(|path| {
                let entry = ignore::WalkBuilder::new(&path)
                    .build()
                    .next()
                    .and_then(|r| r.ok());
                entry.and_then(|e| process_file(&e, &args.paths[0]).ok())
            })
            .collect::<Vec<FileEntry>>()
    } else {
        let mut all_entries = Vec::new();

        for path in &args.paths {
            if path.is_dir() {
                let mut builder = WalkBuilder::new(path);
                builder
                    .max_depth(Some(max_depth))
                    .hidden(!args.hidden)
                    .git_ignore(!args.no_ignore)
                    .ignore(!args.no_ignore);

                if let Some(ref includes) = args.include {
                    for pattern in includes {
                        builder.add_custom_ignore_filename(pattern);
                    }
                }

                let dir_entries: Vec<FileEntry> = builder
                    .build()
                    .par_bridge()
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| {
                        let should_exclude = is_excluded(entry, args);
                        if should_exclude {
                            println!("filtering out: {}", entry.path().display());
                        }
                        !should_exclude
                    })
                    .filter(|entry| {
                        entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                            && source_detection::is_source_file(entry.path())
                            && entry
                                .metadata()
                                .map(|m| m.len() <= max_size)
                                .unwrap_or(false)
                    })
                    .filter_map(|entry| process_file(&entry, path).ok())
                    .collect();

                all_entries.extend(dir_entries);
            } else if path.is_file() {
                // Process single file
                if source_detection::is_source_file(path)
                    && path
                        .metadata()
                        .map(|m| m.len() <= max_size)
                        .unwrap_or(false)
                {
                    let entry = ignore::WalkBuilder::new(path)
                        .build()
                        .next()
                        .and_then(|r| r.ok());
                    if let Some(entry) = entry {
                        if !is_excluded(&entry, args) {
                            if let Ok(file_entry) = process_file(&entry, path) {
                                all_entries.push(file_entry);
                            }
                        }
                    }
                }
            }
        }
        all_entries
    };
    pb.finish();

    // Generate output
    let output = generate_output(&entries, output_format)?;

    // Handle output (print/copy/save)
    handle_output(output, args)?;

    if !args.no_tokens {
        let counter = create_token_counter(args)?;
        display_token_counts(counter, &entries)?;
    }

    Ok(())
}

fn is_excluded(entry: &ignore::DirEntry, args: &Cli) -> bool {
    if let Some(excludes) = &args.exclude {
        let path_str = entry.path().to_string_lossy();

        for exclude in excludes {
            match exclude {
                Exclude::Pattern(pattern) => {
                    let pattern = if !pattern.starts_with("./") && !pattern.starts_with("/") {
                        format!("./{}", pattern)
                    } else {
                        pattern.clone()
                    };

                    if let Ok(glob) = globset::GlobBuilder::new(&pattern)
                        .case_insensitive(false)
                        .build()
                    {
                        let matcher = glob.compile_matcher();
                        let check_path = if !path_str.starts_with("./") {
                            format!("./{}", path_str)
                        } else {
                            path_str.to_string()
                        };

                        if matcher.is_match(&check_path) {
                            println!("excluded: {} (matched {})", path_str, pattern);
                            return true;
                        }
                    }
                }
                Exclude::File(path) => {
                    let matches = entry.path().ends_with(path);
                    println!("file exclude {} matches?: {}", path.display(), matches); // debug
                    if matches {
                        return true;
                    }
                }
            }
        }
    }
    false
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
    let relative_path = if base_path.is_file() {
        // If base_path is a file, use the file name as the relative path
        base_path.file_name().map(PathBuf::from).unwrap_or_default()
    } else {
        // Otherwise, strip the base path as usual
        entry.path().strip_prefix(base_path)?.to_path_buf()
    };
    let content = fs::read_to_string(entry.path())?;

    Ok(FileEntry {
        path: relative_path.to_path_buf(),
        content,
        size: entry.metadata()?.len(),
    })
}

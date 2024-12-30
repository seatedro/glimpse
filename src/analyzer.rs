use crate::cli::Cli;
use crate::output::{generate_output, FileEntry};
use crate::patterns::PatternMatcher;
use crate::source_detection;
use anyhow::Result;
use colored::*;
use ignore::{DirEntry, WalkBuilder};
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

    // Build the walker with ignore patterns
    let mut builder = WalkBuilder::new(&args.path);
    builder
        .max_depth(Some(args.max_depth))
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
                    .map(|m| m.len() <= args.max_size)
                    .unwrap_or(false)
        })
        .filter_map(|entry| process_file(&entry, &args.path).ok())
        .collect();

    pb.finish_with_message("Analysis complete!");

    // Generate and print output
    generate_output(&entries, &args.output)?;

    Ok(())
}

fn should_process_entry(entry: &DirEntry, include_hidden: bool) -> bool {
    if !include_hidden {
        if let Some(file_name) = entry.file_name().to_str() {
            if file_name.starts_with('.') {
                return false;
            }
        }
    }
    true
}

fn filter_entry(entry: &DirEntry, matcher: &PatternMatcher, max_size: u64) -> bool {
    if !entry
        .file_type()
        .expect("Failed to get file type")
        .is_file()
        || !source_detection::is_source_file(entry.path())
    {
        return false;
    }

    if let Ok(metadata) = entry.metadata() {
        if metadata.len() > max_size {
            return false;
        }
    }

    matcher.should_process(entry.path())
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

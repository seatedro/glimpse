use crate::cli::Cli;
use crate::output::{generate_output, FileEntry};
use crate::patterns::PatternMatcher;
use anyhow::Result;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

pub fn process_directory(args: &Cli) -> Result<()> {
    let pattern_matcher = PatternMatcher::new(args.include.clone(), args.exclude.clone())?;

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

    // Collect all valid files
    let entries: Vec<FileEntry> = WalkDir::new(&args.path)
        .max_depth(args.max_depth)
        .into_iter()
        .filter_entry(|e| should_process_entry(e, args.hidden))
        .filter_map(|e| e.ok())
        .filter(|e| filter_entry(e, &pattern_matcher, args.max_size))
        .par_bridge() // Enable parallel processing
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
    if !entry.file_type().is_file() {
        return false;
    }

    if let Ok(metadata) = entry.metadata() {
        if metadata.len() > max_size {
            return false;
        }
    }

    matcher.should_process(entry.path())
}

fn process_file(entry: &DirEntry, base_path: &Path) -> Result<FileEntry> {
    let relative_path = entry.path().strip_prefix(base_path)?;
    let content = fs::read_to_string(entry.path())?;

    Ok(FileEntry {
        path: relative_path.to_path_buf(),
        content,
        size: entry.metadata()?.len(),
    })
}

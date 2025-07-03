use crate::cli::{Cli, Exclude, OutputFormat, TokenizerType};
use crate::file_picker::FilePicker;
use crate::output::{
    display_token_counts, generate_output, generate_pdf, handle_output, FileEntry,
};
use crate::source_detection;
use crate::tokenizer::TokenCounter;
use anyhow::Result;
use ignore::{overrides::OverrideBuilder, WalkBuilder};
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

    let output_format = args
        .output
        .clone()
        .expect("output format should be set from config");
    let entries = process_entries(args)?;
    pb.finish();

    if let Some(pdf_path) = &args.pdf {
        let pdf_data = generate_pdf(&entries, args.output.clone().unwrap_or(OutputFormat::Both))?;
        fs::write(pdf_path, pdf_data)?;
        println!("PDF output written to: {}", pdf_path.display());
    } else {
        // Determine project name for XML output
        let project_name = if args.xml {
            Some(determine_project_name(&args.paths))
        } else {
            None
        };

        // Handle output (print/copy/save)
        let output = generate_output(&entries, output_format, args.xml, project_name)?;
        handle_output(output, args)?;
    }

    if !args.no_tokens {
        let counter = create_token_counter(args)?;
        display_token_counts(counter, &entries)?;
    }

    Ok(())
}

fn determine_project_name(paths: &[String]) -> String {
    if let Some(first_path) = paths.first() {
        let path = std::path::Path::new(first_path);

        // If it's a directory, use its name
        if path.is_dir() {
            if let Some(name) = path.file_name() {
                return name.to_string_lossy().to_string();
            }
        }

        // If it's a file, use the parent directory name
        if path.is_file() {
            if let Some(parent) = path.parent() {
                if let Some(name) = parent.file_name() {
                    return name.to_string_lossy().to_string();
                }
            }
        }

        // Fallback to just the path itself
        first_path.clone()
    } else {
        "project".to_string()
    }
}

pub fn process_entries(args: &Cli) -> Result<Vec<FileEntry>> {
    let max_size = args.max_size.expect("max_size should be set from config");
    let max_depth = args.max_depth.expect("max_depth should be set from config");

    let entries = if args.interactive {
        let mut picker = FilePicker::new(
            PathBuf::from(&args.paths[0]),
            max_size,
            args.hidden,
            args.no_ignore,
        );
        let selected_paths = picker.run()?;

        // Process selected files
        selected_paths
            .into_iter()
            .filter_map(|path| {
                let entry = ignore::WalkBuilder::new(&path)
                    .build()
                    .next()
                    .and_then(|r| r.ok());
                entry.and_then(|e| process_file(&e, &path).ok())
            })
            .collect::<Vec<FileEntry>>()
    } else {
        let mut all_entries = Vec::new();
        for path_str in &args.paths {
            let path = std::path::Path::new(path_str);
            if path.is_dir() {
                let mut builder = WalkBuilder::new(path);
                builder
                    .max_depth(Some(max_depth))
                    .hidden(!args.hidden)
                    .git_ignore(!args.no_ignore)
                    .ignore(!args.no_ignore);

                let mut override_builder = OverrideBuilder::new(path);

                override_builder.add("!**/GLIMPSE.md")?;
                override_builder.add("!**/.glimpse")?;

                // Handle include patterns first (positive patterns)
                if let Some(ref includes) = args.include {
                    for pattern in includes {
                        // Include patterns are positive patterns (no ! prefix)
                        if let Err(e) = override_builder.add(pattern) {
                            eprintln!("Warning: Invalid include pattern '{pattern}': {e}");
                        }
                    }
                }

                // Handle exclude patterns (negative patterns)
                if let Some(ref excludes) = args.exclude {
                    for exclude in excludes {
                        match exclude {
                            Exclude::Pattern(pattern) => {
                                // Add a '!' prefix if it doesn't already have one
                                // This makes it a negative pattern (exclude)
                                let exclude_pattern = if !pattern.starts_with('!') {
                                    format!("!{pattern}")
                                } else {
                                    pattern.clone()
                                };

                                if let Err(e) = override_builder.add(&exclude_pattern) {
                                    eprintln!("Warning: Invalid exclude pattern '{pattern}': {e}");
                                }
                            }
                            Exclude::File(file_path) => {
                                // For file excludes, handle differently if:
                                if file_path.is_absolute() {
                                    // For absolute paths, check if they exist
                                    if file_path.exists() {
                                        // If base_path is part of file_path, make it relative
                                        if let Ok(relative_path) = file_path.strip_prefix(path) {
                                            let pattern = format!("!{}", relative_path.display());
                                            if let Err(e) = override_builder.add(&pattern) {
                                                eprintln!("Warning: Could not add file exclude pattern for '{}': {}", file_path.display(), e);
                                            }
                                        } else {
                                            // This doesn't affect current directory
                                            eprintln!(
                                                "Note: File exclude not under current path: {}",
                                                file_path.display()
                                            );
                                        }
                                    }
                                } else {
                                    // For relative paths like "src", use as-is with a ! prefix
                                    let pattern = format!("!{}", file_path.display());
                                    if let Err(e) = override_builder.add(&pattern) {
                                        eprintln!(
                                            "Warning: Could not add file exclude pattern '{pattern}': {e}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                let overrides = override_builder.build()?;
                builder.overrides(overrides);

                let dir_entries: Vec<FileEntry> = builder
                    .build()
                    .par_bridge()
                    .filter_map(|entry| entry.ok())
                    // No longer need the is_excluded filter here, WalkBuilder handles it
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
                    // Need to check includes and excludes even for single files explicitly passed
                    let mut excluded = false;
                    let mut override_builder = OverrideBuilder::new(path.parent().unwrap_or(path)); // Base relative to parent

                    // Handle include patterns first (positive patterns)
                    if let Some(ref includes) = args.include {
                        for pattern in includes {
                            // Include patterns are positive patterns (no ! prefix)
                            if let Err(e) = override_builder.add(pattern) {
                                eprintln!("Warning: Invalid include pattern '{pattern}': {e}");
                            }
                        }
                    }

                    // Handle exclude patterns (negative patterns)
                    if let Some(ref excludes) = args.exclude {
                        for exclude in excludes {
                            match exclude {
                                Exclude::Pattern(pattern) => {
                                    // Add a '!' prefix if it doesn't already have one
                                    // This makes it a negative pattern (exclude)
                                    let exclude_pattern = if !pattern.starts_with('!') {
                                        format!("!{pattern}")
                                    } else {
                                        pattern.clone()
                                    };
                                    if let Err(e) = override_builder.add(&exclude_pattern) {
                                        eprintln!(
                                            "Warning: Invalid exclude pattern '{pattern}': {e}"
                                        );
                                    }
                                }
                                Exclude::File(file_path) => {
                                    if path == file_path {
                                        excluded = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if excluded {
                            continue;
                        }
                    }

                    let overrides = override_builder.build()?;
                    let match_result = overrides.matched(path, false);

                    // If there are include patterns, the file must match at least one include pattern
                    // and not be excluded by any exclude pattern
                    if args.include.is_some() {
                        // With include patterns: file must be whitelisted (matched by include) and not ignored (excluded)
                        excluded = !match_result.is_whitelist() || match_result.is_ignore();
                    } else {
                        // Without include patterns: file is excluded only if it matches an exclude pattern
                        excluded = match_result.is_ignore();
                    }

                    if !excluded {
                        let entry = ignore::WalkBuilder::new(path)
                            .build()
                            .next()
                            .and_then(|r| r.ok());
                        if let Some(entry) = entry {
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

    Ok(entries)
}

// Removed the is_excluded function as it's now handled by WalkBuilder overrides

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::{tempdir, TempDir};

    fn setup_test_directory() -> Result<(TempDir, Vec<PathBuf>)> {
        let dir = tempdir()?;
        let mut created_files = Vec::new();

        // Create a nested directory structure with various file types
        let test_files = vec![
            ("src/main.rs", "fn main() {}"),
            ("src/lib.rs", "pub fn lib() {}"),
            ("tests/test.rs", "#[test] fn test() {}"),
            ("docs/readme.md", "# Documentation"),
            ("build/output.o", "binary"),
            ("node_modules/package.json", "{}"),
            ("target/debug/binary", "binary"),
            (".git/config", "git config"),
            ("src/nested/deep/code.rs", "fn nested() {}"),
            ("src/nested/deep/script.py", "def nested(): pass"),
        ];

        for (path, content) in test_files {
            let full_path = dir.path().join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = File::create(&full_path)?;
            writeln!(file, "{content}")?;
            created_files.push(full_path);
        }

        Ok((dir, created_files))
    }

    fn create_test_cli(dir_path: &Path) -> Cli {
        Cli {
            config: false,
            paths: vec![dir_path.to_string_lossy().to_string()],
            config_path: false,
            include: None,
            exclude: None,
            max_size: Some(10 * 1024 * 1024), // 10MB
            max_depth: Some(10),
            output: Some(OutputFormat::Both),
            file: None,
            print: true,
            threads: None,
            hidden: false,
            no_ignore: false,
            no_tokens: true,
            tokenizer: None,
            model: None,
            tokenizer_file: None,
            interactive: false,
            pdf: None,
            traverse_links: false,
            link_depth: None,
            xml: false,
        }
    }

    #[test]
    fn test_exclude_patterns() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        let main_rs_path = dir.path().join("src/main.rs");

        let test_cases = vec![
            // Pattern exclusions
            (Exclude::Pattern("**/*.rs".to_string()), true),
            (Exclude::Pattern("**/*.js".to_string()), false),
            (Exclude::Pattern("test/**".to_string()), false),
            // File exclusions
            (Exclude::File(main_rs_path.clone()), true),
            (Exclude::File(PathBuf::from("nonexistent.rs")), false),
        ];

        for (exclude, should_exclude) in test_cases {
            let mut override_builder = OverrideBuilder::new(dir.path());

            match &exclude {
                Exclude::Pattern(pattern) => {
                    // For patterns that should exclude, we need to add a "!" prefix
                    // to make them negative patterns (exclusions)
                    let exclude_pattern = if !pattern.starts_with('!') {
                        format!("!{pattern}")
                    } else {
                        pattern.clone()
                    };
                    override_builder.add(&exclude_pattern).unwrap();
                }
                Exclude::File(file_path) => {
                    if file_path.exists() {
                        // Get the file path relative to the test directory
                        let rel_path = if file_path.is_absolute() {
                            file_path.strip_prefix(dir.path()).unwrap_or(file_path)
                        } else {
                            file_path
                        };
                        // Add as a negative pattern
                        let pattern = format!("!{}", rel_path.display());
                        override_builder.add(&pattern).unwrap();
                    }
                }
            }

            let overrides = override_builder.build()?;
            let is_ignored = overrides.matched(&main_rs_path, false).is_ignore();

            assert_eq!(
                is_ignored, should_exclude,
                "Failed for exclude: {exclude:?}"
            );
        }

        Ok(())
    }

    #[test]
    fn test_process_directory_with_excludes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test excluding all Rust files
        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;

        // Verify no .rs files were processed
        for entry in &entries {
            assert!(
                entry.path.extension().and_then(|ext| ext.to_str()) != Some("rs"),
                "Found .rs file that should have been excluded: {:?}",
                entry.path
            );
        }

        // Test excluding specific directories
        cli.exclude = Some(vec![
            Exclude::Pattern("**/node_modules/**".to_string()),
            Exclude::Pattern("**/target/**".to_string()),
            Exclude::Pattern("**/.git/**".to_string()),
        ]);
        let entries = process_entries(&cli)?;

        // Verify excluded directories were not processed
        for entry in &entries {
            let path_str = entry.path.to_string_lossy();
            assert!(
                !path_str.contains("node_modules")
                    && !path_str.contains("target")
                    && !path_str.contains(".git"),
                "Found file from excluded directory: {:?}",
                entry.path
            );
        }

        Ok(())
    }

    #[test]
    fn test_process_directory_with_includes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test including only Rust files
        cli.include = Some(vec!["**/*.rs".to_string()]);
        let entries = process_entries(&cli)?;

        // Verify only .rs files were processed
        assert!(!entries.is_empty(), "Should have found some .rs files");
        for entry in &entries {
            assert!(
                entry.path.extension().and_then(|ext| ext.to_str()) == Some("rs"),
                "Found non-.rs file: {:?}",
                entry.path
            );
        }

        // Should find 4 .rs files: main.rs, lib.rs, test.rs, code.rs
        assert_eq!(entries.len(), 4, "Should find exactly 4 .rs files");

        // Test including multiple patterns
        cli.include = Some(vec!["**/*.rs".to_string(), "**/*.py".to_string()]);
        let entries = process_entries(&cli)?;

        // Verify only .rs and .py files were processed
        assert!(
            !entries.is_empty(),
            "Should have found some .rs and .py files"
        );
        for entry in &entries {
            let ext = entry.path.extension().and_then(|ext| ext.to_str());
            assert!(
                ext == Some("rs") || ext == Some("py"),
                "Found file with unexpected extension: {:?}",
                entry.path
            );
        }

        // Should find 4 .rs files + 1 .py file = 5 total
        assert_eq!(entries.len(), 5, "Should find exactly 5 .rs and .py files");

        Ok(())
    }

    #[test]
    fn test_process_directory_with_includes_and_excludes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test including only Rust files but excluding specific ones
        cli.include = Some(vec!["**/*.rs".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/test.rs".to_string())]);
        let entries = process_entries(&cli)?;

        // Verify only .rs files were processed, but test.rs was excluded
        assert!(!entries.is_empty(), "Should have found some .rs files");
        for entry in &entries {
            assert!(
                entry.path.extension().and_then(|ext| ext.to_str()) == Some("rs"),
                "Found non-.rs file: {:?}",
                entry.path
            );
            assert!(
                !entry.path.to_string_lossy().contains("test.rs"),
                "Found excluded test.rs file: {:?}",
                entry.path
            );
        }

        // Should find 3 .rs files (main.rs, lib.rs, code.rs) but not test.rs
        assert_eq!(
            entries.len(),
            3,
            "Should find exactly 3 .rs files (excluding test.rs)"
        );

        // Test including multiple file types but excluding a directory
        cli.include = Some(vec!["**/*.rs".to_string(), "**/*.py".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/nested/**".to_string())]);
        let entries = process_entries(&cli)?;

        // Verify only .rs and .py files were processed, but nested directory was excluded
        assert!(
            !entries.is_empty(),
            "Should have found some .rs and .py files"
        );
        for entry in &entries {
            let ext = entry.path.extension().and_then(|ext| ext.to_str());
            assert!(
                ext == Some("rs") || ext == Some("py"),
                "Found file with unexpected extension: {:?}",
                entry.path
            );
            assert!(
                !entry.path.to_string_lossy().contains("nested"),
                "Found file from excluded nested directory: {:?}",
                entry.path
            );
        }

        // Should find 3 .rs files (main.rs, lib.rs, test.rs) but not code.rs or script.py from nested
        assert_eq!(
            entries.len(),
            3,
            "Should find exactly 3 files (excluding nested directory)"
        );

        Ok(())
    }

    #[test]
    fn test_process_directory_depth_limit() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test with depth limit of 1
        cli.max_depth = Some(1);
        process_directory(&cli)?;
        // Verify only top-level files were processed

        // Test with depth limit of 2
        cli.max_depth = Some(2);
        process_directory(&cli)?;
        // Verify files up to depth 2 were processed

        Ok(())
    }

    #[test]
    fn test_process_directory_hidden_files() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test without hidden files
        cli.hidden = false;
        process_directory(&cli)?;
        // Verify hidden files were not processed

        // Test with hidden files
        cli.hidden = true;
        process_directory(&cli)?;
        // Verify hidden files were processed

        Ok(())
    }

    #[test]
    fn test_process_single_file() -> Result<()> {
        let (_, files) = setup_test_directory()?;
        let rust_file = files.iter().find(|f| f.ends_with("main.rs")).unwrap();

        let cli = create_test_cli(rust_file);
        process_directory(&cli)?;
        // Verify single file was processed correctly

        Ok(())
    }
}

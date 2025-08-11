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

fn should_process_file(entry: &ignore::DirEntry, args: &Cli, base_path: &Path) -> bool {
    // Basic file checks
    if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
        return false;
    }

    let path = entry.path();
    let max_size = args.max_size.expect("max_size should be set from config");

    // Size check
    if !entry
        .metadata()
        .map(|m| m.len() <= max_size)
        .unwrap_or(false)
    {
        return false;
    }

    // Check if it's a source file
    let is_source = source_detection::is_source_file(path);

    // Check if it matches additional include patterns
    let matches_include = if let Some(ref includes) = args.include {
        matches_include_patterns(path, includes, base_path)
    } else {
        false
    };

    // Include if EITHER source file OR matches include patterns
    let should_include = is_source || matches_include;

    if !should_include {
        return false;
    }

    // Apply excludes to the union
    if let Some(ref excludes) = args.exclude {
        return !matches_exclude_patterns(path, excludes, base_path);
    }

    true
}

fn matches_include_patterns(path: &Path, includes: &[String], base_path: &Path) -> bool {
    let mut override_builder = OverrideBuilder::new(base_path);

    // Add include patterns (positive)
    for pattern in includes {
        if let Err(e) = override_builder.add(pattern) {
            eprintln!("Warning: Invalid include pattern '{pattern}': {e}");
        }
    }

    let overrides = override_builder.build().unwrap_or_else(|_| {
        // Return a default override that matches nothing if build fails
        OverrideBuilder::new(base_path).build().unwrap()
    });
    let match_result = overrides.matched(path, false);

    // Must be whitelisted and not ignored
    match_result.is_whitelist() && !match_result.is_ignore()
}

fn matches_exclude_patterns(path: &Path, excludes: &[Exclude], base_path: &Path) -> bool {
    let mut override_builder = OverrideBuilder::new(base_path);

    // Add exclude patterns (negative)
    for exclude in excludes {
        match exclude {
            Exclude::Pattern(pattern) => {
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
                // Handle file exclusions
                if file_path.is_absolute() {
                    if file_path.exists() {
                        if let Ok(relative_path) = file_path.strip_prefix(base_path) {
                            let pattern = format!("!{}", relative_path.display());
                            if let Err(e) = override_builder.add(&pattern) {
                                eprintln!(
                                    "Warning: Could not add file exclude pattern for '{}': {}",
                                    file_path.display(),
                                    e
                                );
                            }
                        }
                    }
                } else {
                    let pattern = format!("!{}", file_path.display());
                    if let Err(e) = override_builder.add(&pattern) {
                        eprintln!("Warning: Could not add file exclude pattern '{pattern}': {e}");
                    }
                }
            }
        }
    }

    let overrides = override_builder.build().unwrap_or_else(|_| {
        // Return a default override that matches nothing if build fails
        OverrideBuilder::new(base_path).build().unwrap()
    });
    let match_result = overrides.matched(path, false);

    match_result.is_ignore()
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
                let overrides = override_builder.build()?;
                builder.overrides(overrides);

                let dir_entries: Vec<FileEntry> = builder
                    .build()
                    .par_bridge()
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| should_process_file(entry, args, path))
                    .filter_map(|entry| process_file(&entry, path).ok())
                    .collect();

                all_entries.extend(dir_entries);
            } else if path.is_file() {
                // Process single file
                let entry = ignore::WalkBuilder::new(path)
                    .build()
                    .next()
                    .and_then(|r| r.ok());
                if let Some(entry) = entry {
                    if should_process_file(&entry, args, path.parent().unwrap_or(path)) {
                        if let Ok(file_entry) = process_file(&entry, path) {
                            all_entries.push(file_entry);
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
            assert_ne!(
                entry.path.extension().and_then(|ext| ext.to_str()),
                Some("rs"),
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

        // Test including additional Rust files (should get all source files)
        cli.include = Some(vec!["**/*.rs".to_string()]);
        let entries = process_entries(&cli)?;

        // Should include all source files (since .rs is already a source extension)
        assert!(!entries.is_empty(), "Should have found files");

        // Should include source files: .rs, .py, .md
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        // Test including a non-source extension as additional
        cli.include = Some(vec!["**/*.xyz".to_string()]);

        // Create a .xyz file
        fs::write(dir.path().join("test.xyz"), "data")?;

        let entries = process_entries(&cli)?;

        // Should include BOTH .xyz files AND normal source files
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"xyz")); // Additional pattern
        assert!(extensions.contains(&"rs")); // Normal source file
        assert!(extensions.contains(&"py")); // Normal source file
        assert!(extensions.contains(&"md")); // Normal source file

        Ok(())
    }

    #[test]
    fn test_process_directory_with_includes_and_excludes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test additional includes with excludes - should get all source files plus additional, minus excludes
        cli.include = Some(vec!["**/*.xyz".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/test.rs".to_string())]);

        // Create a .xyz file
        fs::write(dir.path().join("test.xyz"), "data")?;

        let entries = process_entries(&cli)?;

        // Should include all source files + .xyz files, but exclude test.rs
        assert!(!entries.is_empty(), "Should have found files");

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        // Should have .xyz (additional) plus source files (.rs, .py, .md)
        assert!(extensions.contains(&"xyz")); // Additional pattern
        assert!(extensions.contains(&"rs")); // Source files (but not test.rs)
        assert!(extensions.contains(&"py")); // Source files
        assert!(extensions.contains(&"md")); // Source files

        // Verify test.rs was excluded
        for entry in &entries {
            assert!(
                !entry.path.to_string_lossy().contains("test.rs"),
                "Found excluded test.rs file: {:?}",
                entry.path
            );
        }

        // Test excluding a directory
        cli.include = Some(vec!["**/*.xyz".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/nested/**".to_string())]);
        let entries = process_entries(&cli)?;

        // Should include source files + .xyz, but exclude nested directory
        for entry in &entries {
            assert!(
                !entry.path.to_string_lossy().contains("nested"),
                "Found file from excluded nested directory: {:?}",
                entry.path
            );
        }

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

    #[test]
    fn test_include_patterns_extend_source_detection() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        // Create a .peb file (not recognized by source detection)
        let peb_path = dir.path().join("template.peb");
        let mut peb_file = File::create(&peb_path)?;
        writeln!(peb_file, "template content")?;

        // Create a .xyz file (also not recognized)
        let xyz_path = dir.path().join("data.xyz");
        let mut xyz_file = File::create(&xyz_path)?;
        writeln!(xyz_file, "data content")?;

        let mut cli = create_test_cli(dir.path());

        // Test 1: Without include patterns, non-source files should be excluded
        cli.include = None;
        let entries = process_entries(&cli)?;
        assert!(!entries
            .iter()
            .any(|e| e.path.extension().and_then(|ext| ext.to_str()) == Some("peb")));
        assert!(!entries
            .iter()
            .any(|e| e.path.extension().and_then(|ext| ext.to_str()) == Some("xyz")));

        // Test 2: With include patterns, should ADD to source detection
        cli.include = Some(vec!["*.peb".to_string()]);
        let entries = process_entries(&cli)?;

        // Should include .peb files PLUS all normal source files
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"peb")); // Additional pattern
        assert!(extensions.contains(&"rs")); // Normal source file
        assert!(extensions.contains(&"py")); // Normal source file
        assert!(extensions.contains(&"md")); // Normal source file

        // Test 3: Multiple include patterns (additive)
        cli.include = Some(vec!["*.peb".to_string(), "*.xyz".to_string()]);
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"peb")); // Additional pattern
        assert!(extensions.contains(&"xyz")); // Additional pattern
        assert!(extensions.contains(&"rs")); // Normal source file
        assert!(extensions.contains(&"py")); // Normal source file
        assert!(extensions.contains(&"md")); // Normal source file

        // Test 4: Include + exclude patterns (union then subtract)
        cli.include = Some(vec!["*.peb".to_string(), "*.xyz".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("*.xyz".to_string())]);
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"peb")); // Additional pattern, not excluded
        assert!(!extensions.contains(&"xyz")); // Additional pattern, but excluded
        assert!(extensions.contains(&"rs")); // Normal source file, not excluded
        assert!(extensions.contains(&"py")); // Normal source file, not excluded
        assert!(extensions.contains(&"md")); // Normal source file, not excluded

        Ok(())
    }

    #[test]
    fn test_backward_compatibility_no_patterns() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        // Add some non-source files that should be ignored by default
        let binary_path = dir.path().join("binary.bin");
        fs::write(&binary_path, b"\x00\x01\x02\x03")?;

        let config_path = dir.path().join("config.conf");
        fs::write(&config_path, "key=value")?;

        let mut cli = create_test_cli(dir.path());

        // Test 1: No patterns specified - should only get source files
        cli.include = None;
        cli.exclude = None;
        let entries = process_entries(&cli)?;

        // Should find source files (.rs, .py, .md) but not .bin or .conf
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));
        assert!(!extensions.contains(&"bin"));
        assert!(!extensions.contains(&"conf"));

        // Test 2: Only exclude patterns - should work as before
        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;

        // Should still apply source detection, but exclude .rs files
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(!extensions.contains(&"rs")); // Excluded
        assert!(extensions.contains(&"py")); // Source file, not excluded
        assert!(!extensions.contains(&"bin")); // Not source file

        Ok(())
    }

    #[test]
    fn test_single_file_processing_with_patterns() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        // Create a .peb file
        let peb_path = dir.path().join("template.peb");
        fs::write(&peb_path, "template content")?;

        // Test 1: Single .peb file without include patterns - should be rejected
        let mut cli = create_test_cli(&peb_path);
        cli.paths = vec![peb_path.to_string_lossy().to_string()];
        cli.include = None;
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 0);

        // Test 2: Single .peb file WITH include patterns - should be accepted
        cli.include = Some(vec!["*.peb".to_string()]);
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 1);

        // Test 3: Single .rs file with exclude pattern - should be rejected
        let rs_path = dir.path().join("src/main.rs");
        cli.paths = vec![rs_path.to_string_lossy().to_string()];
        cli.include = None;
        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 0);

        Ok(())
    }

    #[test]
    fn test_pattern_edge_cases() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        // Create various test files
        fs::write(dir.path().join("test.peb"), "content")?;
        fs::write(dir.path().join("test.xyz"), "content")?;
        fs::write(dir.path().join("script.py"), "print('test')")?;

        let mut cli = create_test_cli(dir.path());

        // Test 1: Empty include patterns (edge case) - should still get source files
        cli.include = Some(vec![]);
        let entries = process_entries(&cli)?;
        // With empty include patterns, should still get source files
        assert!(!entries.is_empty());

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"rs")); // Source files should still be included
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        // Test 2: Include pattern that matches source files (additive)
        cli.include = Some(vec!["**/*.py".to_string()]);
        let entries = process_entries(&cli)?;
        // Should include source files + additional .py matches
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"py")); // Both existing and additional
        assert!(extensions.contains(&"rs")); // Source files
        assert!(extensions.contains(&"md")); // Source files

        // Test 3: Include everything, then exclude
        cli.include = Some(vec!["**/*".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;

        // Should include everything (.peb, .xyz, .py, .md from both source detection and include pattern) but not .rs files
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(extensions.contains(&"peb"));
        assert!(extensions.contains(&"xyz"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));
        assert!(!extensions.contains(&"rs"));

        Ok(())
    }

    #[test]
    fn test_invalid_patterns_handling() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test 1: Invalid glob pattern (this should not panic)
        cli.include = Some(vec!["[invalid".to_string()]);
        let _entries = process_entries(&cli)?;
        // Should handle gracefully, possibly matching nothing

        // Test 2: Mix of valid and invalid patterns
        cli.include = Some(vec![
            "**/*.rs".to_string(),
            "[invalid".to_string(),
            "**/*.py".to_string(),
        ]);
        let _entries = process_entries(&cli)?;
        // Should process valid patterns, ignore invalid ones

        Ok(())
    }

    #[test]
    fn test_include_patterns_are_additional() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        // Create a .peb file
        fs::write(dir.path().join("template.peb"), "template content")?;

        let mut cli = create_test_cli(dir.path());
        cli.include = Some(vec!["*.peb".to_string()]);

        let entries = process_entries(&cli)?;

        // Should include BOTH .peb files AND normal source files
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(extensions.contains(&"peb")); // Additional pattern
        assert!(extensions.contains(&"rs")); // Normal source file
        assert!(extensions.contains(&"py")); // Normal source file
        assert!(extensions.contains(&"md")); // Normal source file

        // Should be more than just the .peb file
        assert!(entries.len() > 1);

        Ok(())
    }
}

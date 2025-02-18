use crate::cli::{Cli, Exclude, OutputFormat, TokenizerType};
use crate::file_picker::FilePicker;
use crate::output::{
    display_token_counts, generate_output, generate_pdf, handle_output, FileEntry,
};
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
        .clone()
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
        for path in &args.paths {
            let path = std::path::Path::new(path);
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

    if let Some(pdf_path) = &args.pdf {
        let pdf_data = generate_pdf(&entries, args.output.clone().unwrap_or(OutputFormat::Both))?;
        fs::write(pdf_path, pdf_data)?;
        println!("PDF output written to: {}", pdf_path.display());
    } else {
        // Handle output (print/copy/save)
        let output = generate_output(&entries, output_format)?;
        handle_output(output, args)?;
    }

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
                            return true;
                        }
                    }
                }
                Exclude::File(path) => {
                    let matches = entry.path().ends_with(path);
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
            writeln!(file, "{}", content)?;
            created_files.push(full_path);
        }

        Ok((dir, created_files))
    }

    fn create_test_cli(dir_path: &Path) -> Cli {
        Cli {
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
        }
    }

    #[test]
    fn test_exclude_patterns() {
        let (dir, _files) = setup_test_directory().unwrap();
        let entry = ignore::WalkBuilder::new(dir.path())
            .build()
            .find(|e| e.as_ref().unwrap().path().ends_with("main.rs"))
            .unwrap()
            .unwrap();

        // Test various exclude patterns
        let test_cases = vec![
            // Pattern exclusions
            (Exclude::Pattern("**/*.rs".to_string()), true),
            (Exclude::Pattern("**/*.js".to_string()), false),
            (Exclude::Pattern("test/**".to_string()), false),
            // File exclusions
            (Exclude::File(PathBuf::from("main.rs")), true),
            (Exclude::File(PathBuf::from("nonexistent.rs")), false),
        ];

        for (exclude, should_exclude) in test_cases {
            let mut cli = create_test_cli(dir.path());
            cli.exclude = Some(vec![exclude]);
            assert_eq!(
                is_excluded(&entry, &cli),
                should_exclude,
                "Failed for exclude pattern: {:?}",
                cli.exclude
            );
        }
    }

    #[test]
    fn test_process_directory_with_excludes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test excluding all Rust files
        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let _ = process_directory(&cli)?;
        // Verify no .rs files were processed
        // This would need to be adapted based on how you want to verify the results

        // Test excluding specific directories
        cli.exclude = Some(vec![
            Exclude::Pattern("**/node_modules/**".to_string()),
            Exclude::Pattern("**/target/**".to_string()),
            Exclude::Pattern("**/.git/**".to_string()),
        ]);
        let _ = process_directory(&cli)?;
        // Verify excluded directories were not processed

        Ok(())
    }

    #[test]
    fn test_process_directory_with_includes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test including only Rust files
        cli.include = Some(vec!["**/*.rs".to_string()]);
        let _ = process_directory(&cli)?;
        // Verify only .rs files were processed

        // Test including multiple patterns
        cli.include = Some(vec!["**/*.rs".to_string(), "**/*.py".to_string()]);
        let _ = process_directory(&cli)?;
        // Verify only .rs and .py files were processed

        Ok(())
    }

    #[test]
    fn test_process_directory_depth_limit() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test with depth limit of 1
        cli.max_depth = Some(1);
        let _ = process_directory(&cli)?;
        // Verify only top-level files were processed

        // Test with depth limit of 2
        cli.max_depth = Some(2);
        let _ = process_directory(&cli)?;
        // Verify files up to depth 2 were processed

        Ok(())
    }

    #[test]
    fn test_process_directory_hidden_files() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        // Test without hidden files
        cli.hidden = false;
        let _ = process_directory(&cli)?;
        // Verify hidden files were not processed

        // Test with hidden files
        cli.hidden = true;
        let _ = process_directory(&cli)?;
        // Verify hidden files were processed

        Ok(())
    }

    #[test]
    fn test_process_single_file() -> Result<()> {
        let (_, files) = setup_test_directory()?;
        let rust_file = files.iter().find(|f| f.ends_with("main.rs")).unwrap();

        let cli = create_test_cli(rust_file);
        let _ = process_directory(&cli)?;
        // Verify single file was processed correctly

        Ok(())
    }
}

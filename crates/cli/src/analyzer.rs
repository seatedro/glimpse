use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::{overrides::OverrideBuilder, WalkBuilder};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use glimpse_core::{is_source_file, Exclude, FileEntry, OutputFormat, TokenCounter, TokenizerType};
use glimpse_tui::FilePicker;

use crate::cli::Cli;
use crate::output::{display_token_counts, generate_output, generate_pdf, handle_output};

pub fn process_directory(args: &Cli) -> Result<()> {
    if let Some(threads) = args.threads {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build_global()?;
    }

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Scanning files...");

    let output_format = args
        .get_output_format()
        .expect("output format should be set from config");
    let entries = process_entries(args)?;
    pb.finish();

    if let Some(pdf_path) = &args.pdf {
        let pdf_data = generate_pdf(
            &entries,
            args.get_output_format().unwrap_or(OutputFormat::Both),
        )?;
        fs::write(pdf_path, pdf_data)?;
        println!("PDF output written to: {}", pdf_path.display());
    } else {
        let project_name = if args.xml {
            Some(determine_project_name(&args.paths))
        } else {
            None
        };

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

        if path.is_dir() {
            if let Some(name) = path.file_name() {
                return name.to_string_lossy().to_string();
            }
        }

        if path.is_file() {
            if let Some(parent) = path.parent() {
                if let Some(name) = parent.file_name() {
                    return name.to_string_lossy().to_string();
                }
            }
        }

        first_path.clone()
    } else {
        "project".to_string()
    }
}

fn should_process_file(entry: &ignore::DirEntry, args: &Cli, base_path: &Path) -> bool {
    if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
        return false;
    }

    let path = entry.path();
    let max_size = args.max_size.expect("max_size should be set from config");

    if !entry
        .metadata()
        .map(|m| m.len() <= max_size)
        .unwrap_or(false)
    {
        return false;
    }

    if let Some(ref only_includes) = args.only_include {
        let matches_only_include = matches_include_patterns(path, only_includes, base_path);

        if !matches_only_include {
            return false;
        }

        if let Some(ref excludes) = args.exclude {
            return !matches_exclude_patterns(path, excludes, base_path);
        }

        return true;
    }

    let is_source = is_source_file(path);

    let matches_include = if let Some(ref includes) = args.include {
        matches_include_patterns(path, includes, base_path)
    } else {
        false
    };

    let should_include = is_source || matches_include;

    if !should_include {
        return false;
    }

    if let Some(ref excludes) = args.exclude {
        return !matches_exclude_patterns(path, excludes, base_path);
    }

    true
}

fn matches_include_patterns(path: &Path, includes: &[String], base_path: &Path) -> bool {
    let mut override_builder = OverrideBuilder::new(base_path);

    for pattern in includes {
        if let Err(e) = override_builder.add(pattern) {
            eprintln!("Warning: Invalid include pattern '{pattern}': {e}");
        }
    }

    let overrides = override_builder
        .build()
        .unwrap_or_else(|_| OverrideBuilder::new(base_path).build().unwrap());
    let match_result = overrides.matched(path, false);

    match_result.is_whitelist() && !match_result.is_ignore()
}

fn matches_exclude_patterns(path: &Path, excludes: &[Exclude], base_path: &Path) -> bool {
    let mut override_builder = OverrideBuilder::new(base_path);

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

    let overrides = override_builder
        .build()
        .unwrap_or_else(|_| OverrideBuilder::new(base_path).build().unwrap());
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

pub fn create_token_counter(args: &Cli) -> Result<TokenCounter> {
    let tokenizer_type = args.get_tokenizer_type().unwrap_or(TokenizerType::Tiktoken);

    match tokenizer_type {
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
        base_path.file_name().map(PathBuf::from).unwrap_or_default()
    } else {
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

    use crate::cli::CliOutputFormat;

    fn setup_test_directory() -> Result<(TempDir, Vec<PathBuf>)> {
        let dir = tempdir()?;
        let mut created_files = Vec::new();

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
            only_include: None,
            exclude: None,
            max_size: Some(10 * 1024 * 1024),
            max_depth: Some(10),
            output: Some(CliOutputFormat::Both),
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
            (Exclude::Pattern("**/*.rs".to_string()), true),
            (Exclude::Pattern("**/*.js".to_string()), false),
            (Exclude::Pattern("test/**".to_string()), false),
            (Exclude::File(main_rs_path.clone()), true),
            (Exclude::File(PathBuf::from("nonexistent.rs")), false),
        ];

        for (exclude, should_exclude) in test_cases {
            let mut override_builder = OverrideBuilder::new(dir.path());

            match &exclude {
                Exclude::Pattern(pattern) => {
                    let exclude_pattern = if !pattern.starts_with('!') {
                        format!("!{pattern}")
                    } else {
                        pattern.clone()
                    };
                    override_builder.add(&exclude_pattern).unwrap();
                }
                Exclude::File(file_path) => {
                    if file_path.exists() {
                        let rel_path = if file_path.is_absolute() {
                            file_path.strip_prefix(dir.path()).unwrap_or(file_path)
                        } else {
                            file_path
                        };
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

        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;

        for entry in &entries {
            assert_ne!(
                entry.path.extension().and_then(|ext| ext.to_str()),
                Some("rs"),
                "Found .rs file that should have been excluded: {:?}",
                entry.path
            );
        }

        cli.exclude = Some(vec![
            Exclude::Pattern("**/node_modules/**".to_string()),
            Exclude::Pattern("**/target/**".to_string()),
            Exclude::Pattern("**/.git/**".to_string()),
        ]);
        let entries = process_entries(&cli)?;

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

        cli.include = Some(vec!["**/*.rs".to_string()]);
        let entries = process_entries(&cli)?;

        assert!(!entries.is_empty(), "Should have found files");

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        cli.include = Some(vec!["**/*.xyz".to_string()]);
        fs::write(dir.path().join("test.xyz"), "data")?;

        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"xyz"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        Ok(())
    }

    #[test]
    fn test_process_directory_with_includes_and_excludes() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        cli.include = Some(vec!["**/*.xyz".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/test.rs".to_string())]);

        fs::write(dir.path().join("test.xyz"), "data")?;

        let entries = process_entries(&cli)?;

        assert!(!entries.is_empty(), "Should have found files");

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(extensions.contains(&"xyz"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        for entry in &entries {
            assert!(
                !entry.path.to_string_lossy().contains("test.rs"),
                "Found excluded test.rs file: {:?}",
                entry.path
            );
        }

        cli.include = Some(vec!["**/*.xyz".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/nested/**".to_string())]);
        let entries = process_entries(&cli)?;

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

        cli.max_depth = Some(1);
        process_directory(&cli)?;

        cli.max_depth = Some(2);
        process_directory(&cli)?;

        Ok(())
    }

    #[test]
    fn test_process_directory_hidden_files() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;
        let mut cli = create_test_cli(dir.path());

        cli.hidden = false;
        process_directory(&cli)?;

        cli.hidden = true;
        process_directory(&cli)?;

        Ok(())
    }

    #[test]
    fn test_process_single_file() -> Result<()> {
        let (_, files) = setup_test_directory()?;
        let rust_file = files.iter().find(|f| f.ends_with("main.rs")).unwrap();

        let cli = create_test_cli(rust_file);
        process_directory(&cli)?;

        Ok(())
    }

    #[test]
    fn test_include_patterns_extend_source_detection() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        let peb_path = dir.path().join("template.peb");
        let mut peb_file = File::create(&peb_path)?;
        writeln!(peb_file, "template content")?;

        let xyz_path = dir.path().join("data.xyz");
        let mut xyz_file = File::create(&xyz_path)?;
        writeln!(xyz_file, "data content")?;

        let mut cli = create_test_cli(dir.path());

        cli.include = None;
        let entries = process_entries(&cli)?;
        assert!(!entries
            .iter()
            .any(|e| e.path.extension().and_then(|ext| ext.to_str()) == Some("peb")));
        assert!(!entries
            .iter()
            .any(|e| e.path.extension().and_then(|ext| ext.to_str()) == Some("xyz")));

        cli.include = Some(vec!["*.peb".to_string()]);
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"peb"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        cli.include = Some(vec!["*.peb".to_string(), "*.xyz".to_string()]);
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"peb"));
        assert!(extensions.contains(&"xyz"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        cli.include = Some(vec!["*.peb".to_string(), "*.xyz".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("*.xyz".to_string())]);
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"peb"));
        assert!(!extensions.contains(&"xyz"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        Ok(())
    }

    #[test]
    fn test_backward_compatibility_no_patterns() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        let binary_path = dir.path().join("binary.bin");
        fs::write(&binary_path, b"\x00\x01\x02\x03")?;

        let config_path = dir.path().join("config.conf");
        fs::write(&config_path, "key=value")?;

        let mut cli = create_test_cli(dir.path());

        cli.include = None;
        cli.exclude = None;
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));
        assert!(!extensions.contains(&"bin"));
        assert!(!extensions.contains(&"conf"));

        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(!extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(!extensions.contains(&"bin"));

        Ok(())
    }

    #[test]
    fn test_single_file_processing_with_patterns() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        let peb_path = dir.path().join("template.peb");
        fs::write(&peb_path, "template content")?;

        let mut cli = create_test_cli(&peb_path);
        cli.paths = vec![peb_path.to_string_lossy().to_string()];
        cli.include = None;
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 0);

        cli.include = Some(vec!["*.peb".to_string()]);
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 1);

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

        fs::write(dir.path().join("test.peb"), "content")?;
        fs::write(dir.path().join("test.xyz"), "content")?;
        fs::write(dir.path().join("script.py"), "print('test')")?;

        let mut cli = create_test_cli(dir.path());

        cli.include = Some(vec![]);
        let entries = process_entries(&cli)?;
        assert!(!entries.is_empty());

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        cli.include = Some(vec!["**/*.py".to_string()]);
        let entries = process_entries(&cli)?;
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"md"));

        cli.include = Some(vec!["**/*".to_string()]);
        cli.exclude = Some(vec![Exclude::Pattern("**/*.rs".to_string())]);
        let entries = process_entries(&cli)?;

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

        cli.include = Some(vec!["[invalid".to_string()]);
        let _entries = process_entries(&cli)?;

        cli.include = Some(vec![
            "**/*.rs".to_string(),
            "[invalid".to_string(),
            "**/*.py".to_string(),
        ]);
        let _entries = process_entries(&cli)?;

        Ok(())
    }

    #[test]
    fn test_include_patterns_are_additional() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        fs::write(dir.path().join("template.peb"), "template content")?;

        let mut cli = create_test_cli(dir.path());
        cli.include = Some(vec!["*.peb".to_string()]);

        let entries = process_entries(&cli)?;

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();

        assert!(extensions.contains(&"peb"));
        assert!(extensions.contains(&"rs"));
        assert!(extensions.contains(&"py"));
        assert!(extensions.contains(&"md"));

        assert!(entries.len() > 1);

        Ok(())
    }

    #[test]
    fn test_only_include_replacement_behavior() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        fs::write(dir.path().join("config.conf"), "key=value")?;
        fs::write(dir.path().join("data.toml"), "[section]\nkey = 'value'")?;
        fs::write(dir.path().join("template.peb"), "template content")?;

        let mut cli = create_test_cli(dir.path());

        cli.only_include = Some(vec!["*.conf".to_string()]);
        let entries = process_entries(&cli)?;

        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.extension().and_then(|ext| ext.to_str()) == Some("conf"));

        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(!extensions.contains(&"rs"));
        assert!(!extensions.contains(&"py"));
        assert!(!extensions.contains(&"md"));
        assert!(!extensions.contains(&"toml"));

        cli.only_include = Some(vec!["*.conf".to_string(), "*.toml".to_string()]);
        let entries = process_entries(&cli)?;

        assert_eq!(entries.len(), 2);
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"conf"));
        assert!(extensions.contains(&"toml"));
        assert!(!extensions.contains(&"rs"));
        assert!(!extensions.contains(&"py"));

        cli.only_include = Some(vec![
            "*.conf".to_string(),
            "*.toml".to_string(),
            "*.peb".to_string(),
        ]);
        cli.exclude = Some(vec![Exclude::Pattern("*.toml".to_string())]);
        let entries = process_entries(&cli)?;

        assert_eq!(entries.len(), 2);
        let extensions: Vec<_> = entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(extensions.contains(&"conf"));
        assert!(extensions.contains(&"peb"));
        assert!(!extensions.contains(&"toml"));
        assert!(!extensions.contains(&"rs"));

        cli.only_include = Some(vec!["*.nonexistent".to_string()]);
        cli.exclude = None;
        let entries = process_entries(&cli)?;

        assert_eq!(entries.len(), 0);

        Ok(())
    }

    #[test]
    fn test_only_include_vs_include_behavior_difference() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        fs::write(dir.path().join("config.conf"), "key=value")?;

        let mut cli = create_test_cli(dir.path());

        cli.include = Some(vec!["*.conf".to_string()]);
        cli.only_include = None;
        let additive_entries = process_entries(&cli)?;

        let additive_extensions: Vec<_> = additive_entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(additive_extensions.contains(&"conf"));
        assert!(additive_extensions.contains(&"rs"));
        assert!(additive_extensions.contains(&"py"));
        assert!(additive_extensions.contains(&"md"));

        cli.include = None;
        cli.only_include = Some(vec!["*.conf".to_string()]);
        let replacement_entries = process_entries(&cli)?;

        assert_eq!(replacement_entries.len(), 1);
        assert!(
            replacement_entries[0]
                .path
                .extension()
                .and_then(|ext| ext.to_str())
                == Some("conf")
        );

        let replacement_extensions: Vec<_> = replacement_entries
            .iter()
            .filter_map(|e| e.path.extension().and_then(|ext| ext.to_str()))
            .collect();
        assert!(replacement_extensions.contains(&"conf"));
        assert!(!replacement_extensions.contains(&"rs"));
        assert!(!replacement_extensions.contains(&"py"));
        assert!(!replacement_extensions.contains(&"md"));

        assert!(additive_entries.len() > replacement_entries.len());

        Ok(())
    }

    #[test]
    fn test_only_include_single_file_processing() -> Result<()> {
        let (dir, _files) = setup_test_directory()?;

        let config_path = dir.path().join("config.conf");
        fs::write(&config_path, "key=value")?;

        let mut cli = create_test_cli(&config_path);
        cli.paths = vec![config_path.to_string_lossy().to_string()];

        cli.only_include = None;
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 0);

        cli.only_include = Some(vec!["*.conf".to_string()]);
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.extension().and_then(|ext| ext.to_str()) == Some("conf"));

        let rs_path = dir.path().join("src/main.rs");
        cli.paths = vec![rs_path.to_string_lossy().to_string()];
        cli.only_include = Some(vec!["*.conf".to_string()]);
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 0);

        cli.only_include = Some(vec!["*.rs".to_string()]);
        let entries = process_entries(&cli)?;
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path.extension().and_then(|ext| ext.to_str()) == Some("rs"));

        Ok(())
    }
}

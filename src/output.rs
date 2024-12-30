use crate::cli::Cli;
use anyhow::Result;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub size: u64,
}

pub fn generate_output(entries: &[FileEntry], format: &str) -> Result<String> {
    let mut output = String::new();

    match format {
        "tree" => {
            output.push_str("Directory Structure:\n");
            output.push_str(&generate_tree(entries)?);
        }
        "files" => {
            output.push_str("File Contents:\n");
            output.push_str(&generate_files(entries)?);
        }
        "both" => {
            output.push_str("Directory Structure:\n");
            output.push_str(&generate_tree(entries)?);
            output.push_str("\nFile Contents:\n");
            output.push_str(&generate_files(entries)?);
        }
        _ => output.push_str("Invalid output format specified\n"),
    }

    // Add summary
    output.push_str("\nSummary:\n");
    output.push_str(&format!("Total files: {}\n", entries.len()));
    output.push_str(&format!(
        "Total size: {} bytes\n",
        entries.iter().map(|e| e.size).sum::<u64>()
    ));

    Ok(output)
}

fn generate_tree(entries: &[FileEntry]) -> Result<String> {
    let mut output = String::new();
    let mut current_path = vec![];

    // Sort entries by path to ensure consistent output
    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.path.cmp(&b.path));

    for entry in &sorted_entries {
        let components: Vec<_> = entry.path.components().collect();

        for (i, component) in components.iter().enumerate() {
            if i >= current_path.len() || component != &current_path[i] {
                let prefix = "  ".repeat(i);
                // Always use └── for the last component of a file path
                if i == components.len() - 1 {
                    output.push_str(&format!(
                        "{}└── {}\n",
                        prefix,
                        component.as_os_str().to_string_lossy()
                    ));
                } else {
                    // For directories, check if it's the last one at this level
                    let is_last_dir = sorted_entries
                        .iter()
                        .filter_map(|e| e.path.components().nth(i))
                        .filter(|c| c != component)
                        .count()
                        == 0;

                    let prefix_char = if is_last_dir { "└" } else { "├" };
                    output.push_str(&format!(
                        "{}{}── {}/\n",
                        prefix,
                        prefix_char,
                        component.as_os_str().to_string_lossy()
                    ));
                }
            }
        }

        current_path = components;
    }

    Ok(output)
}

fn generate_files(entries: &[FileEntry]) -> Result<String> {
    let mut output = String::new();

    for entry in entries {
        output.push_str(&format!("\nFile: {}\n", entry.path.display()));
        output.push_str(&"=".repeat(48));
        output.push('\n');
        output.push_str(&entry.content);
        output.push('\n');
    }

    Ok(output)
}

pub fn handle_output(content: String, args: &Cli) -> Result<()> {
    // Print to stdout if no other output method is specified
    if args.print {
        println!("{}", content);
    }

    // Copy to clipboard if requested
    if !args.print {
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content.clone())) {
            Ok(_) => println!("Context prepared! Paste into your LLM of choice + Profit."),
            Err(e) => eprintln!("Warning: Failed to copy to clipboard: {}. Output will continue with other specified formats.", e),
        }
    }

    // Write to file if path provided
    if let Some(file_path) = &args.file {
        fs::write(file_path, content)?;
        println!("Output written to: {}", file_path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_entries() -> Vec<FileEntry> {
        vec![
            FileEntry {
                path: PathBuf::from("src/main.rs"),
                content: "fn main() {}\n".to_string(),
                size: 12,
            },
            FileEntry {
                path: PathBuf::from("src/lib/utils.rs"),
                content: "pub fn helper() {}\n".to_string(),
                size: 18,
            },
        ]
    }

    #[test]
    fn test_tree_output() {
        let entries = create_test_entries();
        let tree = generate_tree(&entries).unwrap();
        let expected = "└── src/\n  ├── lib/\n    └── utils.rs\n  └── main.rs\n";
        assert_eq!(
            tree, expected,
            "Tree output doesn't match expected structure"
        );
    }

    #[test]
    fn test_files_output() {
        let entries = create_test_entries();
        let files = generate_files(&entries).unwrap();
        let expected = format!(
            "\nFile: {}\n{}\n{}\n\nFile: {}\n{}\n{}\n",
            "src/main.rs",
            "=".repeat(48),
            "fn main() {}\n",
            "src/lib/utils.rs",
            "=".repeat(48),
            "pub fn helper() {}\n"
        );
        assert_eq!(files, expected);
    }

    #[test]
    fn test_generate_output() {
        let entries = create_test_entries();

        // Test tree format
        let tree_output = generate_output(&entries, "tree").unwrap();
        assert!(tree_output.contains("Directory Structure:"));
        assert!(tree_output.contains("src/"));
        assert!(tree_output.contains("main.rs"));

        // Test files format
        let files_output = generate_output(&entries, "files").unwrap();
        assert!(files_output.contains("File Contents:"));
        assert!(files_output.contains("fn main()"));
        assert!(files_output.contains("pub fn helper()"));

        // Test both format
        let both_output = generate_output(&entries, "both").unwrap();
        assert!(both_output.contains("Directory Structure:"));
        assert!(both_output.contains("File Contents:"));

        // Test invalid format
        let invalid_output = generate_output(&entries, "invalid").unwrap();
        assert!(invalid_output.contains("Invalid output format"));
    }

    #[test]
    fn test_handle_output() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_file = temp_dir.path().join("test_output.txt");

        let content = "Test content".to_string();
        let args = Cli {
            path: PathBuf::from("."),
            include: None,
            exclude: None,
            max_size: Some(1000),
            max_depth: Some(10),
            output: Some("both".to_string()),
            file: Some(temp_file.clone()),
            print: false,
            threads: None,
            hidden: false,
            no_ignore: false,
        };

        handle_output(content.clone(), &args).unwrap();

        // Verify file content
        let file_content = std::fs::read_to_string(temp_file).unwrap();
        assert_eq!(file_content, content);
    }
}

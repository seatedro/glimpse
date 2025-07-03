use crate::{
    cli::{Cli, OutputFormat},
    tokenizer::TokenCounter,
};
use anyhow::Result;
use base64::Engine;
use num_format::{Buffer, Locale};
use printpdf::*;
use std::{fs, io::BufWriter, path::PathBuf};

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub size: u64,
}

pub fn generate_output(
    entries: &[FileEntry],
    format: OutputFormat,
    xml_format: bool,
    project_name: Option<String>,
) -> Result<String> {
    let mut output = String::new();

    if xml_format {
        let project_name = project_name.unwrap_or_else(|| "project".to_string());
        output.push_str(&format!(
            "<context name=\"{}\">\n",
            xml_escape(&project_name)
        ));
    }

    match format {
        OutputFormat::Tree => {
            if xml_format {
                output.push_str("<tree>\n");
            } else {
                output.push_str("Directory Structure:\n");
            }
            output.push_str(&generate_tree(entries)?);
            if xml_format {
                output.push_str("</tree>\n");
            }
        }
        OutputFormat::Files => {
            if xml_format {
                output.push_str("<files>\n");
            } else {
                output.push_str("File Contents:\n");
            }
            output.push_str(&generate_files(entries, xml_format)?);
            if xml_format {
                output.push_str("</files>\n");
            }
        }
        OutputFormat::Both => {
            if xml_format {
                output.push_str("<tree>\n");
            } else {
                output.push_str("Directory Structure:\n");
            }
            output.push_str(&generate_tree(entries)?);
            if xml_format {
                output.push_str("</tree>\n\n<files>\n");
            } else {
                output.push_str("\nFile Contents:\n");
            }
            output.push_str(&generate_files(entries, xml_format)?);
            if xml_format {
                output.push_str("</files>\n");
            }
        }
    }

    // Add summary
    if xml_format {
        output.push_str("<summary>\n");
        output.push_str(&format!("Total files: {}\n", entries.len()));
        output.push_str(&format!(
            "Total size: {} bytes\n",
            entries.iter().map(|e| e.size).sum::<u64>()
        ));
        output.push_str("</summary>\n");
    } else {
        output.push_str("\nSummary:\n");
        output.push_str(&format!("Total files: {}\n", entries.len()));
        output.push_str(&format!(
            "Total size: {} bytes\n",
            entries.iter().map(|e| e.size).sum::<u64>()
        ));
    }

    if xml_format {
        output.push_str("</context>");
    }

    Ok(output)
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn display_token_counts(token_counter: TokenCounter, entries: &[FileEntry]) -> Result<()> {
    let token_count = token_counter.count_files(entries)?;

    let mut buf = Buffer::default();
    let locale = Locale::en;
    buf.write_formatted(&token_count.total_tokens, &locale);

    println!("\nToken Count Summary:");
    println!("Total tokens: {}", buf.as_str());
    println!("\nBreakdown by file:");

    // Sorting breakdown
    let mut breakdown = token_count.breakdown;
    breakdown.sort_by(|(_, a), (_, b)| b.cmp(a));
    let top_files = breakdown.iter().take(15);

    for (path, count) in top_files {
        buf.write_formatted(count, &locale);
        println!("  {}: {}", path.display(), buf.as_str());
    }

    Ok(())
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

fn generate_files(entries: &[FileEntry], xml_format: bool) -> Result<String> {
    let mut output = String::new();

    for entry in entries {
        if xml_format {
            output.push_str(&format!(
                "<file path=\"{}\">\n",
                xml_escape(entry.path.display().to_string().as_str())
            ));
            output.push_str(&"=".repeat(48));
            output.push('\n');
            output.push_str(&entry.content);
            output.push('\n');
            output.push_str("</file>\n");
        } else {
            output.push_str(&format!("\nFile: {}\n", entry.path.display()));
            output.push_str(&"=".repeat(48));
            output.push('\n');
            output.push_str(&entry.content);
            output.push('\n');
        }
    }

    Ok(output)
}

fn try_copy_with_osc52(content: &str) -> Result<(), Box<dyn std::error::Error>> {
    // OSC 52 sequence to set clipboard for special cases (like SSH)
    print!(
        "\x1B]52;c;{}\x07",
        base64::engine::general_purpose::STANDARD.encode(content)
    );
    Ok(())
}

pub fn handle_output(content: String, args: &Cli) -> Result<()> {
    // Print to stdout if no other output method is specified
    if args.print {
        println!("{content}");
    }

    // Copy to clipboard if requested
    if !args.print {
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(content.clone())) {
            Ok(_) => println!("Context prepared! Paste into your LLM of choice + Profit."),
            Err(_) => {
                match try_copy_with_osc52(&content) {
                    Ok(_) => println!("Context prepared! (using terminal clipboard) Paste into your LLM of choice + Profit."),
                    Err(e) => eprintln!("Warning: Failed to copy to clipboard: {e}. Output will continue with other specified formats.")
                }
            },
        }
    }

    // Write to file if path provided
    if let Some(file_path) = &args.file {
        fs::write(file_path, content)?;
        println!("Output written to: {}", file_path.display());
    }

    Ok(())
}

pub fn generate_pdf(entries: &[FileEntry], format: OutputFormat) -> Result<Vec<u8>> {
    let (doc, page1, layer1) = PdfDocument::new("Source Code", Mm(210.0), Mm(297.0), "Layer 1");
    let mut current_layer = doc.get_page(page1).get_layer(layer1);

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)?;
    let mut y_position = 280.0;

    // Add tree if specified
    match format {
        OutputFormat::Tree | OutputFormat::Both => {
            current_layer.use_text(
                "Directory Structure:",
                14.0,
                Mm(10.0),
                Mm(y_position),
                &font,
            );
            y_position -= 10.0;

            let tree = generate_tree(entries)?;
            for line in tree.lines() {
                if y_position < 20.0 {
                    let (page2, layer2) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
                    current_layer = doc.get_page(page2).get_layer(layer2);
                    y_position = 280.0;
                }
                current_layer.use_text(line, 10.0, Mm(10.0), Mm(y_position), &font);
                y_position -= 5.0;
            }

            // New page for files
            let (next_page, next_layer) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
            current_layer = doc.get_page(next_page).get_layer(next_layer);
        }
        _ => {}
    }

    for entry in entries {
        // Start at top of each new page
        y_position = 280.0;

        // Add file path as header
        current_layer.use_text(
            format!("File: {}", entry.path.display()),
            14.0,
            Mm(10.0),
            Mm(y_position),
            &font,
        );
        y_position -= 10.0;

        // Add separator line
        current_layer.use_text("=".repeat(48), 12.0, Mm(10.0), Mm(y_position), &font);
        y_position -= 10.0;

        // Add file content in smaller font
        for line in entry.content.lines() {
            if y_position < 20.0 {
                // Create new page when we run out of space
                let (page2, layer2) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
                current_layer = doc.get_page(page2).get_layer(layer2);
                y_position = 280.0;
            }

            current_layer.use_text(line, 10.0, Mm(10.0), Mm(y_position), &font);
            y_position -= 5.0;
        }

        // Create new page for next file
        let (next_page, next_layer) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
        current_layer = doc.get_page(next_page).get_layer(next_layer);
    }

    // Save to memory buffer
    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(&mut buffer))?;
    Ok(buffer)
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
        let files = generate_files(&entries, false).unwrap();
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
        let tree_output = generate_output(&entries, OutputFormat::Tree, false, None).unwrap();
        assert!(tree_output.contains("Directory Structure:"));
        assert!(tree_output.contains("src/"));
        assert!(tree_output.contains("main.rs"));

        // Test files format
        let files_output = generate_output(&entries, OutputFormat::Files, false, None).unwrap();
        assert!(files_output.contains("File Contents:"));
        assert!(files_output.contains("fn main()"));
        assert!(files_output.contains("pub fn helper()"));

        // Test both format
        let both_output = generate_output(&entries, OutputFormat::Both, false, None).unwrap();
        assert!(both_output.contains("Directory Structure:"));
        assert!(both_output.contains("File Contents:"));
    }

    #[test]
    fn test_xml_output() {
        let entries = create_test_entries();

        // Test XML tree format
        let xml_tree_output = generate_output(
            &entries,
            OutputFormat::Tree,
            true,
            Some("test_project".to_string()),
        )
        .unwrap();
        assert!(xml_tree_output.contains("<context name=\"test_project\">"));
        assert!(xml_tree_output.contains("<tree>"));
        assert!(xml_tree_output.contains("</tree>"));
        assert!(xml_tree_output.contains("<summary>"));
        assert!(xml_tree_output.contains("</summary>"));
        assert!(xml_tree_output.contains("</context>"));

        // Test XML files format
        let xml_files_output = generate_output(
            &entries,
            OutputFormat::Files,
            true,
            Some("test_project".to_string()),
        )
        .unwrap();
        assert!(xml_files_output.contains("<context name=\"test_project\">"));
        assert!(xml_files_output.contains("<files>"));
        assert!(xml_files_output.contains("<file path=\"src/main.rs\">"));
        assert!(xml_files_output.contains("</file>"));
        assert!(xml_files_output.contains("</files>"));
        assert!(xml_files_output.contains("</context>"));

        // Test XML both format
        let xml_both_output = generate_output(
            &entries,
            OutputFormat::Both,
            true,
            Some("test_project".to_string()),
        )
        .unwrap();
        assert!(xml_both_output.contains("<context name=\"test_project\">"));
        assert!(xml_both_output.contains("<tree>"));
        assert!(xml_both_output.contains("</tree>"));
        assert!(xml_both_output.contains("<files>"));
        assert!(xml_both_output.contains("</files>"));
        assert!(xml_both_output.contains("</context>"));
    }

    #[test]
    fn test_handle_output() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let temp_file = temp_dir.path().join("test_output.txt");

        let content = "Test content".to_string();
        let args = Cli {
            config: false,
            paths: vec![".".to_string()],
            include: None,
            exclude: None,
            max_size: Some(1000),
            max_depth: Some(10),
            output: Some(OutputFormat::Both),
            file: Some(temp_file.clone()),
            print: false,
            threads: None,
            hidden: false,
            no_ignore: false,
            no_tokens: true,
            model: None,
            tokenizer: Some(crate::cli::TokenizerType::Tiktoken),
            tokenizer_file: None,
            interactive: false,
            pdf: None,
            traverse_links: false,
            link_depth: None,
            config_path: false,
            xml: false,
        };

        handle_output(content.clone(), &args).unwrap();

        // Verify file content
        let file_content = std::fs::read_to_string(temp_file).unwrap();
        assert_eq!(file_content, content);
    }
}

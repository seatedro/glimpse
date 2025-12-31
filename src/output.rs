use std::fs;
use std::io::BufWriter;

use anyhow::Result;
use base64::Engine;
use num_format::{Buffer, Locale};
use printpdf::*;

use glimpse::{FileEntry, OutputFormat, TokenCounter};

use crate::cli::Cli;

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

    let mut sorted_entries = entries.to_vec();
    sorted_entries.sort_by(|a, b| a.path.cmp(&b.path));

    for entry in &sorted_entries {
        let components: Vec<_> = entry.path.components().collect();

        for (i, component) in components.iter().enumerate() {
            if i >= current_path.len() || component != &current_path[i] {
                let prefix = "  ".repeat(i);
                if i == components.len() - 1 {
                    output.push_str(&format!(
                        "{}└── {}\n",
                        prefix,
                        component.as_os_str().to_string_lossy()
                    ));
                } else {
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
    print!(
        "\x1B]52;c;{}\x07",
        base64::engine::general_purpose::STANDARD.encode(content)
    );
    Ok(())
}

pub fn handle_output(content: String, args: &Cli) -> Result<()> {
    if args.print {
        println!("{content}");
    }

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

            let (next_page, next_layer) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
            current_layer = doc.get_page(next_page).get_layer(next_layer);
        }
        _ => {}
    }

    for entry in entries {
        y_position = 280.0;

        current_layer.use_text(
            format!("File: {}", entry.path.display()),
            14.0,
            Mm(10.0),
            Mm(y_position),
            &font,
        );
        y_position -= 10.0;

        current_layer.use_text("=".repeat(48), 12.0, Mm(10.0), Mm(y_position), &font);
        y_position -= 10.0;

        for line in entry.content.lines() {
            if y_position < 20.0 {
                let (page2, layer2) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
                current_layer = doc.get_page(page2).get_layer(layer2);
                y_position = 280.0;
            }

            current_layer.use_text(line, 10.0, Mm(10.0), Mm(y_position), &font);
            y_position -= 5.0;
        }

        let (next_page, next_layer) = doc.add_page(Mm(210.0), Mm(297.0), "New Layer");
        current_layer = doc.get_page(next_page).get_layer(next_layer);
    }

    let mut buffer = Vec::new();
    doc.save(&mut BufWriter::new(&mut buffer))?;
    Ok(buffer)
}

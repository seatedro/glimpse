use anyhow::Result;
use colored::*;
use std::path::PathBuf;

#[derive(Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub size: u64,
}

pub fn generate_output(entries: &[FileEntry], format: &str) -> Result<(), anyhow::Error> {
    let _ = match format {
        "tree" => print_tree(entries),
        "files" => print_files(entries),
        "both" => {
            print_tree(entries)?;
            println!("\n{}", "File Contents:".green().bold());
            return print_files(entries);
        }
        _ => Err(anyhow::anyhow!("Invalid output format specified")),
    }?;

    // Print summary
    println!("\n{}", "Summary:".green().bold());
    println!("Total files: {}", entries.len());
    println!(
        "Total size: {} bytes",
        entries.iter().map(|e| e.size).sum::<u64>()
    );

    Ok(())
}

fn print_tree(entries: &[FileEntry]) -> Result<()> {
    println!("{}", "Directory Structure:".green().bold());

    let mut current_path = vec![];
    for entry in entries {
        let components: Vec<_> = entry.path.components().collect();

        // Print directory structure
        for (i, component) in components.iter().enumerate() {
            if i >= current_path.len() || component != &current_path[i] {
                let prefix = "  ".repeat(i);
                if i == components.len() - 1 {
                    println!("{}└── {}", prefix, component.as_os_str().to_string_lossy());
                } else {
                    println!("{}├── {}/", prefix, component.as_os_str().to_string_lossy());
                }
            }
        }

        current_path = components;
    }

    Ok(())
}

fn print_files(entries: &[FileEntry]) -> Result<()> {
    for entry in entries {
        println!("\n{} {}", "File:".blue().bold(), entry.path.display());
        println!("{}", "=".repeat(48));
        println!("{}", entry.content);
    }
    Ok(())
}

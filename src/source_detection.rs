use std::fs;
use std::path::Path;

// Include the generated code
include!(concat!(env!("OUT_DIR"), "/languages.rs"));

/// Extract interpreter from shebang line and exec pattern
fn extract_interpreter(data: &str) -> Option<String> {
    let lines: Vec<&str> = data.lines().take(2).collect();

    if !lines.first().is_some_and(|l| l.starts_with("#!")) {
        return None;
    }

    if let Some(second_line) = lines.get(1) {
        if second_line.contains("exec") {
            let parts: Vec<&str> = second_line.split_whitespace().collect();
            if let Some(pos) = parts.iter().position(|&x| x == "exec") {
                if let Some(&interpreter) = parts.get(pos + 1) {
                    return Some(interpreter.to_string());
                }
            }
        }
    }

    let path = lines[0].trim_start_matches("#!").trim();
    if path.is_empty() {
        return None;
    }

    let first_part = path.split_whitespace().next()?;
    if first_part.len() <= 1 {
        return None;
    }

    let mut script = first_part.split('/').next_back()?.to_string();

    // Handle /usr/bin/env
    if script == "env" {
        for part in path.split_whitespace().skip(1) {
            if !part.starts_with('-') && !part.contains('=') {
                script = part.to_string();
                break;
            }
        }
        // If we only found env with no valid interpreter, return None
        if script == "env" {
            return None;
        }
    }

    // Strip version numbers (python2.7 -> python2)
    if let Some(idx) = script.find(|c: char| c.is_ascii_digit()) {
        if let Some(dot_idx) = script[idx..].find('.') {
            script.truncate(idx + dot_idx);
        }
    }

    Some(script)
}

/// Detect language from shebang
fn detect_by_shebang(data: &str) -> bool {
    extract_interpreter(data)
        .map(|script| INTERPRETER_NAMES.contains(script.as_str()))
        .unwrap_or(false)
}

/// Checks if a given path is a source file
pub fn is_source_file(path: &Path) -> bool {
    // Check known filenames first
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if KNOWN_FILENAMES.contains(name) {
            return true;
        }
    }

    // Then check extension
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if SOURCE_EXTENSIONS.contains(ext.to_lowercase().as_str()) {
            return true;
        }
    }

    // Finally check shebang
    match fs::read_to_string(path) {
        Ok(content) => detect_by_shebang(&content),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_interpreter_extraction() {
        let cases = vec![
            // Basic cases
            ("#!/usr/bin/python", Some("python")),
            ("#!/bin/bash", Some("bash")),
            // env cases with flags and vars
            ("#!/usr/bin/env python", Some("python")),
            ("#!/usr/bin/env -S python3 -u", Some("python3")),
            ("#!/usr/bin/env FOO=bar python", Some("python")),
            // Version stripping
            ("#!/usr/bin/python2.7", Some("python2")),
            ("#!/usr/bin/ruby1.9.3", Some("ruby1")),
            // exec patterns
            ("#!/bin/sh\nexec python \"$0\" \"$@\"", Some("python")),
            // Invalid cases
            ("no shebang", None),
            ("#!/", None),
            ("", None),
        ];

        for (input, expected) in cases {
            assert_eq!(
                extract_interpreter(input).as_deref(),
                expected,
                "Failed for input: {input}"
            );
        }
    }

    #[test]
    fn test_source_detection() {
        let dir = tempdir().unwrap();

        // Test cases: (filename, content, expected)
        let test_cases = vec![
            // Extensions
            ("test.rs", "", true),
            ("test.py", "", true),
            ("test.js", "", true),
            // Known filenames
            ("Makefile", "", true),
            ("Dockerfile", "", true),
            // Shebangs
            ("script", "#!/usr/bin/env python\nprint('hi')", true),
            ("run", "#!/usr/bin/node\nconsole.log()", true),
            // Non-source files
            ("random.xyz", "", false),
            ("not-script", "just some text", false),
        ];

        for (name, content, expected) in test_cases {
            let path = dir.path().join(name);
            let mut file = File::create(&path).unwrap();
            writeln!(file, "{content}").unwrap();

            assert_eq!(
                is_source_file(&path),
                expected,
                "Failed for file: {}",
                path.display()
            );
        }
    }
}

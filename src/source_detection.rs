// src/source_detection.rs

use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::path::Path;

// Using Lazy for zero-cost initialization of our static set
static SOURCE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();

    // Systems Programming
    set.extend(&[
        "rs",  // Rust
        "c",   // C
        "h",   // C header
        "cpp", // C++
        "hpp", // C++ header
        "cc",  // C++
        "cxx", // C++
        "hxx", // C++ header
        "zig", // Zig
        "nim", // Nim
        "d",   // D
    ]);

    // Web - Frontend
    set.extend(&[
        "js",     // JavaScript
        "jsx",    // React
        "ts",     // TypeScript
        "tsx",    // React + TypeScript
        "vue",    // Vue
        "svelte", // Svelte
        "html",   // HTML
        "css",    // CSS
        "scss",   // SASS
        "sass",   // SASS
        "less",   // LESS
        "mjs",    // ES modules
        "cjs",    // CommonJS
    ]);

    // Web - Backend
    set.extend(&[
        "php",    // PHP
        "rb",     // Ruby
        "erb",    // Ruby templates
        "py",     // Python
        "pyi",    // Python interface
        "go",     // Go
        "java",   // Java
        "scala",  // Scala
        "kt",     // Kotlin
        "kts",    // Kotlin script
        "groovy", // Groovy
        "cs",     // C#
        "fs",     // F#
        "fsx",    // F# script
        "ex",     // Elixir
        "exs",    // Elixir script
        "erl",    // Erlang
        "hrl",    // Erlang header
    ]);

    // Mobile
    set.extend(&[
        "swift", // Swift
        "m",     // Objective-C
        "mm",    // Objective-C++
        "dart",  // Dart/Flutter
    ]);

    // Shell/Scripts
    set.extend(&[
        "sh",   // Shell
        "bash", // Bash
        "zsh",  // Zsh
        "fish", // Fish
        "ps1",  // PowerShell
        "psm1", // PowerShell module
        "bat",  // Batch
        "cmd",  // Command script
        "pl",   // Perl
        "pm",   // Perl module
        "t",    // Perl test
        "lua",  // Lua
        "tcl",  // Tcl
    ]);

    // Data/Config
    set.extend(&[
        "json",    // JSON
        "yaml",    // YAML
        "yml",     // YAML
        "toml",    // TOML
        "xml",     // XML
        "proto",   // Protocol Buffers
        "graphql", // GraphQL
        "sql",     // SQL
    ]);

    // Documentation
    set.extend(&[
        "md",  // Markdown
        "mdx", // MDX
        "rst", // reStructuredText
        "tex", // LaTeX
        "org", // Org mode
    ]);

    // Other Languages
    set.extend(&[
        "r",    // R
        "jl",   // Julia
        "ml",   // OCaml
        "mli",  // OCaml interface
        "hs",   // Haskell
        "lhs",  // Literate Haskell
        "clj",  // Clojure
        "cljs", // ClojureScript
        "cljc", // Clojure Common
        "bf",   // Brainfuck (why not?)
        "lisp", // Lisp
        "el",   // Emacs Lisp
        "vim",  // Vim script
    ]);

    // Build/Config specific
    set.extend(&[
        "cmake",       // CMake
        "make",        // Makefile
        "mak",         // Makefile
        "nix",         // Nix
        "dockerfile",  // Docker
        "vagrantfile", // Vagrant
    ]);

    set
});

/// Checks if a given path is a source code file based on its extension
pub fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get the total number of supported extensions
pub fn supported_extension_count() -> usize {
    SOURCE_EXTENSIONS.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_source_detection() {
        let test_cases = vec![
            ("test.rs", true),
            ("test.py", true),
            ("test.js", true),
            ("test.xyz", false),
            ("test", false),
            ("test.txt", false),
            ("test.RSS", true), // Should be case insensitive
            ("test.PY", true),  // Should be case insensitive
        ];

        for (file, expected) in test_cases {
            let path = PathBuf::from(file);
            assert_eq!(is_source_file(&path), expected, "Failed for {}", file);
        }
    }
}

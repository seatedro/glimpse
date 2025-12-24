# CLAUDE.md - AI Assistant Guide for Glimpse

## Project Overview

Glimpse is a fast Rust CLI tool for extracting codebase content into LLM-friendly formats. It's designed to help users prepare source code for loading into Large Language Models with built-in token counting, tree visualization, and multiple output formats.

**Key capabilities:**
- Fast parallel file processing using Rayon
- Directory tree visualization
- Source code content extraction
- Token counting (tiktoken/HuggingFace backends)
- Git repository cloning and processing
- Web page scraping with Markdown conversion
- Interactive TUI file picker
- XML and PDF output formats
- Per-repository configuration via `.glimpse` files

## Codebase Structure

```
glimpse/
├── src/
│   ├── main.rs           # Entry point, CLI arg handling, routing
│   ├── cli.rs            # CLI argument definitions using clap
│   ├── config.rs         # Global and repo-level configuration
│   ├── analyzer.rs       # Core file processing logic
│   ├── source_detection.rs # Source file detection (extensions, shebangs)
│   ├── output.rs         # Output formatting (tree, files, XML, PDF)
│   ├── tokenizer.rs      # Token counting backends
│   ├── git_processor.rs  # Git repository cloning
│   ├── url_processor.rs  # Web page fetching and HTML→Markdown
│   └── file_picker.rs    # Interactive TUI file selector
├── build.rs              # Build script that generates languages.rs from languages.yml
├── languages.yml         # Language definitions (extensions, filenames, interpreters)
├── Cargo.toml            # Dependencies and package metadata
├── .github/workflows/
│   ├── test.yml          # CI: tests, clippy, formatting
│   └── release.yml       # CD: multi-platform builds, publishing
└── test_project/         # Test fixtures
```

## Development Commands

```bash
# Build and run
cargo build
cargo run -- [OPTIONS] [PATH]

# Run tests
cargo test

# Check code quality (required to pass CI)
cargo clippy -- -D warnings
cargo fmt -- --check

# Format code
cargo fmt

# Build release
cargo build --release
```

## Key Architecture Decisions

### Source File Detection
Detection happens in `source_detection.rs` via `is_source_file()`:
1. Check known filenames (Makefile, Dockerfile, etc.)
2. Check file extensions against `SOURCE_EXTENSIONS`
3. Fall back to shebang parsing for scripts

Extension/filename data is code-generated at build time from `languages.yml` via `build.rs`.

### Include/Exclude Pattern Behavior
- `--include` (or `-i`): **Additive** - patterns are added to default source detection
- `--only-include`: **Replacement** - only specified patterns are used, ignoring source detection
- `--exclude` (or `-e`): Applied after inclusion, works with both modes

### Token Counting
Two backends available in `tokenizer.rs`:
- `TokenizerType::Tiktoken` (default) - Uses `tiktoken-rs` for OpenAI-compatible counting
- `TokenizerType::HuggingFace` - Uses `tokenizers` crate for HuggingFace models

### Configuration Hierarchy
1. Global config: `~/.config/glimpse/config.toml` (Linux/macOS) or `%APPDATA%\glimpse\config.toml` (Windows)
2. Repo config: `.glimpse` file in project root
3. CLI arguments (highest priority)

### Output Formats
- Default: Copies to clipboard
- `-p/--print`: Outputs to stdout
- `-f/--file [PATH]`: Writes to file (default: `GLIMPSE.md`)
- `-x/--xml`: Wraps output in XML tags for better LLM parsing
- `--pdf PATH`: Generates PDF output

## Testing Conventions

- Unit tests are co-located with source code in `#[cfg(test)]` modules
- Integration tests use `tempfile` for isolated filesystem testing
- Tests should handle network-dependent operations gracefully (see `git_processor.rs` tests)
- Mock servers used for URL processing tests via `mockito`

Example test pattern:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_feature() -> Result<()> {
        let dir = tempdir()?;
        // Test logic using temp directory
        Ok(())
    }
}
```

## CI/CD Pipeline

### Test Workflow (`.github/workflows/test.yml`)
Runs on push/PR to `master`:
- `cargo test --verbose`
- `cargo clippy -- -D warnings`
- `cargo fmt -- --check`

### Release Workflow (`.github/workflows/release.yml`)
Triggered by version tags (`v*`):
1. Creates GitHub release
2. Builds binaries for: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`
3. Uploads release assets
4. Updates Homebrew tap
5. Publishes to crates.io

## Code Style Guidelines

- Write terse, self-commenting code
- Comments only on docstrings for functions
- Follow standard Rust formatting (`cargo fmt`)
- Use `anyhow::Result` for error handling in application code
- Prefer `?` operator over explicit `match` for error propagation
- Use `#[derive]` macros for common traits

## Version Control

Use jujutsu (`jj`) instead of git for all version control operations.

```bash
jj status
jj diff
jj new -m "message"
jj describe -m "message"
jj bookmark set <name>
jj git push
```

## Common Patterns

### Adding a new CLI option
1. Add field to `Cli` struct in `cli.rs` with appropriate `#[arg(...)]` attributes
2. Handle the option in `main.rs` routing logic
3. Update `RepoConfig` in `config.rs` if it should be saveable

### Adding file type support
Edit `languages.yml` to add extensions, filenames, or interpreters. The build script will regenerate detection code automatically.

### Modifying output format
Edit `output.rs`:
- `generate_tree()` for tree structure
- `generate_files()` for file contents
- `generate_output()` orchestrates the full output

## Important Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive macros |
| `rayon` | Parallel file processing |
| `ignore` | .gitignore-aware file walking |
| `tiktoken-rs` | OpenAI tokenizer |
| `tokenizers` | HuggingFace tokenizer |
| `git2` | Git repository operations |
| `scraper` | HTML parsing for web processing |
| `ratatui` | Terminal UI for file picker |
| `arboard` | Clipboard access |
| `printpdf` | PDF generation |

## Debugging Tips

- Use `--print` to see output directly instead of clipboard
- Use `--no-tokens` to skip tokenizer initialization during debugging
- For file selection issues, check `.gitignore` patterns with `--no-ignore`
- For hidden file issues, use `-H/--hidden`

## Version Bumping

Version is defined in `Cargo.toml`. When releasing:
1. Update version in `Cargo.toml`
2. Commit: `jj new -m "bump: vX.Y.Z"`
3. Tag: `jj git push && git tag vX.Y.Z && git push --tags`

The release workflow handles the rest automatically.

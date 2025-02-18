# Glimpse

A blazingly fast tool for peeking at codebases. Perfect for loading your codebase into an LLM's context, with built-in token counting support.

## Features

- 🚀 Fast parallel file processing
- 🌳 Tree-view of codebase structure
- 📝 Source code content viewing
- 🔢 Token counting with multiple backends
- ⚙️ Configurable defaults
- 📋 Clipboard support
- 🎨 Customizable file type detection
- 🥷 Respects .gitignore automatically
- 🔗 Web content processing with Markdown conversion
- 📦 Git repository support
- 🌐 URL traversal with configurable depth

## Installation

Using cargo:
```bash
cargo install glimpse
```

Using homebrew:
```bash
brew tap seatedro/glimpse
brew install glimpse
```

Using Nix:
```bash
# Install directly
nix profile install github:seatedro/glimpse

# Or use in your flake
{
  inputs.glimpse.url = "github:seatedro/glimpse";
}
```

Using an AUR helper:
```bash
# Using yay
yay -S glimpse

# Using paru
paru -S glimpse
```

## Usage

Basic usage:
```bash
# Process a local directory
glimpse /path/to/project

# Process multiple files
glimpse file1 file2 file3

# Process a Git repository
glimpse https://github.com/username/repo.git

# Process a web page and convert to Markdown
glimpse https://example.com/docs

# Process a web page and its linked pages
glimpse https://example.com/docs --traverse-links --link-depth 2
```

Common options:
```bash
# Show hidden files
glimpse -H /path/to/project

# Only show tree structure
glimpse -o tree /path/to/project

# Copy output to clipboard
glimpse -c /path/to/project

# Save output to file
glimpse -f output.txt /path/to/project

# Include specific file types
glimpse -i "*.rs,*.go" /path/to/project

# Exclude patterns
glimpse -e "target/*,dist/*" /path/to/project

# Count tokens using tiktoken (OpenAI's tokenizer)
glimpse /path/to/project

# Use HuggingFace tokenizer with specific model
glimpse --tokenizer huggingface --model gpt2 /path/to/project

# Use custom local tokenizer file
glimpse --tokenizer huggingface --tokenizer-file /path/to/tokenizer.json /path/to/project

# Process a Git repository and save as PDF
glimpse https://github.com/username/repo.git --pdf output.pdf
```

## CLI Options

```
Usage: glimpse [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory/Files/URLs to analyze [default: .]

Options:
      --interactive              Opens interactive file picker (? for help)
  -i, --include <PATTERNS>       Additional patterns to include (e.g. "*.rs,*.go")
  -e, --exclude <PATTERNS>       Additional patterns to exclude
  -s, --max-size <BYTES>         Maximum file size in bytes
      --max-depth <DEPTH>        Maximum directory depth to traverse
  -o, --output <FORMAT>          Output format: tree, files, or both
  -f, --file <PATH>              Save output to specified file
  -p, --print                    Print to stdout instead of clipboard
  -t, --threads <COUNT>          Number of threads for parallel processing
  -H, --hidden                   Show hidden files and directories
      --no-ignore                Don't respect .gitignore files
      --no-tokens                Disable token counting
      --tokenizer <TYPE>         Tokenizer to use: tiktoken or huggingface
      --model <NAME>             Model name for HuggingFace tokenizer
      --tokenizer-file <PATH>    Path to local tokenizer file
      --traverse-links           Traverse links when processing URLs
      --link-depth <DEPTH>       Maximum depth to traverse links (default: 1)
      --pdf <PATH>               Save output as PDF
  -h, --help                     Print help
  -V, --version                  Print version
```

## Configuration

Glimpse uses a config file located at:
- Linux/macOS: `~/.config/glimpse/config.toml`
- Windows: `%APPDATA%\glimpse\config.toml`

Example configuration:
```toml
# General settings
max_size = 10485760  # 10MB
max_depth = 20
default_output_format = "both"

# Token counting settings
default_tokenizer = "tiktoken"       # Can be "tiktoken" or "huggingface"
default_tokenizer_model = "gpt2"     # Default model for HuggingFace tokenizer

# URL processing settings
traverse_links = false               # Whether to traverse links by default
default_link_depth = 1               # Default depth for link traversal

# Default exclude patterns
default_excludes = [
    "**/.git/**",
    "**/target/**",
    "**/node_modules/**"
]
```

## Token Counting

Glimpse supports two tokenizer backends:

1. Tiktoken (Default): OpenAI's tokenizer implementation, perfect for accurately estimating tokens for GPT models.

2. HuggingFace Tokenizers: Supports any model from the HuggingFace hub or local tokenizer files, great for custom models or other ML frameworks.

The token count appears in both file content views and the final summary, helping you estimate context window usage for large language models.

Example token count output:
```
File: src/main.rs
Tokens: 245
==================================================
// File contents here...

Summary:
Total files: 10
Total size: 15360 bytes
Total tokens: 2456
```

## Troubleshooting

1. **File too large**: Adjust `max_size` in config
2. **Missing files**: Check `hidden` flag and exclude patterns
3. **Performance issues**: Try adjusting thread count with `-t`
4. **Tokenizer errors**: 
   - For HuggingFace models, ensure you have internet connection for downloading
   - For local tokenizer files, verify the file path and format
   - Try using the default tiktoken backend if issues persist

## License

MIT

## Features in Detail

### Git Repository Support
Glimpse can directly process Git repositories from popular hosting services:
- GitHub repositories
- GitLab repositories
- Bitbucket repositories
- Azure DevOps repositories
- Any Git repository URL (ending with .git)

The repository is cloned to a temporary directory, processed, and automatically cleaned up.

### Web Content Processing
Glimpse can process web pages and convert them to Markdown:
- Preserves heading structure
- Converts links (both relative and absolute)
- Handles code blocks and quotes
- Supports nested lists
- Processes images and tables

With link traversal enabled, Glimpse can also process linked pages up to a specified depth, making it perfect for documentation sites and wikis.

### PDF Output
Any processed content (local files, Git repositories, or web pages) can be saved as a PDF with:
- Preserved formatting
- Syntax highlighting
- Table of contents
- Page numbers
- Custom headers and footers

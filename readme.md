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
- 📁 Local per-repo configuration with `.glimpse` file
- 🔗 Web content processing with Markdown conversion
- 📦 Git repository support
- 🌐 URL traversal with configurable depth
- 🏷️ XML output format for better LLM compatibility

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

On first use in a repository, Glimpse will save a `.glimpse` configuration file locally with your specified options. This file can be referenced on subsequent runs, or overridden by passing options again.

Common options:
```bash
# Show hidden files
glimpse -H /path/to/project

# Only show tree structure
glimpse -o tree /path/to/project

# Save output to GLIMPSE.md (default if no path given)
glimpse -f /path/to/project

# Save output to a specific file
glimpse -f output.txt /path/to/project

# Print output to stdout instead of copying to clipboard
glimpse -p /path/to/project

# Include specific file types
glimpse -i "*.rs,*.go" /path/to/project

# Exclude patterns or files
glimpse -e "target/*,dist/*" /path/to/project

# Count tokens using tiktoken (OpenAI's tokenizer)
glimpse /path/to/project

# Use HuggingFace tokenizer with specific model
glimpse --tokenizer huggingface --model gpt2 /path/to/project

# Use custom local tokenizer file
glimpse --tokenizer huggingface --tokenizer-file /path/to/tokenizer.json /path/to/project

# Process a Git repository and save as PDF
glimpse https://github.com/username/repo.git --pdf output.pdf

# Open interactive file picker
glimpse --interactive /path/to/project

# Print the config file path and exit
glimpse --config_path

# Initialize a .glimpse config file in the current directory
glimpse --config

# Output in XML format for better LLM compatibility
glimpse -x /path/to/project
```

## CLI Options

```
Usage: glimpse [OPTIONS] [PATH]

Arguments:
  [PATH]  Files, directories, or URLs to analyze [default: .]

Options:
      --config_path                Print the config file path and exit
      --config                     Init glimpse config file in current directory
      --interactive                Opens interactive file picker (? for help)
  -i, --include <PATTERNS>         Additional patterns to include (e.g. "*.rs,*.go")
  -e, --exclude <PATTERNS|PATHS>   Additional patterns or files to exclude
  -s, --max-size <BYTES>           Maximum file size in bytes
      --max-depth <DEPTH>          Maximum directory depth to traverse
  -o, --output <FORMAT>            Output format: tree, files, or both
  -f, --file [<PATH>]              Save output to specified file (default: GLIMPSE.md)
  -p, --print                      Print to stdout instead of copying to clipboard
  -t, --threads <COUNT>            Number of threads for parallel processing
  -H, --hidden                     Show hidden files and directories
      --no-ignore                  Don't respect .gitignore files
      --no-tokens                  Disable token counting
      --tokenizer <TYPE>           Tokenizer to use: tiktoken or huggingface
      --model <NAME>               Model name for HuggingFace tokenizer
      --tokenizer-file <PATH>      Path to local tokenizer file
      --traverse-links             Traverse links when processing URLs
      --link-depth <DEPTH>         Maximum depth to traverse links (default: 1)
      --pdf <PATH>                 Save output as PDF
  -x, --xml                        Output in XML format for better LLM compatibility
  -h, --help                       Print help
  -V, --version                    Print version
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

## XML Output Format

Glimpse supports XML output format designed for better compatibility with Large Language Models (LLMs) like Claude, GPT, and others. When using the `-x` or `--xml` flag, the output is structured with clear XML tags that help LLMs better understand the context and structure of your codebase.

### XML Structure

The XML output wraps all content in a `<context>` tag with the project name:

```xml
<context name="my_project">
<tree>
└── src/
  └── main.rs
</tree>

<files>
<file path="src/main.rs">
================================================
fn main() {
    println!("Hello, World!");
}
</file>
</files>

<summary>
Total files: 1
Total size: 45 bytes
</summary>
</context>
```

### Benefits for LLM Usage

- **Clear Context Boundaries**: The `<context>` wrapper helps LLMs understand where your codebase begins and ends
- **Structured Information**: Separate sections for directory tree, file contents, and summary
- **Proper Escaping**: XML-safe content that won't confuse parsers
- **Project Identification**: Automatic project name detection for better context

### Usage Examples

```bash
# Basic XML output
glimpse -x /path/to/project

# XML output with file save
glimpse -x -f project.xml /path/to/project

# XML output to stdout
glimpse -x --print /path/to/project

# XML output with specific includes
glimpse -x -i "*.rs,*.py" /path/to/project
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

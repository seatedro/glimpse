# Glimpse

A blazingly fast tool for peeking at codebases. Perfect for loading your codebase into an LLM's context.

## Features

- 🚀 Fast parallel file processing
- 🌳 Tree-view of codebase structure
- 📝 Source code content viewing
- ⚙️ Configurable defaults
- 📋 Clipboard support
- 🎨 Customizable file type detection
- 🥷 Respects .gitignore automatically

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

## Usage

Basic usage:
```bash
glimpse /path/to/project
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
```

## CLI Options

```
Usage: glimpse [OPTIONS] [PATH]

Arguments:
  [PATH]  Directory to analyze [default: .]

Options:
  -i, --include <PATTERNS>    Additional patterns to include (e.g. "*.rs,*.go")
  -e, --exclude <PATTERNS>    Additional patterns to exclude
  -s, --max-size <BYTES>      Maximum file size in bytes
      --max-depth <DEPTH>     Maximum directory depth to traverse
  -o, --output <FORMAT>       Output format: tree, files, or both
  -f, --file <PATH>          Save output to specified file
  -p, --print               Print to stdout instead of clipboard
  -t, --threads <COUNT>      Number of threads for parallel processing
  -H, --hidden              Show hidden files and directories
      --no-ignore           Don't respect .gitignore files
      --tokens              Show token count estimates
  -h, --help                Print help
  -V, --version             Print version
```

## Configuration

Glimpse uses a config file located at:
- Linux/macOS: `~/.config/glimpse/config.toml`
- Windows: `%APPDATA%\glimpse\config.toml`

Example configuration:
```toml
max_size = 10485760  # 10MB
max_depth = 20
default_output_format = "both"
default_excludes = [
    "**/.git/**",
    "**/target/**",
    "**/node_modules/**"
]
```

## Troubleshooting

1. **File too large**: Adjust `max_size` in config
2. **Missing files**: Check `hidden` flag and exclude patterns
3. **Performance issues**: Try adjusting thread count with `-t`

## License

MIT

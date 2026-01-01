# Glimpse Development Guide

A blazingly fast tool for peeking at codebases. Perfect for loading your codebase into an LLM's context.

## Task Tracking

Check `.todo.md` for current tasks and next steps. Keep it updated:
- Mark items `[x]` when completed
- Add new tasks as they're discovered
- Reference it before asking "what's next?"

## Commits

Use `jj` for version control. Always commit after completing a phase:

```bash
jj commit -m "feat: add glimpse-code crate scaffolding"
```

Use conventional commit prefixes:
- `feat` - new feature
- `fix` - bug fix
- `refactor` - restructure without behavior change
- `chore` - maintenance, dependencies, config
- `docs` - documentation only
- `test` - adding or updating tests

## Build Commands

```bash
cargo build                    # debug build
cargo build --release          # release build
cargo run -- <args>            # run with arguments
cargo run -- .                 # analyze current directory
cargo run -- --help            # show help
```

## Test Commands

```bash
cargo test                              # run all tests
cargo test test_name                    # run single test by name
cargo test test_name -- --nocapture     # run test with stdout
cargo test -- --test-threads=1         # run tests sequentially
```

## Lint & Format

```bash
cargo fmt                      # format all code
cargo fmt -- --check           # check formatting (CI)
cargo clippy                   # run linter
cargo clippy -- -D warnings    # fail on warnings (CI)
```

## Project Structure

```
glimpse/
├── src/
│   ├── main.rs        # binary entry point
│   ├── lib.rs         # library root
│   ├── cli.rs         # CLI arg parsing
│   ├── analyzer.rs    # directory processing
│   ├── output.rs      # output formatting
│   ├── core/          # config, tokenizer, types, source detection
│   ├── fetch/         # git clone, url/html processing
│   ├── tui/           # file picker
│   └── code/          # code analysis (extract, graph, index, resolve)
├── tests/             # integration tests
├── languages.yml      # language definitions for source detection
├── registry.toml      # tree-sitter grammar registry
└── build.rs           # generates language data from languages.yml
```

## Code Style

### No Comments

Code should be self-documenting. The only acceptable documentation is:
- Brief `///` docstrings on public API functions that aren't obvious
- `//!` module-level docs when necessary

```rust
// BAD: explaining what code does
// Check if the file is a source file
if is_source_file(path) { ... }

// BAD: inline comments
let name = path.file_name(); // get the filename

// GOOD: self-documenting code, no comments needed
if is_source_file(path) { ... }

// GOOD: docstring for non-obvious public function
/// Extract interpreter from shebang line and exec pattern
fn extract_interpreter(data: &str) -> Option<String> { ... }
```

### Import Order

Group imports in this order, separated by blank lines:
1. `std` library
2. External crates (alphabetical)
3. Internal crates - prefer `super::` over `crate::` when possible

```rust
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::types::FileEntry;      // preferred for sibling modules
use crate::config::Config;        // only when super:: won't reach
```

### Error Handling

- Use `anyhow::Result` for fallible functions
- Propagate errors with `?` operator
- Use `.expect("message")` only when failure is a bug
- Never use `.unwrap()` outside of tests
- Use `anyhow::bail!` for early returns with errors

### Naming Conventions

- `snake_case` for functions, methods, variables, modules
- `PascalCase` for types, traits, enums
- `SCREAMING_SNAKE_CASE` for constants
- Prefer descriptive names over abbreviations
- Boolean functions: `is_`, `has_`, `can_`, `should_`

### Type Definitions

- Derive common traits: `Debug`, `Clone`, `Serialize`, `Deserialize`
- Put derives in consistent order
- Use `pub` sparingly - only what's needed

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub size: u64,
}
```

### Function Style

- Keep functions focused and small
- Use early returns for guard clauses
- Prefer iterators and combinators over loops when clearer
- Use `impl Trait` for return types when appropriate

### Testing

- Tests live in `#[cfg(test)] mod tests` at bottom of file
- Use descriptive test names: `test_<what>_<condition>`
- Use `tempfile` for filesystem tests
- Group related assertions

### Patterns to Follow

- Use `Option` combinators: `.map()`, `.and_then()`, `.unwrap_or()`
- Use `Result` combinators: `.map_err()`, `.context()`
- Prefer `&str` over `String` in function parameters
- Use `impl AsRef<Path>` for path parameters when flexible
- Use builders for complex configuration

### Patterns to Avoid

- Comments explaining what code does (code should be obvious)
- Deeply nested code (use early returns)
- Magic numbers (use named constants)
- `clone()` when borrowing works
- `Box<dyn Error>` (use `anyhow::Error`)
- Panicking in library code

## Adding New Language Support

To add support for a new programming language, you need to:
1. Find the tree-sitter grammar repository
2. Add the language configuration to `registry.toml`
3. Write and test tree-sitter queries
4. Verify the language exists in `languages.yml` (for file detection)

### Step 1: Find the Tree-Sitter Grammar

Check the [tree-sitter wiki](https://github.com/tree-sitter/tree-sitter/wiki/List-of-parsers) for available parsers. Look for:
- Official parsers under `tree-sitter/` org
- Community parsers under `tree-sitter-grammars/` org
- Language-specific orgs (e.g., `nix-community/tree-sitter-nix`)

Clone the grammar repo to examine its structure:

```bash
# Using repo-explorer or manually
git clone https://github.com/<org>/tree-sitter-<lang>
```

Key files to examine:
- `grammar.js` - the grammar definition
- `src/node-types.json` - all node types in the AST
- `queries/tags.scm` - existing tag queries (if any)
- `queries/highlights.scm` - syntax highlighting queries (helpful reference)

### Step 2: Add to registry.toml

Add a new `[[language]]` section to `registry.toml`:

```toml
[[language]]
name = "mylang"
extensions = ["ml", "mli"]
repo = "https://github.com/org/tree-sitter-mylang"
branch = "master"
symbol = "tree_sitter_mylang"  # C symbol name from bindings
color = "#HEXCOLOR"            # from languages.yml

definition_query = """
# Query for function/method definitions
"""

call_query = """
# Query for function calls
"""

import_query = """
# Query for imports/includes
"""

[language.lsp]
binary = "mylang-lsp"
args = []
```

### Step 3: Write Tree-Sitter Queries

Queries use S-expression syntax. Key captures:
- `@name` - the function/symbol name (required)
- `@body` - the function body
- `@doc` - documentation comments
- `@qualifier` - object/module for qualified calls (e.g., `obj` in `obj.method()`)
- `@path` - import path
- `@function.definition` / `@reference.call` / `@import` - node type markers

#### Definition Query Pattern

```scheme
(
  (comment)* @doc
  .
  (function_definition
    name: (identifier) @name
    body: (_) @body) @function.definition
)
```

#### Call Query Pattern

```scheme
(call_expression
  function: [
    (identifier) @name
    (member_expression
      object: (_) @qualifier
      property: (identifier) @name)
  ]) @reference.call
```

#### Import Query Pattern

```scheme
(import_statement
  source: (string) @path) @import
```

### Step 4: Test Queries

**Always test queries before committing.** Use the tree-sitter CLI:

```bash
# Install tree-sitter CLI if needed
nix-shell -p tree-sitter nodejs python3

# Navigate to the grammar repo
cd /path/to/tree-sitter-mylang

# Generate the parser (if needed)
tree-sitter generate

# Write your query to a .scm file
cat > queries/test-definition.scm << 'EOF'
(function_definition
  name: (identifier) @name) @function.definition
EOF

# Test against a sample file
tree-sitter query queries/test-definition.scm sample.ml
```

Create a comprehensive test file that covers:
- Simple function definitions
- Functions with various argument patterns
- Nested functions
- Method definitions (if applicable)
- Different call patterns (simple, qualified, chained)
- Various import styles

Example test output:

```
sample.ml
  pattern: 0
    capture: 1 - function.definition, start: (5, 2), end: (5, 24)
    capture: 0 - name, start: (5, 2), end: (5, 12), text: `myFunction`
```

### Step 5: Verify Language Detection

Ensure the language exists in `languages.yml` with correct:
- `extensions` - file extensions
- `type: programming`
- `language_id` - unique ID

Most common languages are already in `languages.yml` (sourced from GitHub Linguist).

### Step 6: Build and Test

```bash
# Build glimpse
cargo build

# Test on a real file
cargo run -- code path/to/file.ml:function_name

# Test with callers
cargo run -- code path/to/file.ml:function_name --callers -d 2
```

### Query Writing Tips

1. **Use alternatives `[...]`** for multiple patterns:
   ```scheme
   (call_expression
     function: [
       (identifier) @name
       (member_expression property: (identifier) @name)
     ])
   ```

2. **Use predicates** for filtering:
   ```scheme
   ((identifier) @name
    (#eq? @name "import"))
   
   ((identifier) @name
    (#match? @name "^fetch.*"))
   ```

3. **Handle optional nodes** with `?`:
   ```scheme
   (function_definition
     name: (identifier) @name
     parameters: (parameters)? @params)
   ```

4. **Anchor with `.`** for adjacent siblings:
   ```scheme
   (comment)* @doc
   .
   (function_definition) @function
   ```

5. **Examine the AST** when queries don't match:
   ```bash
   tree-sitter parse sample.ml
   ```

### Common Pitfalls

- **Wrong node names**: Always check `src/node-types.json` for exact names
- **Missing field names**: Some nodes use positional children, not named fields
- **Nested structures**: Languages with currying or chaining need multiple patterns
- **External scanner**: Some grammars have custom scanners in `src/scanner.c`

### LSP Configuration

#### Finding Download URLs

1. Check the LSP's GitHub releases page for pre-built binaries
2. Use the GitHub API to get exact asset names:
   ```bash
   curl -s https://api.github.com/repos/OWNER/REPO/releases/latest | jq '.assets[].name'
   ```
3. Identify the URL pattern and platform-specific target names

#### Install Methods (in priority order)

| Method | Config Field | Requirement | Example |
|--------|--------------|-------------|---------|
| URL download | `url_template` | None | rust-analyzer, lua-language-server |
| npm | `npm_package` | npm or bun | pyright, typescript-language-server |
| go | `go_package` | go toolchain | gopls |
| cargo | `cargo_crate` | cargo | nil |

If no install method is configured, users must install the LSP manually.

#### Configuration Options

```toml
[language.lsp]
binary = "lsp-server"           # executable name
args = ["--stdio"]              # CLI arguments

# URL-based download (preferred when binaries available)
version = "1.0.0"
url_template = "https://github.com/org/repo/releases/download/{version}/lsp-{version}-{target}.tar.gz"
archive = "tar.gz"              # or "zip", "gz", "tar.xz"
binary_path = "bin/server"      # path within archive (optional)

# Package manager installs (fallback when no binaries)
npm_package = "pkg-name"        # install via npm/bun
go_package = "pkg/path@latest"  # install via go
cargo_crate = "crate-name"      # install via cargo

[language.lsp.targets]          # map rust target triple to release asset name
"x86_64-unknown-linux-gnu" = "linux-x64"
"aarch64-unknown-linux-gnu" = "linux-arm64"
"x86_64-apple-darwin" = "darwin-x64"
"aarch64-apple-darwin" = "darwin-arm64"
```

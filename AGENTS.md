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

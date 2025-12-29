use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::index::{Definition, DefinitionKind, Index, Span};

const SYSTEM_HEADERS: &[&str] = &[
    "stdio",
    "stdlib",
    "string",
    "math",
    "time",
    "errno",
    "assert",
    "ctype",
    "signal",
    "stdarg",
    "stddef",
    "setjmp",
    "locale",
    "limits",
    "float",
    "iso646",
    "stdbool",
    "stdint",
    "inttypes",
    "wchar",
    "wctype",
    "fenv",
    "complex",
    "tgmath",
    "stdalign",
    "stdnoreturn",
    "stdatomic",
    "threads",
    "uchar",
    "iostream",
    "vector",
    "string",
    "map",
    "set",
    "unordered_map",
    "unordered_set",
    "algorithm",
    "memory",
    "functional",
    "utility",
    "tuple",
    "array",
    "deque",
    "list",
    "forward_list",
    "stack",
    "queue",
    "priority_queue",
    "bitset",
    "valarray",
    "regex",
    "random",
    "chrono",
    "ratio",
    "thread",
    "mutex",
    "condition_variable",
    "future",
    "atomic",
    "filesystem",
    "optional",
    "variant",
    "any",
    "string_view",
    "charconv",
    "execution",
    "span",
    "ranges",
    "numbers",
    "concepts",
    "coroutine",
    "compare",
    "version",
    "source_location",
    "format",
    "bit",
    "numbers",
    "typeinfo",
    "typeindex",
    "type_traits",
    "initializer_list",
    "new",
    "exception",
    "stdexcept",
    "system_error",
    "cerrno",
    "cassert",
    "cctype",
    "cfenv",
    "cfloat",
    "cinttypes",
    "climits",
    "clocale",
    "cmath",
    "csetjmp",
    "csignal",
    "cstdarg",
    "cstddef",
    "cstdint",
    "cstdio",
    "cstdlib",
    "cstring",
    "ctime",
    "cuchar",
    "cwchar",
    "cwctype",
    "codecvt",
    "fstream",
    "iomanip",
    "ios",
    "iosfwd",
    "istream",
    "ostream",
    "sstream",
    "streambuf",
    "syncstream",
    "iterator",
    "locale",
    "numeric",
    "limits",
    "unistd",
    "fcntl",
    "sys/",
    "pthread",
    "netinet/",
    "arpa/",
    "dirent",
    "dlfcn",
    "poll",
    "sched",
    "semaphore",
    "spawn",
    "termios",
];

fn is_system_header(path: &str) -> bool {
    let clean = path.trim_matches(|c| c == '<' || c == '>' || c == '"');
    SYSTEM_HEADERS.iter().any(|s| clean.starts_with(s))
}

fn normalize_to_patterns(import_path: &str, lang: &str) -> Vec<String> {
    let clean = import_path.trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>');

    match lang {
        "rust" | "rs" => normalize_rust_import(clean),
        "python" | "py" => normalize_python_import(clean),
        "go" => normalize_go_import(clean),
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "mjs" | "cjs" | "jsx" => {
            normalize_js_import(clean)
        }
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => normalize_c_import(clean),
        "java" => normalize_java_import(clean),
        "scala" | "sc" => normalize_scala_import(clean),
        "zig" => normalize_zig_import(clean),
        _ => vec![format!("**/{}", clean)],
    }
}

fn normalize_rust_import(path: &str) -> Vec<String> {
    let stripped = path
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::");

    let parts: Vec<&str> = stripped.split("::").filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return vec![];
    }

    let file_path = parts.join("/");
    vec![
        format!("**/{}.rs", file_path),
        format!("**/{}/mod.rs", file_path),
        format!("**/src/{}.rs", file_path),
        format!("**/src/{}/mod.rs", file_path),
    ]
}

fn normalize_python_import(path: &str) -> Vec<String> {
    if path.starts_with('.') {
        return vec![];
    }

    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return vec![];
    }

    let file_path = parts.join("/");
    vec![
        format!("**/{}.py", file_path),
        format!("**/{}/__init__.py", file_path),
        format!("**/src/{}.py", file_path),
        format!("**/src/{}/__init__.py", file_path),
    ]
}

fn normalize_go_import(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();

    let local_parts: Vec<&str> = if parts.len() >= 3
        && (parts[0].contains('.') || parts[0] == "github" || parts[0] == "golang")
    {
        parts[3..].to_vec()
    } else {
        parts
    };

    if local_parts.is_empty() {
        return vec![];
    }

    let dir_path = local_parts.join("/");
    vec![
        format!("{}/*.go", dir_path),
        format!("**/{}/*.go", dir_path),
    ]
}

fn normalize_js_import(path: &str) -> Vec<String> {
    if path.starts_with('.') {
        let clean = path.trim_start_matches("./").trim_start_matches("../");
        return vec![
            format!("**/{}.ts", clean),
            format!("**/{}.tsx", clean),
            format!("**/{}.js", clean),
            format!("**/{}.jsx", clean),
            format!("**/{}/index.ts", clean),
            format!("**/{}/index.tsx", clean),
            format!("**/{}/index.js", clean),
        ];
    }

    let clean = path.trim_start_matches("@/").trim_start_matches('@');
    let parts: Vec<&str> = clean.split('/').collect();

    if parts.is_empty() {
        return vec![];
    }

    let file_path = parts.join("/");
    vec![
        format!("**/{}.ts", file_path),
        format!("**/{}.tsx", file_path),
        format!("**/{}.js", file_path),
        format!("**/{}.jsx", file_path),
        format!("**/{}/index.ts", file_path),
        format!("**/{}/index.tsx", file_path),
        format!("**/{}/index.js", file_path),
        format!("**/src/{}.ts", file_path),
        format!("**/src/{}.tsx", file_path),
    ]
}

fn normalize_c_import(path: &str) -> Vec<String> {
    if is_system_header(path) {
        return vec![];
    }

    let clean = path.trim_matches(|c| c == '"' || c == '<' || c == '>');
    vec![
        format!("**/{}", clean),
        format!("**/include/{}", clean),
        format!("**/src/{}", clean),
    ]
}

fn normalize_java_import(path: &str) -> Vec<String> {
    if path.starts_with("java.") || path.starts_with("javax.") || path.starts_with("sun.") {
        return vec![];
    }

    let file_path = path.replace('.', "/");
    vec![
        format!("**/{}.java", file_path),
        format!("**/src/{}.java", file_path),
        format!("**/src/main/java/{}.java", file_path),
    ]
}

fn normalize_scala_import(path: &str) -> Vec<String> {
    if path.starts_with("scala.") || path.starts_with("java.") {
        return vec![];
    }

    let clean = path.trim_end_matches("._").trim_end_matches(".*");
    let file_path = clean.replace('.', "/");
    vec![
        format!("**/{}.scala", file_path),
        format!("**/{}.sc", file_path),
        format!("**/src/{}.scala", file_path),
        format!("**/src/main/scala/{}.scala", file_path),
    ]
}

fn normalize_zig_import(path: &str) -> Vec<String> {
    if path == "std" {
        return vec![];
    }

    if path.ends_with(".zig") || path.contains('/') {
        return vec![format!("**/{}", path), format!("**/src/{}", path)];
    }

    vec![format!("**/{}.zig", path), format!("**/src/{}.zig", path)]
}

fn search_patterns(patterns: &[String], root: &Path) -> Option<PathBuf> {
    for pattern in patterns {
        let full_pattern = root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        if let Ok(paths) = glob::glob(&pattern_str) {
            for entry in paths.flatten() {
                if entry.is_file() {
                    return Some(entry);
                }
            }
        }
    }
    None
}

fn resolve_relative(import_path: &str, from_file: &Path, lang: &str) -> Option<PathBuf> {
    let from_dir = from_file.parent()?;
    let clean = import_path.trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>');

    match lang {
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => {
            if is_system_header(import_path) {
                return None;
            }
            let candidate = from_dir.join(clean);
            if candidate.exists() {
                return Some(candidate);
            }
            let parent = from_dir.parent()?;
            let candidate = parent.join(clean);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "mjs" | "cjs" | "jsx" => {
            if !clean.starts_with('.') {
                return None;
            }
            let base = from_dir.join(clean.trim_start_matches("./"));
            for ext in &["ts", "tsx", "js", "jsx"] {
                let candidate = base.with_extension(ext);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
            let index = base.join("index");
            for ext in &["ts", "tsx", "js", "jsx"] {
                let candidate = index.with_extension(ext);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        "zig" => {
            if clean.ends_with(".zig") || clean.contains('/') {
                let candidate = from_dir.join(clean);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        _ => {}
    }

    None
}

struct DefinitionPattern {
    extensions: &'static [&'static str],
    pattern: &'static str,
}

const DEFINITION_PATTERNS: &[DefinitionPattern] = &[
    DefinitionPattern {
        extensions: &["rs"],
        pattern: r"fn\s+{NAME}\s*[<(]",
    },
    DefinitionPattern {
        extensions: &["go"],
        pattern: r"func\s+(\([^)]*\)\s*)?{NAME}\s*[\[<(]",
    },
    DefinitionPattern {
        extensions: &["py"],
        pattern: r"def\s+{NAME}\s*\(",
    },
    DefinitionPattern {
        extensions: &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
        pattern: r"(function\s+{NAME}|const\s+{NAME}\s*=|let\s+{NAME}\s*=|{NAME}\s*\([^)]*\)\s*\{)",
    },
    DefinitionPattern {
        extensions: &["java", "scala"],
        pattern: r"(void|int|String|boolean|public|private|protected|static|def)\s+{NAME}\s*[<(]",
    },
    DefinitionPattern {
        extensions: &["c", "cpp", "cc", "cxx", "h", "hpp"],
        pattern: r"\b\w+[\s*]+{NAME}\s*\(",
    },
    DefinitionPattern {
        extensions: &["zig"],
        pattern: r"(fn|pub fn)\s+{NAME}\s*\(",
    },
    DefinitionPattern {
        extensions: &["sh", "bash"],
        pattern: r"(function\s+{NAME}|{NAME}\s*\(\s*\))",
    },
];

pub fn resolve_same_file(callee: &str, file: &Path, index: &Index) -> Option<Definition> {
    let record = index.get(file)?;
    record
        .definitions
        .iter()
        .find(|d| d.name == callee)
        .cloned()
}

pub fn resolve_by_index(callee: &str, index: &Index) -> Option<Definition> {
    index.definitions().find(|d| d.name == callee).cloned()
}

pub fn resolve_by_search(callee: &str, root: &Path) -> Result<Option<Definition>> {
    use grep::regex::RegexMatcher;
    use grep::searcher::sinks::UTF8;
    use grep::searcher::Searcher;

    let escaped = regex::escape(callee);

    for pattern_def in DEFINITION_PATTERNS {
        let pattern = pattern_def.pattern.replace("{NAME}", &escaped);

        let matcher = match RegexMatcher::new(&pattern) {
            Ok(m) => m,
            Err(_) => continue,
        };

        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if !pattern_def.extensions.contains(&ext) {
                continue;
            }

            let mut found: Option<(u64, PathBuf)> = None;

            let _ = Searcher::new().search_path(
                &matcher,
                path,
                UTF8(|line_num, _line| {
                    found = Some((line_num, path.to_path_buf()));
                    Ok(false)
                }),
            );

            if let Some((line_num, file_path)) = found {
                return Ok(Some(Definition {
                    name: callee.to_string(),
                    kind: DefinitionKind::Function,
                    span: Span {
                        start_byte: 0,
                        end_byte: 0,
                        start_line: line_num as usize,
                        end_line: line_num as usize,
                    },
                    file: file_path,
                }));
            }
        }
    }

    Ok(None)
}

pub struct Resolver<'a> {
    index: &'a Index,
    root: PathBuf,
    discovered_files: RefCell<HashSet<PathBuf>>,
}

impl<'a> Resolver<'a> {
    pub fn new(index: &'a Index, root: PathBuf) -> Self {
        Self {
            index,
            root,
            discovered_files: RefCell::new(HashSet::new()),
        }
    }

    pub fn resolve(&self, callee: &str, from_file: &Path) -> Result<Option<Definition>> {
        if let Some(def) = resolve_same_file(callee, from_file, self.index) {
            return Ok(Some(def));
        }

        if let Some(def) = resolve_by_index(callee, self.index) {
            return Ok(Some(def));
        }

        if let Some(def) = self.resolve_via_imports(callee, from_file) {
            return Ok(Some(def));
        }

        if let Some(def) = resolve_by_search(callee, &self.root)? {
            self.discovered_files.borrow_mut().insert(def.file.clone());
            return Ok(Some(def));
        }

        Ok(None)
    }

    pub fn files_to_index(&self) -> Vec<PathBuf> {
        self.discovered_files.borrow().iter().cloned().collect()
    }

    pub fn clear_discovered(&self) {
        self.discovered_files.borrow_mut().clear();
    }

    fn resolve_via_imports(&self, callee: &str, from_file: &Path) -> Option<Definition> {
        let record = self.index.get(from_file)?;
        let ext = from_file.extension().and_then(|e| e.to_str()).unwrap_or("");

        for import in &record.imports {
            if !self.import_matches_callee(&import.module_path, callee, ext) {
                continue;
            }

            if let Some(resolved) = resolve_relative(&import.module_path, from_file, ext) {
                self.discovered_files.borrow_mut().insert(resolved.clone());
                if let Some(def) = self.find_def_in_file(&resolved, callee) {
                    return Some(def);
                }
            }

            let patterns = normalize_to_patterns(&import.module_path, ext);
            if let Some(resolved) = search_patterns(&patterns, &self.root) {
                self.discovered_files.borrow_mut().insert(resolved.clone());
                if let Some(def) = self.find_def_in_file(&resolved, callee) {
                    return Some(def);
                }
            }
        }

        None
    }

    fn import_matches_callee(&self, module_path: &str, callee: &str, lang: &str) -> bool {
        let clean = module_path.trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>');

        match lang {
            "rs" => {
                let parts: Vec<&str> = clean.split("::").collect();
                parts.last().map(|s| *s == callee).unwrap_or(false)
            }
            "py" => {
                let parts: Vec<&str> = clean.split('.').collect();
                parts.last().map(|s| *s == callee).unwrap_or(false)
            }
            "go" => {
                let parts: Vec<&str> = clean.split('/').collect();
                parts.last().map(|s| *s == callee).unwrap_or(false)
            }
            "java" | "scala" | "sc" => {
                let parts: Vec<&str> = clean.split('.').collect();
                parts
                    .last()
                    .map(|s| *s == callee || *s == "*" || *s == "_")
                    .unwrap_or(false)
            }
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => true,
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => true,
            "zig" => true,
            _ => true,
        }
    }

    fn find_def_in_file(&self, file: &Path, name: &str) -> Option<Definition> {
        let record = self.index.get(file)?;
        record.definitions.iter().find(|d| d.name == name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_normalize_rust_import() {
        let patterns = normalize_rust_import("crate::foo::bar");
        assert!(patterns.iter().any(|p| p.contains("foo/bar.rs")));
        assert!(patterns.iter().any(|p| p.contains("foo/bar/mod.rs")));
    }

    #[test]
    fn test_normalize_python_import() {
        let patterns = normalize_python_import("mypackage.utils.helper");
        assert!(patterns
            .iter()
            .any(|p| p.contains("mypackage/utils/helper.py")));
        assert!(patterns
            .iter()
            .any(|p| p.contains("mypackage/utils/helper/__init__.py")));
    }

    #[test]
    fn test_normalize_go_import() {
        let patterns = normalize_go_import("github.com/user/repo/pkg/utils");
        assert!(patterns.iter().any(|p| p.contains("pkg/utils")));
    }

    #[test]
    fn test_normalize_js_import_relative() {
        let patterns = normalize_js_import("./components/Button");
        assert!(patterns.iter().any(|p| p.contains("components/Button.ts")));
        assert!(patterns
            .iter()
            .any(|p| p.contains("components/Button/index.ts")));
    }

    #[test]
    fn test_normalize_js_import_alias() {
        let patterns = normalize_js_import("@/components/Button");
        assert!(patterns.iter().any(|p| p.contains("components/Button.ts")));
    }

    #[test]
    fn test_normalize_c_import() {
        let patterns = normalize_c_import("utils/helper.h");
        assert!(patterns.iter().any(|p| p.contains("utils/helper.h")));
    }

    #[test]
    fn test_normalize_c_import_system_skipped() {
        let patterns = normalize_c_import("<stdio.h>");
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_normalize_java_import() {
        let patterns = normalize_java_import("com.example.utils.Helper");
        assert!(patterns
            .iter()
            .any(|p| p.contains("com/example/utils/Helper.java")));
    }

    #[test]
    fn test_normalize_java_import_stdlib_skipped() {
        let patterns = normalize_java_import("java.util.List");
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_is_system_header() {
        assert!(is_system_header("<stdio.h>"));
        assert!(is_system_header("<vector>"));
        assert!(is_system_header("<iostream>"));
        assert!(is_system_header("<sys/types.h>"));
        assert!(!is_system_header("\"myheader.h\""));
        assert!(!is_system_header("\"utils/helper.h\""));
    }

    #[test]
    fn test_search_patterns() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("helper.rs"), "fn helper() {}").unwrap();

        let patterns = vec!["**/helper.rs".to_string()];
        let found = search_patterns(&patterns, dir.path());
        assert!(found.is_some());
        assert!(found.unwrap().ends_with("helper.rs"));
    }

    #[test]
    fn test_resolve_same_file() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Span};

        let mut index = Index::new();
        let file = PathBuf::from("src/main.rs");

        index.update(FileRecord {
            path: file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "foo".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 1,
                    end_line: 3,
                },
                file: file.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        let found = resolve_same_file("foo", &file, &index);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo");

        let not_found = resolve_same_file("bar", &file, &index);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_resolve_by_index() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Span};

        let mut index = Index::new();

        index.update(FileRecord {
            path: PathBuf::from("src/a.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "alpha".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 1,
                    end_line: 3,
                },
                file: PathBuf::from("src/a.rs"),
            }],
            calls: vec![],
            imports: vec![],
        });

        let found = resolve_by_index("alpha", &index);
        assert!(found.is_some());

        let not_found = resolve_by_index("gamma", &index);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_resolver_prefers_same_file() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Span};

        let mut index = Index::new();
        let file_a = PathBuf::from("src/a.rs");
        let file_b = PathBuf::from("src/b.rs");

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "foo".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 1,
                    end_line: 3,
                },
                file: file_a.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "foo".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 10,
                    start_line: 10,
                    end_line: 12,
                },
                file: file_b.clone(),
            }],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index, PathBuf::from("."));

        let found = resolver.resolve("foo", &file_a).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_a);

        let found = resolver.resolve("foo", &file_b).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_b);
    }

    #[test]
    fn test_resolve_by_search_rust() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("lib.rs"),
            "pub fn my_function() {\n    println!(\"hello\");\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("my_function", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "my_function");
        assert_eq!(def.span.start_line, 1);
    }

    #[test]
    fn test_resolve_by_search_python() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("utils.py"),
            "def helper_func():\n    pass\n",
        )
        .unwrap();

        let found = resolve_by_search("helper_func", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "helper_func");
    }

    #[test]
    fn test_resolve_via_imports_with_glob() {
        use super::super::index::{Definition, DefinitionKind, FileRecord, Import, Span};

        let dir = TempDir::new().unwrap();
        let utils_dir = dir.path().join("src/utils");
        fs::create_dir_all(&utils_dir).unwrap();
        fs::write(utils_dir.join("helper.rs"), "pub fn helper() {}").unwrap();

        let mut index = Index::new();
        let main_file = dir.path().join("src/main.rs");

        index.update(FileRecord {
            path: utils_dir.join("helper.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![Definition {
                name: "helper".to_string(),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 20,
                    start_line: 1,
                    end_line: 1,
                },
                file: utils_dir.join("helper.rs"),
            }],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: main_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![Import {
                module_path: "crate::utils::helper".to_string(),
                alias: None,
                span: Span {
                    start_byte: 0,
                    end_byte: 25,
                    start_line: 1,
                    end_line: 1,
                },
                file: main_file.clone(),
            }],
        });

        let resolver = Resolver::new(&index, dir.path().to_path_buf());
        let found = resolver.resolve("helper", &main_file).unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "helper");
    }
}

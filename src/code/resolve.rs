use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::index::{Definition, Index};

fn import_to_file_patterns(module_path: &str, lang: &str) -> Vec<String> {
    let clean = module_path.trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>');

    match lang {
        "rs" => {
            let stripped = clean
                .trim_start_matches("crate::")
                .trim_start_matches("self::")
                .trim_start_matches("super::");
            let parts: Vec<&str> = stripped.split("::").filter(|p| !p.is_empty()).collect();
            if parts.is_empty() {
                return vec![];
            }
            let file_path = parts.join("/");
            vec![
                format!("{}.rs", file_path),
                format!("{}/mod.rs", file_path),
                format!("src/{}.rs", file_path),
                format!("src/{}/mod.rs", file_path),
            ]
        }
        "py" => {
            if clean.starts_with('.') {
                return vec![];
            }
            let parts: Vec<&str> = clean.split('.').collect();
            if parts.is_empty() {
                return vec![];
            }
            let file_path = parts.join("/");
            vec![
                format!("{}.py", file_path),
                format!("{}/__init__.py", file_path),
                format!("src/{}.py", file_path),
            ]
        }
        "go" => {
            let parts: Vec<&str> = clean.split('/').collect();
            let local_parts: Vec<&str> = if parts.len() >= 3 && parts[0].contains('.') {
                parts[3..].to_vec()
            } else {
                parts
            };
            if local_parts.is_empty() {
                return vec![];
            }
            let dir_path = local_parts.join("/");
            vec![dir_path]
        }
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
            let base = clean
                .trim_start_matches("./")
                .trim_start_matches("../")
                .trim_start_matches("@/")
                .trim_start_matches('@');
            vec![
                format!("{}.ts", base),
                format!("{}.tsx", base),
                format!("{}.js", base),
                format!("{}/index.ts", base),
                format!("{}/index.tsx", base),
                format!("{}/index.js", base),
            ]
        }
        "java" => {
            let file_path = clean.replace('.', "/");
            vec![
                format!("{}.java", file_path),
                format!("src/{}.java", file_path),
                format!("src/main/java/{}.java", file_path),
            ]
        }
        "scala" | "sc" => {
            let trimmed = clean.trim_end_matches("._").trim_end_matches(".*");
            let file_path = trimmed.replace('.', "/");
            vec![format!("{}.scala", file_path), format!("{}.sc", file_path)]
        }
        "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => {
            vec![
                clean.to_string(),
                format!("include/{}", clean),
                format!("src/{}", clean),
            ]
        }
        "zig" => {
            if clean.ends_with(".zig") || clean.contains('/') {
                vec![clean.to_string(), format!("src/{}", clean)]
            } else {
                vec![format!("{}.zig", clean), format!("src/{}.zig", clean)]
            }
        }
        _ => vec![clean.to_string()],
    }
}

fn extensions_compatible(ext1: &str, ext2: &str) -> bool {
    if ext1 == ext2 {
        return true;
    }

    let family = |ext: &str| -> u8 {
        match ext {
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => 1,
            "c" | "cpp" | "cc" | "cxx" | "h" | "hpp" | "hxx" => 2,
            "scala" | "sc" => 3,
            _ => 0,
        }
    };

    let f1 = family(ext1);
    let f2 = family(ext2);
    f1 != 0 && f1 == f2
}

fn import_matches_callee(module_path: &str, callee: &str, lang: &str) -> bool {
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
        _ => true,
    }
}

struct FilePatternIndex {
    by_filename: HashMap<String, Vec<PathBuf>>,
    by_suffix: HashMap<String, Vec<PathBuf>>,
    by_def_name: HashMap<String, Vec<Definition>>,
}

impl FilePatternIndex {
    fn build(index: &Index) -> Self {
        let mut by_filename: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut by_suffix: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut by_def_name: HashMap<String, Vec<Definition>> = HashMap::new();

        for path in index.files.keys() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                by_filename
                    .entry(filename.to_string())
                    .or_default()
                    .push(path.clone());
            }

            let path_str = path.to_string_lossy();
            let components: Vec<&str> = path_str.split('/').collect();
            for i in 0..components.len() {
                let suffix = components[i..].join("/");
                by_suffix.entry(suffix).or_default().push(path.clone());
            }
        }

        for def in index.definitions() {
            by_def_name
                .entry(def.name.clone())
                .or_default()
                .push(def.clone());
        }

        Self {
            by_filename,
            by_suffix,
            by_def_name,
        }
    }

    fn files_matching(&self, pattern: &str) -> Vec<&PathBuf> {
        if pattern.contains('/') {
            self.by_suffix
                .get(pattern)
                .map(|v| v.iter().collect())
                .unwrap_or_default()
        } else {
            self.by_filename
                .get(pattern)
                .map(|v| v.iter().collect())
                .unwrap_or_default()
        }
    }

    fn definition_by_name(&self, name: &str, from_file: &Path) -> Option<Definition> {
        let defs = self.by_def_name.get(name)?;
        let from_ext = from_file.extension().and_then(|e| e.to_str()).unwrap_or("");

        defs.iter()
            .find(|d| {
                let def_ext = d.file.extension().and_then(|e| e.to_str()).unwrap_or("");
                extensions_compatible(from_ext, def_ext)
            })
            .cloned()
    }
}

pub struct Resolver<'a> {
    index: &'a Index,
    strict: bool,
    pattern_index: FilePatternIndex,
}

impl<'a> Resolver<'a> {
    pub fn new(index: &'a Index) -> Self {
        Self {
            pattern_index: FilePatternIndex::build(index),
            index,
            strict: false,
        }
    }

    pub fn with_strict(index: &'a Index, strict: bool) -> Self {
        Self {
            pattern_index: FilePatternIndex::build(index),
            index,
            strict,
        }
    }

    /// Resolve a callee to its definition.
    ///
    /// Resolution order:
    /// 1. Same file - check if callee is defined in the calling file
    /// 2. Via imports - use import statements to find the defining file
    /// 3. Global fallback (unless strict mode) - search entire index by name
    ///
    /// Note: Global fallback can produce false positives when multiple functions
    /// share the same name (e.g., `parse`). Use strict mode to disable it.
    pub fn resolve(
        &self,
        callee: &str,
        _qualifier: Option<&str>,
        from_file: &Path,
    ) -> Option<Definition> {
        if let Some(def) = self.resolve_same_file(callee, from_file) {
            return Some(def);
        }

        if let Some(def) = self.resolve_via_imports(callee, from_file) {
            return Some(def);
        }

        if !self.strict {
            return self.resolve_by_index(callee, from_file);
        }

        None
    }

    fn resolve_same_file(&self, callee: &str, file: &Path) -> Option<Definition> {
        let record = self.index.get(file)?;
        record
            .definitions
            .iter()
            .find(|d| d.name == callee)
            .cloned()
    }

    fn resolve_by_index(&self, callee: &str, from_file: &Path) -> Option<Definition> {
        self.pattern_index.definition_by_name(callee, from_file)
    }

    fn resolve_via_imports(&self, callee: &str, from_file: &Path) -> Option<Definition> {
        let record = self.index.get(from_file)?;
        let ext = from_file.extension().and_then(|e| e.to_str()).unwrap_or("");

        for import in &record.imports {
            if !import_matches_callee(&import.module_path, callee, ext) {
                continue;
            }

            let patterns = import_to_file_patterns(&import.module_path, ext);

            for pattern in &patterns {
                for indexed_file in self.pattern_index.files_matching(pattern) {
                    if let Some(def) = self.find_def_in_file(indexed_file, callee) {
                        return Some(def);
                    }
                }
            }
        }

        None
    }

    fn find_def_in_file(&self, file: &Path, name: &str) -> Option<Definition> {
        let record = self.index.get(file)?;
        record.definitions.iter().find(|d| d.name == name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::index::{Definition, DefinitionKind, FileRecord, Import, Span};
    use std::path::PathBuf;

    fn make_def(name: &str, file: &str) -> Definition {
        Definition {
            name: name.to_string(),
            kind: DefinitionKind::Function,
            span: Span {
                start_byte: 0,
                end_byte: 10,
                start_line: 1,
                end_line: 3,
            },
            file: PathBuf::from(file),
            signature: None,
        }
    }

    fn make_import(module_path: &str, file: &str) -> Import {
        Import {
            module_path: module_path.to_string(),
            alias: None,
            span: Span {
                start_byte: 0,
                end_byte: 10,
                start_line: 1,
                end_line: 1,
            },
            file: PathBuf::from(file),
        }
    }

    #[test]
    fn test_resolve_same_file() {
        let mut index = Index::new();
        let file = PathBuf::from("src/main.rs");

        index.update(FileRecord {
            path: file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("foo", "src/main.rs")],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index);
        let found = resolver.resolve("foo", None, &file);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "foo");

        let not_found = resolver.resolve("bar", None, &file);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_resolve_prefers_same_file() {
        let mut index = Index::new();
        let file_a = PathBuf::from("src/a.rs");
        let file_b = PathBuf::from("src/b.rs");

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("foo", "src/a.rs")],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("foo", "src/b.rs")],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index);

        let found = resolver.resolve("foo", None, &file_a);
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_a);

        let found = resolver.resolve("foo", None, &file_b);
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_b);
    }

    #[test]
    fn test_resolve_via_imports() {
        let mut index = Index::new();
        let main_file = PathBuf::from("src/main.rs");
        let helper_file = PathBuf::from("src/utils/helper.rs");

        index.update(FileRecord {
            path: helper_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("helper", "src/utils/helper.rs")],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: main_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![make_import("crate::utils::helper", "src/main.rs")],
        });

        let resolver = Resolver::new(&index);
        let found = resolver.resolve("helper", None, &main_file);

        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "helper");
    }

    #[test]
    fn test_resolve_falls_back_to_index() {
        let mut index = Index::new();
        let main_file = PathBuf::from("src/main.rs");

        index.update(FileRecord {
            path: PathBuf::from("src/parse.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("parse", "src/parse.rs")],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: main_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index);

        // Should find via global index lookup
        let found = resolver.resolve("parse", None, &main_file);
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, PathBuf::from("src/parse.rs"));
    }

    #[test]
    fn test_file_pattern_index() {
        let mut index = Index::new();
        index.update(FileRecord {
            path: PathBuf::from("src/utils/helper.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });
        index.update(FileRecord {
            path: PathBuf::from("src/other.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });

        let pattern_index = FilePatternIndex::build(&index);

        let matches = pattern_index.files_matching("utils/helper.rs");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], &PathBuf::from("src/utils/helper.rs"));

        let matches = pattern_index.files_matching("helper.rs");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], &PathBuf::from("src/utils/helper.rs"));

        let matches = pattern_index.files_matching("other.rs");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], &PathBuf::from("src/other.rs"));

        let matches = pattern_index.files_matching("nonexistent.rs");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_import_to_file_patterns_rust() {
        let patterns = import_to_file_patterns("crate::utils::helper", "rs");
        assert!(patterns.iter().any(|p| p.contains("utils/helper.rs")));
        assert!(patterns.iter().any(|p| p.contains("utils/helper/mod.rs")));
    }

    #[test]
    fn test_import_to_file_patterns_python() {
        let patterns = import_to_file_patterns("mypackage.utils.helper", "py");
        assert!(patterns
            .iter()
            .any(|p| p.contains("mypackage/utils/helper.py")));
    }

    #[test]
    fn test_import_to_file_patterns_js() {
        let patterns = import_to_file_patterns("./components/Button", "ts");
        assert!(patterns.iter().any(|p| p.contains("components/Button.ts")));
        assert!(patterns
            .iter()
            .any(|p| p.contains("components/Button/index.ts")));
    }

    #[test]
    fn test_resolve_ignores_cross_language_definitions() {
        let mut index = Index::new();
        let nix_file = PathBuf::from("config.nix");
        let cpp_file = PathBuf::from("src/filter.cpp");

        index.update(FileRecord {
            path: cpp_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("filter", "src/filter.cpp")],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: nix_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index);

        // Should NOT find cpp definition when resolving from nix file
        let found = resolver.resolve("filter", None, &nix_file);
        assert!(found.is_none());
    }

    #[test]
    fn test_resolve_allows_same_language_family() {
        let mut index = Index::new();
        let ts_file = PathBuf::from("src/app.ts");
        let tsx_file = PathBuf::from("src/component.tsx");

        index.update(FileRecord {
            path: tsx_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("Button", "src/component.tsx")],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: ts_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index);

        // Should find tsx definition when resolving from ts file (same family)
        let found = resolver.resolve("Button", None, &ts_file);
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, tsx_file);
    }

    #[test]
    fn test_extensions_compatible() {
        // Same extension
        assert!(extensions_compatible("rs", "rs"));
        assert!(extensions_compatible("py", "py"));

        // Same family
        assert!(extensions_compatible("ts", "tsx"));
        assert!(extensions_compatible("js", "jsx"));
        assert!(extensions_compatible("c", "h"));
        assert!(extensions_compatible("cpp", "hpp"));

        // Different languages
        assert!(!extensions_compatible("rs", "py"));
        assert!(!extensions_compatible("nix", "cpp"));
        assert!(!extensions_compatible("go", "java"));
    }
}

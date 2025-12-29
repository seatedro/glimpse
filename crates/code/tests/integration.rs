use std::collections::HashSet;
use std::fs;
use std::path::Path;

use glimpse_code::extract::Extractor;
use glimpse_code::graph::CallGraph;
use glimpse_code::index::{file_fingerprint, FileRecord, Index};
use glimpse_code::resolve::{resolve_by_index, resolve_by_search, resolve_same_file, Resolver};
use tree_sitter::Parser;

fn index_file(index: &mut Index, extractor: &Extractor, path: &Path, source: &str) {
    let mut parser = Parser::new();
    parser.set_language(extractor.language()).unwrap();
    let tree = parser.parse(source, None).unwrap();

    let (mtime, size) = file_fingerprint(path).unwrap_or((0, source.len() as u64));

    let record = FileRecord {
        path: path.to_path_buf(),
        mtime,
        size,
        definitions: extractor.extract_definitions(&tree, source.as_bytes(), path),
        calls: extractor.extract_calls(&tree, source.as_bytes(), path),
        imports: extractor.extract_imports(&tree, source.as_bytes(), path),
    };

    index.update(record);
}

mod resolver_tests {
    use super::*;
    use glimpse_code::index::{Call, Definition, DefinitionKind, FileRecord, Import, Span};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_span() -> Span {
        Span {
            start_byte: 0,
            end_byte: 10,
            start_line: 1,
            end_line: 1,
        }
    }

    fn make_def(name: &str, file: &Path) -> Definition {
        Definition {
            name: name.to_string(),
            kind: DefinitionKind::Function,
            span: make_span(),
            file: file.to_path_buf(),
        }
    }

    #[test]
    fn test_resolve_same_file_priority() {
        let mut index = Index::new();
        let file_a = PathBuf::from("src/a.rs");
        let file_b = PathBuf::from("src/b.rs");

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("helper", &file_a)],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("helper", &file_b)],
            calls: vec![],
            imports: vec![],
        });

        let from_a = resolve_same_file("helper", &file_a, &index);
        assert!(from_a.is_some());
        assert_eq!(from_a.unwrap().file, file_a);

        let from_b = resolve_same_file("helper", &file_b, &index);
        assert!(from_b.is_some());
        assert_eq!(from_b.unwrap().file, file_b);

        let not_found = resolve_same_file("nonexistent", &file_a, &index);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_resolve_by_index_cross_file() {
        let mut index = Index::new();
        let file_a = PathBuf::from("src/a.rs");
        let file_b = PathBuf::from("src/b.rs");

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("func_a", &file_a)],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("func_b", &file_b)],
            calls: vec![],
            imports: vec![],
        });

        let found_a = resolve_by_index("func_a", &index);
        assert!(found_a.is_some());
        assert_eq!(found_a.unwrap().file, file_a);

        let found_b = resolve_by_index("func_b", &index);
        assert!(found_b.is_some());
        assert_eq!(found_b.unwrap().file, file_b);
    }

    #[test]
    fn test_resolve_by_search_rust() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("lib.rs"),
            "pub fn my_searched_function() {\n    println!(\"found\");\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("my_searched_function", dir.path()).unwrap();
        assert!(found.is_some());
        let def = found.unwrap();
        assert_eq!(def.name, "my_searched_function");
        assert!(def.file.ends_with("lib.rs"));
    }

    #[test]
    fn test_resolve_by_search_python() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("utils.py"),
            "def searched_python_func():\n    pass\n",
        )
        .unwrap();

        let found = resolve_by_search("searched_python_func", dir.path()).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "searched_python_func");
    }

    #[test]
    fn test_resolve_by_search_go() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("main.go"),
            "package main\n\nfunc searchedGoFunc() {\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("searchedGoFunc", dir.path()).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "searchedGoFunc");
    }

    #[test]
    fn test_resolve_by_search_typescript() {
        let dir = TempDir::new().unwrap();

        fs::write(
            dir.path().join("index.ts"),
            "function searchedTsFunc() {\n    return 42;\n}\n",
        )
        .unwrap();

        let found = resolve_by_search("searchedTsFunc", dir.path()).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "searchedTsFunc");
    }

    #[test]
    fn test_resolve_by_search_not_found() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join("empty.rs"), "// no functions here\n").unwrap();

        let found = resolve_by_search("nonexistent_function", dir.path()).unwrap();
        assert!(found.is_none());
    }

    #[test]
    fn test_resolver_resolution_chain() {
        let dir = TempDir::new().unwrap();
        let mut index = Index::new();

        let file_main = dir.path().join("main.rs");
        let file_utils = dir.path().join("utils.rs");

        fs::write(&file_main, "fn main() { helper(); }").unwrap();
        fs::write(&file_utils, "pub fn helper() { nested(); }\npub fn nested() {}").unwrap();

        index.update(FileRecord {
            path: file_main.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("main", &file_main)],
            calls: vec![Call {
                callee: "helper".to_string(),
                caller: Some("main".to_string()),
                span: make_span(),
                file: file_main.clone(),
            }],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_utils.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("helper", &file_utils), make_def("nested", &file_utils)],
            calls: vec![],
            imports: vec![],
        });

        let resolver = Resolver::new(&index, dir.path().to_path_buf());

        let found = resolver.resolve("helper", &file_main).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().file, file_utils);

        let same_file = resolver.resolve("nested", &file_utils).unwrap();
        assert!(same_file.is_some());
        assert_eq!(same_file.unwrap().file, file_utils);
    }

    #[test]
    fn test_resolver_grep_fallback() {
        let dir = TempDir::new().unwrap();
        let index = Index::new();

        fs::write(
            dir.path().join("hidden.rs"),
            "fn not_indexed_function() {\n    println!(\"hidden\");\n}\n",
        )
        .unwrap();

        let resolver = Resolver::new(&index, dir.path().to_path_buf());
        let from_file = dir.path().join("caller.rs");

        let found = resolver.resolve("not_indexed_function", &from_file).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "not_indexed_function");

        let discovered = resolver.files_to_index();
        assert!(!discovered.is_empty());
    }

    #[test]
    fn test_resolver_tracks_discovered_files() {
        let dir = TempDir::new().unwrap();
        let index = Index::new();

        fs::write(dir.path().join("a.rs"), "fn discovered_a() {}").unwrap();
        fs::write(dir.path().join("b.rs"), "fn discovered_b() {}").unwrap();

        let resolver = Resolver::new(&index, dir.path().to_path_buf());
        let from_file = dir.path().join("main.rs");

        resolver.resolve("discovered_a", &from_file).unwrap();
        resolver.resolve("discovered_b", &from_file).unwrap();

        let discovered = resolver.files_to_index();
        assert_eq!(discovered.len(), 2);

        resolver.clear_discovered();
        assert!(resolver.files_to_index().is_empty());
    }

    #[test]
    fn test_resolver_with_imports() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();

        fs::write(src.join("utils.rs"), "pub fn imported_helper() {}").unwrap();

        let mut index = Index::new();
        let main_file = dir.path().join("src/main.rs");

        index.update(FileRecord {
            path: src.join("utils.rs"),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("imported_helper", &src.join("utils.rs"))],
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
                module_path: "crate::utils::imported_helper".to_string(),
                alias: None,
                span: make_span(),
                file: main_file.clone(),
            }],
        });

        let resolver = Resolver::new(&index, dir.path().to_path_buf());

        let found = resolver.resolve("imported_helper", &main_file).unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "imported_helper");
    }

    #[test]
    fn test_import_discovery_tracks_files_for_reindexing() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        let utils_dir = src.join("utils");
        fs::create_dir_all(&utils_dir).unwrap();

        fs::write(utils_dir.join("helper.rs"), "pub fn helper() {}").unwrap();

        let mut index = Index::new();
        let main_file = src.join("main.rs");

        index.update(FileRecord {
            path: main_file.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("main", &main_file)],
            calls: vec![],
            imports: vec![Import {
                module_path: "crate::utils::helper".to_string(),
                alias: None,
                span: make_span(),
                file: main_file.clone(),
            }],
        });

        let resolver = Resolver::new(&index, dir.path().to_path_buf());

        let found = resolver.resolve("helper", &main_file).unwrap();
        assert!(found.is_some(), "grep fallback should find unindexed definition");
        assert_eq!(found.unwrap().name, "helper");

        let discovered = resolver.files_to_index();
        assert!(
            discovered.iter().any(|p| p.ends_with("helper.rs")),
            "should track helper.rs for re-indexing"
        );
    }
}

mod call_graph_resolution {
    use super::*;
    use glimpse_code::index::{Call, Definition, DefinitionKind, FileRecord, Span};
    use tempfile::TempDir;

    fn make_span() -> Span {
        Span {
            start_byte: 0,
            end_byte: 10,
            start_line: 1,
            end_line: 1,
        }
    }

    fn make_def(name: &str, file: &Path) -> Definition {
        Definition {
            name: name.to_string(),
            kind: DefinitionKind::Function,
            span: make_span(),
            file: file.to_path_buf(),
        }
    }

    #[test]
    fn test_graph_resolves_cross_file_calls() {
        let dir = TempDir::new().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");

        fs::write(&file_a, "fn caller() { callee(); }").unwrap();
        fs::write(&file_b, "pub fn callee() {}").unwrap();

        let mut index = Index::new();

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("caller", &file_a)],
            calls: vec![Call {
                callee: "callee".to_string(),
                caller: Some("caller".to_string()),
                span: make_span(),
                file: file_a.clone(),
            }],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("callee", &file_b)],
            calls: vec![],
            imports: vec![],
        });

        let graph = CallGraph::build(&index, dir.path());

        let caller_id = graph.find_node("caller").unwrap();
        let callees = graph.get_callees(caller_id);

        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].definition.name, "callee");
        assert_eq!(callees[0].definition.file, file_b);
    }

    #[test]
    fn test_graph_uses_grep_fallback_for_unindexed() {
        let dir = TempDir::new().unwrap();
        let file_caller = dir.path().join("caller.rs");
        let file_hidden = dir.path().join("hidden.rs");

        fs::write(&file_caller, "fn caller() { hidden_func(); }").unwrap();
        fs::write(&file_hidden, "fn hidden_func() {}").unwrap();

        let mut index = Index::new();

        index.update(FileRecord {
            path: file_caller.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("caller", &file_caller)],
            calls: vec![Call {
                callee: "hidden_func".to_string(),
                caller: Some("caller".to_string()),
                span: make_span(),
                file: file_caller.clone(),
            }],
            imports: vec![],
        });

        let graph = CallGraph::build(&index, dir.path());

        let caller_id = graph.find_node("caller").unwrap();
        let callees = graph.get_callees(caller_id);

        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].definition.name, "hidden_func");
    }

    #[test]
    fn test_graph_same_name_different_files() {
        let dir = TempDir::new().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        let file_main = dir.path().join("main.rs");

        fs::write(&file_a, "fn helper() {}").unwrap();
        fs::write(&file_b, "fn helper() {}").unwrap();
        fs::write(&file_main, "fn main() { helper(); }").unwrap();

        let mut index = Index::new();

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("helper", &file_a)],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("helper", &file_b)],
            calls: vec![],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_main.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("main", &file_main)],
            calls: vec![Call {
                callee: "helper".to_string(),
                caller: Some("main".to_string()),
                span: make_span(),
                file: file_main.clone(),
            }],
            imports: vec![],
        });

        let graph = CallGraph::build(&index, dir.path());

        assert_eq!(graph.node_count(), 3);

        let a_id = graph.find_node_by_file_and_name(&file_a, "helper");
        let b_id = graph.find_node_by_file_and_name(&file_b, "helper");
        assert!(a_id.is_some());
        assert!(b_id.is_some());
        assert_ne!(a_id, b_id);
    }

    #[test]
    fn test_graph_transitive_through_resolution() {
        let dir = TempDir::new().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        let file_c = dir.path().join("c.rs");

        fs::write(&file_a, "fn entry() { middle(); }").unwrap();
        fs::write(&file_b, "fn middle() { leaf(); }").unwrap();
        fs::write(&file_c, "fn leaf() {}").unwrap();

        let mut index = Index::new();

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("entry", &file_a)],
            calls: vec![Call {
                callee: "middle".to_string(),
                caller: Some("entry".to_string()),
                span: make_span(),
                file: file_a.clone(),
            }],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_b.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("middle", &file_b)],
            calls: vec![Call {
                callee: "leaf".to_string(),
                caller: Some("middle".to_string()),
                span: make_span(),
                file: file_b.clone(),
            }],
            imports: vec![],
        });

        index.update(FileRecord {
            path: file_c.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("leaf", &file_c)],
            calls: vec![],
            imports: vec![],
        });

        let graph = CallGraph::build(&index, dir.path());

        let entry_id = graph.find_node("entry").unwrap();
        let transitive = graph.get_transitive_callees(entry_id);

        assert_eq!(transitive.len(), 2);

        let names: HashSet<_> = transitive.iter().map(|n| n.definition.name.as_str()).collect();
        assert!(names.contains("middle"));
        assert!(names.contains("leaf"));

        let order = graph.post_order_definitions(entry_id);
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].name, "leaf");
        assert_eq!(order[1].name, "middle");
        assert_eq!(order[2].name, "entry");
    }

    #[test]
    fn test_graph_unresolved_calls_ignored() {
        let dir = TempDir::new().unwrap();
        let file_a = dir.path().join("a.rs");

        fs::write(&file_a, "fn caller() { nonexistent(); }").unwrap();

        let mut index = Index::new();

        index.update(FileRecord {
            path: file_a.clone(),
            mtime: 0,
            size: 0,
            definitions: vec![make_def("caller", &file_a)],
            calls: vec![Call {
                callee: "nonexistent".to_string(),
                caller: Some("caller".to_string()),
                span: make_span(),
                file: file_a.clone(),
            }],
            imports: vec![],
        });

        let graph = CallGraph::build(&index, dir.path());

        let caller_id = graph.find_node("caller").unwrap();
        let callees = graph.get_callees(caller_id);

        assert!(callees.is_empty());
    }
}

mod language_extraction {
    use super::*;
    use tempfile::TempDir;

    #[test]
    #[ignore]
    fn test_rust_full_pipeline() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();

        let main_rs = r#"
mod utils;

fn main() {
    let config = load_config();
    utils::process(config);
}

fn load_config() -> Config {
    Config::default()
}

struct Config {
    data: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { data: String::new() }
    }
}
"#;

        let utils_rs = r#"
use crate::Config;

pub fn process(cfg: Config) {
    validate(&cfg);
    save(&cfg);
}

fn validate(cfg: &Config) {
    check_data(cfg);
}

fn check_data(_cfg: &Config) {}

fn save(cfg: &Config) {
    write_file(&cfg.data);
}

fn write_file(_data: &str) {}
"#;

        fs::write(src.join("main.rs"), main_rs).unwrap();
        fs::write(src.join("utils.rs"), utils_rs).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("rust").unwrap();

        index_file(&mut index, &extractor, &src.join("main.rs"), main_rs);
        index_file(&mut index, &extractor, &src.join("utils.rs"), utils_rs);

        let graph = CallGraph::build(&index, dir.path());

        assert!(graph.node_count() >= 5);

        if let Some(process_id) = graph.find_node("process") {
            let callees = graph.get_callees(process_id);
            let names: HashSet<_> = callees.iter().map(|n| n.definition.name.as_str()).collect();
            assert!(names.contains("validate") || names.contains("save"));
        }
    }

    #[test]
    #[ignore]
    fn test_python_full_pipeline() {
        let dir = TempDir::new().unwrap();

        let main_py = r#"
from utils import helper

def main():
    data = load()
    result = process(data)
    helper(result)

def load():
    return read_file()

def read_file():
    return "data"

def process(data):
    return transform(data)

def transform(x):
    return x.upper()

if __name__ == "__main__":
    main()
"#;

        let utils_py = r#"
def helper(data):
    print(data)
    format_output(data)

def format_output(s):
    return s.strip()
"#;

        fs::write(dir.path().join("main.py"), main_py).unwrap();
        fs::write(dir.path().join("utils.py"), utils_py).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("python").unwrap();

        index_file(&mut index, &extractor, &dir.path().join("main.py"), main_py);
        index_file(&mut index, &extractor, &dir.path().join("utils.py"), utils_py);

        let graph = CallGraph::build(&index, dir.path());

        if let Some(main_id) = graph.find_node("main") {
            let transitive = graph.get_transitive_callees(main_id);
            assert!(!transitive.is_empty());
        }
    }

    #[test]
    #[ignore]
    fn test_typescript_full_pipeline() {
        let dir = TempDir::new().unwrap();

        let main_ts = r#"
import { helper } from './utils';

function main() {
    const result = processData();
    helper(result);
}

function processData(): string {
    return transform("input");
}

function transform(input: string): string {
    return input.toUpperCase();
}

main();
"#;

        let utils_ts = r#"
export function helper(data: string) {
    console.log(data);
    format(data);
}

function format(s: string): string {
    return s.trim();
}
"#;

        fs::write(dir.path().join("main.ts"), main_ts).unwrap();
        fs::write(dir.path().join("utils.ts"), utils_ts).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("typescript").unwrap();

        index_file(&mut index, &extractor, &dir.path().join("main.ts"), main_ts);
        index_file(&mut index, &extractor, &dir.path().join("utils.ts"), utils_ts);

        let graph = CallGraph::build(&index, dir.path());

        if let Some(main_id) = graph.find_node("main") {
            let callees = graph.get_callees(main_id);
            assert!(!callees.is_empty());
        }
    }

    #[test]
    #[ignore]
    fn test_go_full_pipeline() {
        let dir = TempDir::new().unwrap();

        let main_go = r#"
package main

func main() {
    config := loadConfig()
    process(config)
}

func loadConfig() *Config {
    return &Config{}
}

func process(cfg *Config) {
    validate(cfg)
    save(cfg)
}

func validate(cfg *Config) {}

func save(cfg *Config) {}

type Config struct {
    Name string
}
"#;

        fs::write(dir.path().join("main.go"), main_go).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("go").unwrap();

        index_file(&mut index, &extractor, &dir.path().join("main.go"), main_go);

        let graph = CallGraph::build(&index, dir.path());

        if let Some(main_id) = graph.find_node("main") {
            let transitive = graph.get_transitive_callees(main_id);
            assert!(transitive.len() >= 2);
        }
    }
}

mod index_persistence {
    use super::*;
    use glimpse_code::index::{clear_index, load_index, save_index};
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_preserves_data() {
        let dir = TempDir::new().unwrap();

        let mut index = Index::new();
        index.update(FileRecord {
            path: dir.path().join("test.rs"),
            mtime: 12345,
            size: 100,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });

        save_index(&index, dir.path()).unwrap();

        let loaded = load_index(dir.path()).unwrap().unwrap();
        assert_eq!(loaded.files.len(), 1);
        assert!(loaded.get(&dir.path().join("test.rs")).is_some());

        clear_index(dir.path()).unwrap();
        assert!(load_index(dir.path()).unwrap().is_none());
    }

    #[test]
    fn test_index_staleness_detection() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");

        fs::write(&file, "fn test() {}").unwrap();

        let (mtime, size) = file_fingerprint(&file).unwrap();

        let mut index = Index::new();
        index.update(FileRecord {
            path: file.clone(),
            mtime,
            size,
            definitions: vec![],
            calls: vec![],
            imports: vec![],
        });

        assert!(!index.is_stale(&file, mtime, size));
        assert!(index.is_stale(&file, mtime + 1, size));
        assert!(index.is_stale(&file, mtime, size + 1));
        assert!(index.is_stale(&dir.path().join("other.rs"), mtime, size));
    }
}

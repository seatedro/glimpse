use std::fs;
use std::path::{Path, PathBuf};

use glimpse::code::extract::Extractor;
use glimpse::code::index::{file_fingerprint, Call, FileRecord, Index};
use glimpse::code::lsp::AsyncLspResolver;
use tempfile::TempDir;
use tree_sitter::Parser;

fn index_file(index: &mut Index, extractor: &Extractor, base: &Path, path: &Path, source: &str) {
    let mut parser = Parser::new();
    parser.set_language(extractor.language()).unwrap();
    let tree = parser.parse(source, None).unwrap();

    let rel_path = path
        .strip_prefix(base)
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            path.file_name()
                .map(PathBuf::from)
                .unwrap_or(path.to_path_buf())
        });
    let (mtime, size) = file_fingerprint(path).unwrap_or((0, source.len() as u64));

    let record = FileRecord {
        path: rel_path.clone(),
        mtime,
        size,
        definitions: extractor.extract_definitions(&tree, source.as_bytes(), &rel_path),
        calls: extractor.extract_calls(&tree, source.as_bytes(), &rel_path),
        imports: extractor.extract_imports(&tree, source.as_bytes(), &rel_path),
    };

    index.update(record);
}

fn lsp_available(binary: &str) -> bool {
    which::which(binary).is_ok() || {
        let lsp_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("glimpse")
            .join("lsp");
        lsp_dir.join(binary).exists()
    }
}

fn collect_calls(index: &Index) -> Vec<Call> {
    index.calls().cloned().collect()
}

async fn resolve_call(
    resolver: &mut AsyncLspResolver,
    call: &Call,
    index: &Index,
) -> Option<String> {
    let calls: Vec<&Call> = vec![call];
    let results = resolver
        .resolve_calls_batch(&calls, index, 1, true, |_, _, _: &str| {})
        .await;
    results
        .first()
        .map(|(_, resolved)| resolved.target_name.clone())
}

mod rust_lsp {
    use super::*;

    fn rust_analyzer_available() -> bool {
        lsp_available("rust-analyzer")
    }

    #[tokio::test]
    #[ignore]
    async fn test_rust_same_file_definition() {
        if !rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not available");
            return;
        }

        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();

        let main_rs = r#"fn main() {
    helper();
}

fn helper() {
    println!("hello");
}
"#;

        let cargo_toml = r#"[package]
name = "test_project"
version = "0.1.0"
edition = "2021"
"#;

        fs::write(src.join("main.rs"), main_rs).unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("rust").unwrap();

        let main_rs_path = src.join("main.rs");
        let rel_path = main_rs_path.strip_prefix(dir.path()).unwrap();

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(extractor.language()).unwrap();
        let tree = parser.parse(main_rs, None).unwrap();

        let record = FileRecord {
            path: rel_path.to_path_buf(),
            mtime: 0,
            size: main_rs.len() as u64,
            definitions: extractor.extract_definitions(&tree, main_rs.as_bytes(), rel_path),
            calls: extractor.extract_calls(&tree, main_rs.as_bytes(), rel_path),
            imports: extractor.extract_imports(&tree, main_rs.as_bytes(), rel_path),
        };

        eprintln!("Index record path: {:?}", record.path);
        eprintln!(
            "Definitions: {:?}",
            record
                .definitions
                .iter()
                .map(|d| (&d.name, &d.file))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "Calls: {:?}",
            record
                .calls
                .iter()
                .map(|c| (&c.callee, &c.file, c.span.start_line))
                .collect::<Vec<_>>()
        );

        index.update(record);

        let calls = collect_calls(&index);
        assert!(!calls.is_empty(), "Should extract calls from Rust code");

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            eprintln!(
                "Resolving call: callee={}, file={:?}, line={}",
                call.callee, call.file, call.span.start_line
            );
            let def_name = resolve_call(&mut resolver, call, &index).await;
            if def_name.is_none() {
                eprintln!("Resolution failed! Check LSP logs.");
            }
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_rust_cross_module_definition() {
        if !rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not available");
            return;
        }

        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(&src).unwrap();

        let main_rs = r#"mod utils;

fn main() {
    utils::process();
}
"#;

        let utils_rs = r#"pub fn process() {
    println!("processing");
}
"#;

        let cargo_toml = r#"[package]
name = "test_project"
version = "0.1.0"
edition = "2021"
"#;

        fs::write(src.join("main.rs"), main_rs).unwrap();
        fs::write(src.join("utils.rs"), utils_rs).unwrap();
        fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("rust").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &src.join("main.rs"),
            main_rs,
        );
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &src.join("utils.rs"),
            utils_rs,
        );

        let calls = collect_calls(&index);
        eprintln!(
            "calls: {:?}",
            calls.iter().map(|c| &c.callee).collect::<Vec<_>>()
        );
        let process_call = calls.iter().find(|c| c.callee == "process");
        assert!(process_call.is_some(), "Should find call to process()");

        let mut resolver = AsyncLspResolver::new(dir.path());
        eprintln!("created resolver for {:?}", dir.path());

        if let Some(call) = process_call {
            eprintln!(
                "resolving call: {:?} at {:?}:{}",
                call.callee, call.file, call.span.start_line
            );
            let def_name = resolve_call(&mut resolver, call, &index).await;
            eprintln!("def_name: {:?}", def_name);
            eprintln!("stats: {:?}", resolver.stats());
            assert!(def_name.is_some(), "LSP should resolve process() call");
            assert_eq!(def_name.unwrap(), "process");
        }

        resolver.shutdown_all().await;
    }
}

mod go_lsp {
    use super::*;

    fn gopls_available() -> bool {
        lsp_available("gopls")
    }

    #[tokio::test]
    #[ignore]
    async fn test_go_same_file_definition() {
        if !gopls_available() {
            eprintln!("Skipping: gopls not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_go = r#"package main

func main() {
	helper()
}

func helper() {
	println("hello")
}
"#;

        let go_mod = "module test_project\n\ngo 1.21\n";

        fs::write(dir.path().join("main.go"), main_go).unwrap();
        fs::write(dir.path().join("go.mod"), go_mod).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("go").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.go"),
            main_go,
        );

        let calls = collect_calls(&index);
        assert!(!calls.is_empty(), "Should extract calls from Go code");

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_go_cross_package_definition() {
        if !gopls_available() {
            eprintln!("Skipping: gopls not available");
            return;
        }

        let dir = TempDir::new().unwrap();
        let utils_dir = dir.path().join("utils");
        fs::create_dir_all(&utils_dir).unwrap();

        let main_go = r#"package main

import "test_project/utils"

func main() {
	utils.Process()
}
"#;

        let utils_go = r#"package utils

func Process() {
	println("processing")
}
"#;

        let go_mod = "module test_project\n\ngo 1.21\n";

        fs::write(dir.path().join("main.go"), main_go).unwrap();
        fs::write(utils_dir.join("utils.go"), utils_go).unwrap();
        fs::write(dir.path().join("go.mod"), go_mod).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("go").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.go"),
            main_go,
        );
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &utils_dir.join("utils.go"),
            utils_go,
        );

        let calls = collect_calls(&index);
        let process_call = calls.iter().find(|c| c.callee == "Process");
        assert!(process_call.is_some(), "Should find call to Process()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = process_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(
                def_name.is_some(),
                "LSP should resolve utils.Process() call"
            );
            assert_eq!(def_name.unwrap(), "Process");
        }

        resolver.shutdown_all().await;
    }
}

mod python_lsp {
    use super::*;

    fn pyright_available() -> bool {
        lsp_available("pyright-langserver") || lsp_available("pyright")
    }

    #[tokio::test]
    #[ignore]
    async fn test_python_same_file_definition() {
        if !pyright_available() {
            eprintln!("Skipping: pyright not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_py = r#"def main():
    helper()

def helper():
    print("hello")

if __name__ == "__main__":
    main()
"#;

        fs::write(dir.path().join("main.py"), main_py).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("python").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.py"),
            main_py,
        );

        let calls = collect_calls(&index);
        assert!(!calls.is_empty(), "Should extract calls from Python code");

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_python_cross_module_definition() {
        if !pyright_available() {
            eprintln!("Skipping: pyright not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_py = r#"from utils import process

def main():
    process()

if __name__ == "__main__":
    main()
"#;

        let utils_py = r#"def process():
    print("processing")
"#;

        fs::write(dir.path().join("main.py"), main_py).unwrap();
        fs::write(dir.path().join("utils.py"), utils_py).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("python").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.py"),
            main_py,
        );
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("utils.py"),
            utils_py,
        );

        let calls = collect_calls(&index);
        let process_call = calls.iter().find(|c| c.callee == "process");
        assert!(process_call.is_some(), "Should find call to process()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = process_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve process() call");
            assert_eq!(def_name.unwrap(), "process");
        }

        resolver.shutdown_all().await;
    }
}

mod typescript_lsp {
    use super::*;

    fn tsserver_available() -> bool {
        lsp_available("typescript-language-server") || lsp_available("tsserver")
    }

    #[tokio::test]
    #[ignore]
    async fn test_typescript_same_file_definition() {
        if !tsserver_available() {
            eprintln!("Skipping: typescript-language-server not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_ts = r#"function main() {
    helper();
}

function helper() {
    console.log("hello");
}

main();
"#;

        let tsconfig = r#"{
    "compilerOptions": {
        "target": "ES2020",
        "module": "commonjs",
        "strict": true
    }
}
"#;

        fs::write(dir.path().join("main.ts"), main_ts).unwrap();
        fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("typescript").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.ts"),
            main_ts,
        );

        let calls = collect_calls(&index);
        assert!(
            !calls.is_empty(),
            "Should extract calls from TypeScript code"
        );

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_typescript_cross_module_definition() {
        if !tsserver_available() {
            eprintln!("Skipping: typescript-language-server not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_ts = r#"import { process } from "./utils";

function main() {
    process();
}

main();
"#;

        let utils_ts = r#"export function process() {
    console.log("processing");
}
"#;

        let tsconfig = r#"{
    "compilerOptions": {
        "target": "ES2020",
        "module": "commonjs",
        "strict": true
    }
}
"#;

        fs::write(dir.path().join("main.ts"), main_ts).unwrap();
        fs::write(dir.path().join("utils.ts"), utils_ts).unwrap();
        fs::write(dir.path().join("tsconfig.json"), tsconfig).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("typescript").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.ts"),
            main_ts,
        );
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("utils.ts"),
            utils_ts,
        );

        let calls = collect_calls(&index);
        let process_call = calls.iter().find(|c| c.callee == "process");
        assert!(process_call.is_some(), "Should find call to process()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = process_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve process() call");
            assert_eq!(def_name.unwrap(), "process");
        }

        resolver.shutdown_all().await;
    }
}

mod javascript_lsp {
    use super::*;

    fn tsserver_available() -> bool {
        lsp_available("typescript-language-server") || lsp_available("tsserver")
    }

    #[tokio::test]
    #[ignore]
    async fn test_javascript_same_file_definition() {
        if !tsserver_available() {
            eprintln!("Skipping: typescript-language-server not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_js = r#"function main() {
    helper();
}

function helper() {
    console.log("hello");
}

main();
"#;

        let jsconfig = r#"{
    "compilerOptions": {
        "target": "ES2020",
        "module": "commonjs"
    }
}
"#;

        fs::write(dir.path().join("main.js"), main_js).unwrap();
        fs::write(dir.path().join("jsconfig.json"), jsconfig).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("javascript").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.js"),
            main_js,
        );

        let calls = collect_calls(&index);
        assert!(
            !calls.is_empty(),
            "Should extract calls from JavaScript code"
        );

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }
}

mod c_lsp {
    use super::*;

    fn clangd_available() -> bool {
        lsp_available("clangd")
    }

    #[tokio::test]
    #[ignore]
    async fn test_c_same_file_definition() {
        if !clangd_available() {
            eprintln!("Skipping: clangd not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_c = r#"#include <stdio.h>

void helper(void);

int main(void) {
    helper();
    return 0;
}

void helper(void) {
    printf("hello\n");
}
"#;

        let compile_commands = r#"[
    {
        "directory": ".",
        "command": "cc -c main.c",
        "file": "main.c"
    }
]
"#;

        fs::write(dir.path().join("main.c"), main_c).unwrap();
        fs::write(dir.path().join("compile_commands.json"), compile_commands).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("c").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.c"),
            main_c,
        );

        let calls = collect_calls(&index);
        assert!(!calls.is_empty(), "Should extract calls from C code");

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_c_cross_file_definition() {
        if !clangd_available() {
            eprintln!("Skipping: clangd not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_c = r#"#include "utils.h"

int main(void) {
    process();
    return 0;
}
"#;

        let utils_h = r#"#ifndef UTILS_H
#define UTILS_H

void process(void);

#endif
"#;

        let utils_c = r#"#include "utils.h"
#include <stdio.h>

void process(void) {
    printf("processing\n");
}
"#;

        let compile_commands = r#"[
    {
        "directory": ".",
        "command": "cc -c main.c",
        "file": "main.c"
    },
    {
        "directory": ".",
        "command": "cc -c utils.c",
        "file": "utils.c"
    }
]
"#;

        fs::write(dir.path().join("main.c"), main_c).unwrap();
        fs::write(dir.path().join("utils.h"), utils_h).unwrap();
        fs::write(dir.path().join("utils.c"), utils_c).unwrap();
        fs::write(dir.path().join("compile_commands.json"), compile_commands).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("c").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.c"),
            main_c,
        );
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("utils.c"),
            utils_c,
        );

        let calls = collect_calls(&index);
        let process_call = calls.iter().find(|c| c.callee == "process");
        assert!(process_call.is_some(), "Should find call to process()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = process_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve process() call");
            assert_eq!(def_name.unwrap(), "process");
        }

        resolver.shutdown_all().await;
    }
}

mod cpp_lsp {
    use super::*;

    fn clangd_available() -> bool {
        lsp_available("clangd")
    }

    #[tokio::test]
    #[ignore]
    async fn test_cpp_same_file_definition() {
        if !clangd_available() {
            eprintln!("Skipping: clangd not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_cpp = r#"#include <iostream>

void helper();

int main() {
    helper();
    return 0;
}

void helper() {
    std::cout << "hello" << std::endl;
}
"#;

        let compile_commands = r#"[
    {
        "directory": ".",
        "command": "c++ -std=c++17 -c main.cpp",
        "file": "main.cpp"
    }
]
"#;

        fs::write(dir.path().join("main.cpp"), main_cpp).unwrap();
        fs::write(dir.path().join("compile_commands.json"), compile_commands).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("cpp").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.cpp"),
            main_cpp,
        );

        let calls = collect_calls(&index);
        assert!(!calls.is_empty(), "Should extract calls from C++ code");

        let helper_call = calls.iter().find(|c| c.callee == "helper");
        assert!(helper_call.is_some(), "Should find call to helper()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = helper_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve helper() call");
            assert_eq!(def_name.unwrap(), "helper");
        }

        resolver.shutdown_all().await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_cpp_method_definition() {
        if !clangd_available() {
            eprintln!("Skipping: clangd not available");
            return;
        }

        let dir = TempDir::new().unwrap();

        let main_cpp = r#"#include <iostream>

class Processor {
public:
    void process();
};

void Processor::process() {
    std::cout << "processing" << std::endl;
}

int main() {
    Processor p;
    p.process();
    return 0;
}
"#;

        let compile_commands = r#"[
    {
        "directory": ".",
        "command": "c++ -std=c++17 -c main.cpp",
        "file": "main.cpp"
    }
]
"#;

        fs::write(dir.path().join("main.cpp"), main_cpp).unwrap();
        fs::write(dir.path().join("compile_commands.json"), compile_commands).unwrap();

        let mut index = Index::new();
        let extractor = Extractor::new("cpp").unwrap();
        index_file(
            &mut index,
            &extractor,
            dir.path(),
            &dir.path().join("main.cpp"),
            main_cpp,
        );

        let calls = collect_calls(&index);
        let process_call = calls.iter().find(|c| c.callee == "process");
        assert!(process_call.is_some(), "Should find call to process()");

        let mut resolver = AsyncLspResolver::new(dir.path());

        if let Some(call) = process_call {
            let def_name = resolve_call(&mut resolver, call, &index).await;
            assert!(def_name.is_some(), "LSP should resolve p.process() call");
            assert_eq!(def_name.unwrap(), "process");
        }

        resolver.shutdown_all().await;
    }
}

mod lsp_availability {
    use glimpse::code::lsp::check_lsp_availability;

    #[test]
    fn test_check_lsp_availability_returns_results() {
        let availability = check_lsp_availability();

        assert!(
            !availability.is_empty(),
            "Should return availability for at least one language"
        );

        for (lang, info) in &availability {
            println!(
                "  {}: available={}, location={:?}, can_install={}, method={:?}",
                lang, info.available, info.location, info.can_auto_install, info.install_method
            );
        }
    }

    #[test]
    fn test_rust_analyzer_detection() {
        let availability = check_lsp_availability();

        if let Some(info) = availability.get("rust") {
            println!(
                "rust-analyzer: available={}, location={:?}, can_install={}",
                info.available, info.location, info.can_auto_install
            );
            if info.available {
                assert!(
                    info.location.is_some(),
                    "If available, should have location"
                );
            }
        }
    }

    #[test]
    fn test_npm_packages_can_be_installed() {
        let availability = check_lsp_availability();

        if let Some(info) = availability.get("typescript") {
            println!(
                "typescript-language-server: available={}, can_install={}, method={:?}",
                info.available, info.can_auto_install, info.install_method
            );
            if !info.available {
                assert!(
                    matches!(info.install_method.as_deref(), Some("bun" | "npm")),
                    "expected bun or npm, got {:?}",
                    info.install_method
                );
            }
        }

        if let Some(info) = availability.get("python") {
            println!(
                "pyright: available={}, can_install={}, method={:?}",
                info.available, info.can_auto_install, info.install_method
            );
            if !info.available {
                assert!(
                    matches!(info.install_method.as_deref(), Some("bun" | "npm")),
                    "expected bun or npm, got {:?}",
                    info.install_method
                );
            }
        }
    }

    #[test]
    fn test_go_package_can_be_installed() {
        let availability = check_lsp_availability();

        if let Some(info) = availability.get("go") {
            println!(
                "gopls: available={}, can_install={}, method={:?}",
                info.available, info.can_auto_install, info.install_method
            );
            if !info.available {
                assert_eq!(info.install_method.as_deref(), Some("go"));
            }
        }
    }
}

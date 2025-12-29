use std::path::Path;

use glimpse::code::extract::Extractor;
use glimpse::code::index::DefinitionKind;
use tree_sitter::Parser;

fn parse_and_extract(lang: &str, source: &str) -> ExtractResult {
    let extractor = Extractor::new(lang).expect(&format!("failed to load {}", lang));
    let mut parser = Parser::new();
    parser
        .set_language(extractor.language())
        .expect("failed to set language");
    let tree = parser.parse(source, None).expect("failed to parse");
    let path = Path::new("test.src");

    ExtractResult {
        definitions: extractor.extract_definitions(&tree, source.as_bytes(), path),
        calls: extractor.extract_calls(&tree, source.as_bytes(), path),
        imports: extractor.extract_imports(&tree, source.as_bytes(), path),
    }
}

struct ExtractResult {
    definitions: Vec<glimpse::code::index::Definition>,
    calls: Vec<glimpse::code::index::Call>,
    imports: Vec<glimpse::code::index::Import>,
}

mod rust {
    use super::*;

    const SAMPLE: &str = r#"
use std::fs;
use std::path::Path;
use crate::config::Config;

fn main() {
    let config = Config::load();
    helper(config);
    println!("done");
}

fn helper(cfg: Config) {
    cfg.validate();
    process(cfg);
}

fn process(cfg: Config) {
    fs::write("out.txt", cfg.data());
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("rust", SAMPLE);

        assert_eq!(result.definitions.len(), 3);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"process"));

        for def in &result.definitions {
            assert!(matches!(def.kind, DefinitionKind::Function));
        }
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("rust", SAMPLE);

        let callers: Vec<_> = result
            .calls
            .iter()
            .filter_map(|c| {
                c.caller
                    .as_ref()
                    .map(|caller| (caller.as_str(), c.callee.as_str()))
            })
            .collect();

        assert!(callers.contains(&("main", "helper")));
        assert!(callers.contains(&("helper", "process")));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("rust", SAMPLE);

        assert!(!result.imports.is_empty());
        let paths: Vec<_> = result.imports.iter().map(|i| &i.module_path).collect();
        assert!(paths.iter().any(|p| p.contains("std")));
    }
}

mod python {
    use super::*;

    const SAMPLE: &str = r#"
import os
from pathlib import Path
from typing import Optional

def main():
    config = load_config()
    process(config)

def load_config():
    return Config()

def process(config):
    save(config.data)

class Config:
    def __init__(self):
        self.data = {}

    def validate(self):
        return True
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("python", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"load_config"));
        assert!(names.contains(&"process"));
        assert!(names.contains(&"Config"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("python", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"load_config"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("python", SAMPLE);

        assert!(!result.imports.is_empty());
        let paths: Vec<_> = result.imports.iter().map(|i| &i.module_path).collect();
        assert!(paths
            .iter()
            .any(|p| p.contains("os") || p.contains("pathlib")));
    }
}

mod typescript {
    use super::*;

    const SAMPLE: &str = r#"
import { readFile } from 'fs';
import path from 'path';

function main() {
    const config = loadConfig();
    process(config);
}

function loadConfig(): Config {
    return new Config();
}

const process = (config: Config) => {
    config.validate();
    save(config);
};

class Config {
    validate() {
        return true;
    }
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("typescript", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"loadConfig"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("typescript", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"loadConfig"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("typescript", SAMPLE);

        assert!(!result.imports.is_empty());
    }
}

mod javascript {
    use super::*;

    const SAMPLE: &str = r#"
const fs = require('fs');
import { join } from 'path';

function main() {
    const data = loadData();
    process(data);
}

function loadData() {
    return fs.readFileSync('data.json');
}

const process = (data) => {
    transform(data);
};
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("javascript", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"loadData"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("javascript", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"loadData"));
        assert!(callees.contains(&"process"));
    }
}

mod go {
    use super::*;

    const SAMPLE: &str = r#"
package main

import (
    "fmt"
    "os"
)

func main() {
    config := loadConfig()
    process(config)
}

func loadConfig() *Config {
    return &Config{}
}

func process(cfg *Config) {
    cfg.Validate()
    save(cfg)
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("go", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"loadConfig"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("go", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"loadConfig"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("go", SAMPLE);

        assert!(!result.imports.is_empty());
    }
}

mod c {
    use super::*;

    const SAMPLE: &str = r#"
#include <stdio.h>
#include "config.h"

void process(Config* cfg);

int main() {
    Config* cfg = load_config();
    process(cfg);
    return 0;
}

Config* load_config() {
    return malloc(sizeof(Config));
}

void process(Config* cfg) {
    validate(cfg);
    save(cfg);
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("c", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"load_config"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("c", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"load_config"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("c", SAMPLE);

        assert!(!result.imports.is_empty());
    }
}

mod cpp {
    use super::*;

    const SAMPLE: &str = r#"
#include <iostream>
#include "config.hpp"

class Processor {
public:
    void run() {
        process();
    }

    void process() {
        helper();
    }
};

int main() {
    Processor p;
    p.run();
    return 0;
}

void standalone() {
    std::cout << "hello" << std::endl;
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("cpp", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"standalone"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("cpp", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"run"));
    }
}

mod java {
    use super::*;

    const SAMPLE: &str = r#"
import java.util.List;
import com.example.Config;

public class Main {
    public static void main(String[] args) {
        Config config = loadConfig();
        process(config);
    }

    private static Config loadConfig() {
        return new Config();
    }

    private static void process(Config cfg) {
        cfg.validate();
        save(cfg);
    }
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("java", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"loadConfig"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("java", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"loadConfig"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("java", SAMPLE);

        assert!(!result.imports.is_empty());
    }
}

mod bash {
    use super::*;

    const SAMPLE: &str = r#"
#!/bin/bash

source ./config.sh

main() {
    load_config
    process "$1"
}

load_config() {
    echo "loading"
}

process() {
    validate "$1"
    save "$1"
}

main "$@"
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("bash", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"load_config"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("bash", SAMPLE);

        // Bash treats all commands as calls
        assert!(!result.calls.is_empty());
    }
}

mod zig {
    use super::*;

    const SAMPLE: &str = r#"
const std = @import("std");

pub fn main() void {
    const config = loadConfig();
    process(config);
}

fn loadConfig() Config {
    return Config{};
}

fn process(cfg: Config) void {
    cfg.validate();
    save(cfg);
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("zig", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"loadConfig"));
        assert!(names.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("zig", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"loadConfig"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("zig", SAMPLE);

        assert!(!result.imports.is_empty());
        assert!(result.imports.iter().any(|i| i.module_path == "std"));
    }
}

mod scala {
    use super::*;

    const SAMPLE: &str = r#"
import scala.collection.mutable
import com.example.Config

object Main {
    def main(args: Array[String]): Unit = {
        val config = loadConfig()
        process(config)
    }

    def loadConfig(): Config = {
        new Config()
    }

    def process(cfg: Config): Unit = {
        cfg.validate()
        save(cfg)
    }
}

class Processor {
    def run(): Unit = {
        helper()
    }
}

trait Validator {
    def validate(): Boolean
}
"#;

    #[test]
    #[ignore]
    fn definitions() {
        let result = parse_and_extract("scala", SAMPLE);

        let names: Vec<_> = result.definitions.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"loadConfig"));
        assert!(names.contains(&"process"));
        assert!(names.contains(&"Main"));
        assert!(names.contains(&"Processor"));
        assert!(names.contains(&"Validator"));
    }

    #[test]
    #[ignore]
    fn calls() {
        let result = parse_and_extract("scala", SAMPLE);

        let callees: Vec<_> = result.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callees.contains(&"loadConfig"));
        assert!(callees.contains(&"process"));
    }

    #[test]
    #[ignore]
    fn imports() {
        let result = parse_and_extract("scala", SAMPLE);

        assert!(!result.imports.is_empty());
    }
}

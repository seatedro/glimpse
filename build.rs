use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[derive(Deserialize)]
struct Language {
    #[serde(default)]
    r#_type: Option<String>,
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    filenames: Vec<String>,
    #[serde(default)]
    interpreters: Vec<String>,
    #[serde(default)]
    _language_id: Option<i32>,
}

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let languages_path = Path::new(&manifest_dir).join("languages.yml");

    println!(
        "cargo:rerun-if-changed={}",
        languages_path.to_string_lossy()
    );

    let yaml_content =
        std::fs::read_to_string(&languages_path).expect("Failed to read languages.yml");
    let languages: HashMap<String, Language> =
        serde_yaml::from_str(&yaml_content).expect("Failed to parse languages.yml");

    let mut code = String::new();

    code.push_str("use once_cell::sync::Lazy;\n");
    code.push_str("use std::collections::HashSet;\n\n");

    code.push_str("pub static SOURCE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {\n");
    code.push_str("    let mut set = HashSet::new();\n\n");

    for lang in languages.values() {
        for ext in &lang.extensions {
            let ext = ext.trim_start_matches('.');
            code.push_str(&format!("    set.insert(\"{ext}\");\n"));
        }
    }

    code.push_str("    set\n");
    code.push_str("});\n\n");

    code.push_str("pub static KNOWN_FILENAMES: Lazy<HashSet<&'static str>> = Lazy::new(|| {\n");
    code.push_str("    let mut set = HashSet::new();\n\n");

    for lang in languages.values() {
        for filename in &lang.filenames {
            code.push_str(&format!("    set.insert(\"{filename}\");\n"));
        }
    }

    code.push_str("    set\n");
    code.push_str("});\n\n");

    code.push_str("pub static INTERPRETER_NAMES: Lazy<HashSet<&'static str>> = Lazy::new(|| {\n");
    code.push_str("    let mut set = HashSet::new();\n\n");

    for lang in languages.values() {
        for interpreter in &lang.interpreters {
            code.push_str(&format!("    set.insert(\"{interpreter}\");\n"));
        }
    }

    code.push_str("    set\n");
    code.push_str("});\n");

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("languages.rs");
    let mut f = File::create(dest_path).unwrap();
    f.write_all(code.as_bytes()).unwrap();
}

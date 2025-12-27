use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use git2::Repository;

use super::loader::cache_dir;
use super::registry::LanguageEntry;

fn sources_dir() -> PathBuf {
    cache_dir().join("sources")
}

pub fn fetch_grammar(lang: &LanguageEntry) -> Result<PathBuf> {
    let sources = sources_dir();
    fs::create_dir_all(&sources)?;

    let dest = sources.join(&lang.name);

    if dest.exists() {
        return Ok(dest);
    }

    Repository::clone(&lang.repo, &dest)
        .with_context(|| format!("failed to clone grammar repo: {}", lang.repo))?;

    let repo = Repository::open(&dest)?;
    let (object, reference) = repo.revparse_ext(&lang.branch)?;
    repo.checkout_tree(&object, None)?;
    match reference {
        Some(r) => repo.set_head(r.name().unwrap())?,
        None => repo.set_head_detached(object.id())?,
    }

    Ok(dest)
}

pub fn compile_grammar(lang: &LanguageEntry, grammar_dir: &Path) -> Result<PathBuf> {
    let output_dir = cache_dir();
    fs::create_dir_all(&output_dir)?;

    let lib_name = format!("tree-sitter-{}", lang.name);
    let output_path = output_dir.join(lib_filename(&lib_name));

    if output_path.exists() {
        return Ok(output_path);
    }

    let src_dir = match &lang.subpath {
        Some(subpath) => grammar_dir.join(subpath).join("src"),
        None => grammar_dir.join("src"),
    };

    let parser_c = src_dir.join("parser.c");
    if !parser_c.exists() {
        bail!("parser.c not found at: {}", parser_c.display());
    }

    let temp_dir = tempfile::tempdir()?;
    let mut objects = Vec::new();

    objects.push(compile_c_file(&parser_c, &src_dir, temp_dir.path())?);

    let scanner_c = src_dir.join("scanner.c");
    if scanner_c.exists() {
        objects.push(compile_c_file(&scanner_c, &src_dir, temp_dir.path())?);
    }

    let scanner_cc = src_dir.join("scanner.cc");
    if scanner_cc.exists() {
        objects.push(compile_cpp_file(&scanner_cc, &src_dir, temp_dir.path())?);
    }

    link_shared_library(&objects, &output_path)?;

    Ok(output_path)
}

fn compile_c_file(source: &Path, include_dir: &Path, out_dir: &Path) -> Result<PathBuf> {
    let obj_name = source.file_stem().unwrap().to_string_lossy();
    let obj_path = out_dir.join(format!("{}.o", obj_name));

    let status = Command::new("cc")
        .args(["-c", "-O3", "-fPIC", "-w"])
        .arg("-I")
        .arg(include_dir)
        .arg("-o")
        .arg(&obj_path)
        .arg(source)
        .status()
        .context("failed to run cc")?;

    if !status.success() {
        bail!("failed to compile: {}", source.display());
    }

    Ok(obj_path)
}

fn compile_cpp_file(source: &Path, include_dir: &Path, out_dir: &Path) -> Result<PathBuf> {
    let obj_name = source.file_stem().unwrap().to_string_lossy();
    let obj_path = out_dir.join(format!("{}.o", obj_name));

    let status = Command::new("c++")
        .args(["-c", "-O3", "-fPIC", "-w"])
        .arg("-I")
        .arg(include_dir)
        .arg("-o")
        .arg(&obj_path)
        .arg(source)
        .status()
        .context("failed to run c++")?;

    if !status.success() {
        bail!("failed to compile: {}", source.display());
    }

    Ok(obj_path)
}

fn link_shared_library(objects: &[PathBuf], output: &Path) -> Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("cc");
        c.args(["-dynamiclib", "-undefined", "dynamic_lookup"]);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("link");
        c.arg("/DLL");
        c
    } else {
        let mut c = Command::new("cc");
        c.arg("-shared");
        c
    };

    for obj in objects {
        cmd.arg(obj);
    }

    if cfg!(target_os = "windows") {
        cmd.arg(format!("/OUT:{}", output.display()));
    } else {
        cmd.arg("-o").arg(output);
    }

    let status = cmd.status().context("failed to link shared library")?;

    if !status.success() {
        bail!("failed to link shared library: {}", output.display());
    }

    Ok(())
}

fn lib_filename(name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{}.dylib", name)
    } else if cfg!(target_os = "windows") {
        format!("{}.dll", name)
    } else {
        format!("lib{}.so", name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lib_filename() {
        let name = "tree-sitter-rust";
        let filename = lib_filename(name);

        if cfg!(target_os = "macos") {
            assert_eq!(filename, "libtree-sitter-rust.dylib");
        } else if cfg!(target_os = "windows") {
            assert_eq!(filename, "tree-sitter-rust.dll");
        } else {
            assert_eq!(filename, "libtree-sitter-rust.so");
        }
    }

    #[test]
    fn test_sources_dir() {
        let dir = sources_dir();
        assert!(dir.ends_with("grammars/sources"));
    }
}

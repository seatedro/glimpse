use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const INDEX_FILE: &str = "index.bin";
pub const INDEX_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    pub kind: DefinitionKind,
    pub span: Span,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefinitionKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Trait,
    Interface,
    Module,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub callee: String,
    pub span: Span,
    pub file: PathBuf,
    pub caller: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub module_path: String,
    pub alias: Option<String>,
    pub span: Span,
    pub file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: PathBuf,
    pub mtime: u64,
    pub size: u64,
    pub definitions: Vec<Definition>,
    pub calls: Vec<Call>,
    pub imports: Vec<Import>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Index {
    pub files: HashMap<PathBuf, FileRecord>,
    pub version: u32,
}

impl Index {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            version: INDEX_VERSION,
        }
    }

    pub fn is_stale(&self, path: &Path, mtime: u64, size: u64) -> bool {
        match self.files.get(path) {
            Some(record) => record.mtime != mtime || record.size != size,
            None => true,
        }
    }

    pub fn update(&mut self, record: FileRecord) {
        self.files.insert(record.path.clone(), record);
    }

    pub fn remove(&mut self, path: &Path) {
        self.files.remove(path);
    }

    pub fn get(&self, path: &Path) -> Option<&FileRecord> {
        self.files.get(path)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &Definition> {
        self.files.values().flat_map(|f| &f.definitions)
    }

    pub fn calls(&self) -> impl Iterator<Item = &Call> {
        self.files.values().flat_map(|f| &f.calls)
    }

    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.files.values().flat_map(|f| &f.imports)
    }
}

pub fn file_fingerprint(path: &Path) -> Result<(u64, u64)> {
    let meta = fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let mtime = meta
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let size = meta.len();
    Ok((mtime, size))
}

fn hash_path(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn index_dir() -> Result<PathBuf> {
    dirs::data_local_dir()
        .map(|d| d.join("glimpse").join("indices"))
        .context("could not determine local data directory")
}

pub fn index_path(root: &Path) -> Result<PathBuf> {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let hash = hash_path(&canonical);
    Ok(index_dir()?.join(hash).join(INDEX_FILE))
}

pub fn save_index(index: &Index, root: &Path) -> Result<()> {
    let path = index_path(root)?;
    let dir = path.parent().unwrap();
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let file = File::create(&path).with_context(|| format!("failed to create {}", path.display()))?;
    let writer = BufWriter::new(file);

    bincode::serialize_into(writer, index).context("failed to serialize index")?;
    Ok(())
}

pub fn load_index(root: &Path) -> Result<Option<Index>> {
    let path = index_path(root)?;
    if !path.exists() {
        return Ok(None);
    }

    let file = File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);

    let index: Index = match bincode::deserialize_from(reader) {
        Ok(idx) => idx,
        Err(_) => return Ok(None),
    };

    if index.version != INDEX_VERSION {
        return Ok(None);
    }

    Ok(Some(index))
}

pub fn clear_index(root: &Path) -> Result<()> {
    let path = index_path(root)?;
    if let Some(dir) = path.parent() {
        if dir.exists() {
            fs::remove_dir_all(dir).with_context(|| format!("failed to remove {}", dir.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_record(name: &str) -> FileRecord {
        FileRecord {
            path: PathBuf::from(format!("src/{}.rs", name)),
            mtime: 1234567890,
            size: 1024,
            definitions: vec![Definition {
                name: format!("{}_fn", name),
                kind: DefinitionKind::Function,
                span: Span {
                    start_byte: 0,
                    end_byte: 100,
                    start_line: 1,
                    end_line: 10,
                },
                file: PathBuf::from(format!("src/{}.rs", name)),
            }],
            calls: vec![Call {
                callee: "other_fn".to_string(),
                span: Span {
                    start_byte: 50,
                    end_byte: 60,
                    start_line: 5,
                    end_line: 5,
                },
                file: PathBuf::from(format!("src/{}.rs", name)),
                caller: Some(format!("{}_fn", name)),
            }],
            imports: vec![Import {
                module_path: "std::fs".to_string(),
                alias: None,
                span: Span {
                    start_byte: 0,
                    end_byte: 15,
                    start_line: 1,
                    end_line: 1,
                },
                file: PathBuf::from(format!("src/{}.rs", name)),
            }],
        }
    }

    #[test]
    fn test_index_update_and_get() {
        let mut index = Index::new();
        let record = make_test_record("main");

        index.update(record.clone());
        let got = index.get(Path::new("src/main.rs")).unwrap();

        assert_eq!(got.path, record.path);
        assert_eq!(got.definitions.len(), 1);
        assert_eq!(got.calls.len(), 1);
        assert_eq!(got.imports.len(), 1);
    }

    #[test]
    fn test_index_is_stale() {
        let mut index = Index::new();
        let record = make_test_record("lib");
        index.update(record);

        assert!(!index.is_stale(Path::new("src/lib.rs"), 1234567890, 1024));
        assert!(index.is_stale(Path::new("src/lib.rs"), 1234567891, 1024));
        assert!(index.is_stale(Path::new("src/lib.rs"), 1234567890, 2048));
        assert!(index.is_stale(Path::new("src/other.rs"), 1234567890, 1024));
    }

    #[test]
    fn test_index_remove() {
        let mut index = Index::new();
        index.update(make_test_record("foo"));
        index.update(make_test_record("bar"));

        assert!(index.get(Path::new("src/foo.rs")).is_some());
        index.remove(Path::new("src/foo.rs"));
        assert!(index.get(Path::new("src/foo.rs")).is_none());
        assert!(index.get(Path::new("src/bar.rs")).is_some());
    }

    #[test]
    fn test_index_iterators() {
        let mut index = Index::new();
        index.update(make_test_record("a"));
        index.update(make_test_record("b"));

        assert_eq!(index.definitions().count(), 2);
        assert_eq!(index.calls().count(), 2);
        assert_eq!(index.imports().count(), 2);
    }

    #[test]
    fn test_index_path_uses_data_dir() {
        let path = index_path(Path::new("/some/project")).unwrap();
        let data_dir = dirs::data_local_dir().unwrap();
        assert!(path.starts_with(data_dir.join("glimpse").join("indices")));
        assert!(path.ends_with(INDEX_FILE));
    }

    #[test]
    fn test_index_path_different_projects() {
        let path1 = index_path(Path::new("/project/a")).unwrap();
        let path2 = index_path(Path::new("/project/b")).unwrap();
        assert_ne!(path1, path2);
    }

    #[test]
    fn test_save_and_load_index() {
        let project_dir = tempfile::tempdir().unwrap();
        let mut index = Index::new();
        index.update(make_test_record("main"));
        index.update(make_test_record("lib"));

        save_index(&index, project_dir.path()).unwrap();

        let loaded = load_index(project_dir.path()).unwrap().unwrap();
        assert_eq!(loaded.version, INDEX_VERSION);
        assert_eq!(loaded.files.len(), 2);
        assert!(loaded.get(Path::new("src/main.rs")).is_some());
        assert!(loaded.get(Path::new("src/lib.rs")).is_some());

        clear_index(project_dir.path()).unwrap();
    }

    #[test]
    fn test_load_index_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_index(dir.path()).unwrap();
        assert!(result.is_none());
    }
}

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::path::Path;

pub struct PatternMatcher {
    include_patterns: Option<GlobSet>,
    exclude_patterns: Option<GlobSet>,
}

impl PatternMatcher {
    pub fn new(
        include_patterns: Option<Vec<String>>,
        exclude_patterns: Option<Vec<String>>,
    ) -> anyhow::Result<Self> {
        let include_patterns = match include_patterns {
            Some(patterns) => {
                let mut builder = GlobSetBuilder::new();
                for pattern in patterns {
                    builder.add(Glob::new(&pattern)?);
                }
                Some(builder.build()?)
            }
            None => None,
        };

        let exclude_patterns = match exclude_patterns {
            Some(patterns) => {
                let mut builder = GlobSetBuilder::new();
                for pattern in patterns {
                    builder.add(Glob::new(&pattern)?);
                }
                Some(builder.build()?)
            }
            None => Some(default_excludes()?),
        };

        Ok(PatternMatcher {
            include_patterns,
            exclude_patterns,
        })
    }

    pub fn should_process(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // If include patterns exist, path must match at least one
        if let Some(ref includes) = self.include_patterns {
            if !includes.is_match(&*path_str) {
                return false;
            }
        }

        // If exclude patterns exist, path must not match any
        if let Some(ref excludes) = self.exclude_patterns {
            if excludes.is_match(&*path_str) {
                return false;
            }
        }

        true
    }
}

fn default_excludes() -> anyhow::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();

    // Version control
    builder.add(Glob::new("**/.git/**")?);
    builder.add(Glob::new("**/.svn/**")?);
    builder.add(Glob::new("**/.hg/**")?);

    // Build artifacts and dependencies
    builder.add(Glob::new("**/target/**")?);
    builder.add(Glob::new("**/node_modules/**")?);
    builder.add(Glob::new("**/dist/**")?);
    builder.add(Glob::new("**/build/**")?);

    // Binaries and objects
    builder.add(Glob::new("**/*.exe")?);
    builder.add(Glob::new("**/*.dll")?);
    builder.add(Glob::new("**/*.so")?);
    builder.add(Glob::new("**/*.dylib")?);
    builder.add(Glob::new("**/*.o")?);
    builder.add(Glob::new("**/*.obj")?);

    // Cache directories
    builder.add(Glob::new("**/__pycache__/**")?);
    builder.add(Glob::new("**/.mypy_cache/**")?);
    builder.add(Glob::new("**/.pytest_cache/**")?);

    Ok(builder.build()?)
}

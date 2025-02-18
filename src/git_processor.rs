use anyhow::Result;
use git2::Repository;
use std::path::PathBuf;
use tempfile::TempDir;
use url::Url;

pub struct GitProcessor {
    temp_dir: TempDir,
}

impl GitProcessor {
    pub fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    pub fn process_repo(&self, url: &str) -> Result<PathBuf> {
        let parsed_url = Url::parse(url)?;
        let repo_name = parsed_url
            .path_segments()
            .and_then(|segments| segments.last())
            .map(|name| name.trim_end_matches(".git"))
            .unwrap_or("repo")
            .to_string();

        let clone_path = self.temp_dir.path().join(&repo_name);

        Repository::clone(url, &clone_path)?;

        Ok(clone_path)
    }

    pub fn is_git_url(url: &str) -> bool {
        if let Ok(parsed_url) = Url::parse(url) {
            let host = parsed_url.host_str().unwrap_or("");
            let is_git_host = host.contains("github.com")
                || host.contains("gitlab.com")
                || host.contains("bitbucket.org")
                || host.contains("dev.azure.com");

            let is_git_protocol = parsed_url.scheme() == "git"
                || url.ends_with(".git")
                || (is_git_host && !url.contains("/raw/"));

            return is_git_protocol;
        }
        false
    }
}

impl Drop for GitProcessor {
    fn drop(&mut self) {
        // Temp directory will be automatically cleaned up when dropped
    }
}

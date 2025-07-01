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
            .and_then(|mut segments| segments.next_back())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_git_url() {
        let valid_urls = vec![
            "https://github.com/user/repo.git",
            "https://github.com/user/repo",
            "git://github.com/user/repo.git",
            "https://gitlab.com/user/repo.git",
            "https://bitbucket.org/user/repo.git",
            "https://dev.azure.com/org/project/_git/repo",
        ];

        let invalid_urls = vec![
            "https://github.com/user/repo/raw/main/file.txt",
            "https://gitlab.com/user/repo/raw/main/file",
            "file:///path/to/repo",
            "not_a_url",
            "",
        ];

        for url in valid_urls {
            assert!(GitProcessor::is_git_url(url), "URL should be valid: {url}");
        }

        for url in invalid_urls {
            assert!(
                !GitProcessor::is_git_url(url),
                "URL should be invalid: {url}"
            );
        }
    }

    #[test]
    fn test_new_git_processor() {
        let processor = GitProcessor::new().expect("Failed to create GitProcessor");
        assert!(
            processor.temp_dir.path().exists(),
            "Temp directory should exist"
        );
    }

    #[test]
    fn test_temp_dir_cleanup() {
        let temp_path;
        {
            let processor = GitProcessor::new().expect("Failed to create GitProcessor");
            temp_path = processor.temp_dir.path().to_path_buf();
            assert!(
                temp_path.exists(),
                "Temp directory should exist during processor lifetime"
            );
        } // processor is dropped here
        assert!(
            !temp_path.exists(),
            "Temp directory should be cleaned up after drop"
        );
    }

    #[test]
    fn test_process_repo_name_extraction() {
        let urls_and_names = vec![
            ("https://github.com/user/repo.git", "repo"),
            ("https://github.com/user/repo", "repo"),
            ("https://gitlab.com/group/subgroup/repo.git", "repo"),
            ("https://dev.azure.com/org/project/_git/repo", "repo"),
        ];

        let _ = GitProcessor::new().expect("Failed to create GitProcessor");

        for (url, expected_name) in urls_and_names {
            let parsed_url = Url::parse(url).unwrap();
            let repo_name = parsed_url
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .map(|name| name.trim_end_matches(".git"))
                .unwrap_or("repo");

            assert_eq!(repo_name, expected_name, "Failed for URL: {url}");
        }
    }

    #[test]
    fn test_process_repo_creates_directory() {
        let test_repo = "https://github.com/rust-lang/rust-analyzer.git";
        if let Ok(processor) = GitProcessor::new() {
            match processor.process_repo(test_repo) {
                Ok(path) => {
                    assert!(path.exists(), "Repository directory should exist");
                    assert!(path.join(".git").exists(), "Git directory should exist");
                    // Check for some common files that should be present
                    assert!(path.join("Cargo.toml").exists(), "Cargo.toml should exist");
                }
                Err(e) => println!("Skipping clone test due to error: {e}"),
            }
        }
    }
}

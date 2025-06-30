use std::path::{Path, PathBuf};
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::cli::{Exclude, OutputFormat};

#[derive(Debug, Serialize, Clone)]
#[serde(into = "String")]
pub struct BackwardsCompatOutputFormat(OutputFormat);

impl From<BackwardsCompatOutputFormat> for String {
    fn from(format: BackwardsCompatOutputFormat) -> Self {
        match format.0 {
            OutputFormat::Tree => "tree".to_string(),
            OutputFormat::Files => "files".to_string(),
            OutputFormat::Both => "both".to_string(),
        }
    }
}

impl<'de> Deserialize<'de> for BackwardsCompatOutputFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum FormatOrString {
            Format(OutputFormat),
            String(String),
        }

        match FormatOrString::deserialize(deserializer)? {
            FormatOrString::Format(format) => Ok(BackwardsCompatOutputFormat(format)),
            FormatOrString::String(s) => {
                let format = match s.to_lowercase().as_str() {
                    "tree" => OutputFormat::Tree,
                    "files" => OutputFormat::Files,
                    "both" => OutputFormat::Both,
                    _ => OutputFormat::Both, // Default to Both for unknown values
                };
                Ok(BackwardsCompatOutputFormat(format))
            }
        }
    }
}

impl From<OutputFormat> for BackwardsCompatOutputFormat {
    fn from(format: OutputFormat) -> Self {
        BackwardsCompatOutputFormat(format)
    }
}

impl From<BackwardsCompatOutputFormat> for OutputFormat {
    fn from(format: BackwardsCompatOutputFormat) -> Self {
        format.0
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_max_size")]
    pub max_size: u64,

    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    #[serde(default = "default_output_format")]
    pub default_output_format: BackwardsCompatOutputFormat,

    #[serde(default)]
    pub default_excludes: Vec<Exclude>,

    #[serde(default = "default_tokenizer_type")]
    pub default_tokenizer: String,

    #[serde(default = "default_tokenizer_model")]
    pub default_tokenizer_model: String,

    #[serde(default = "default_link_depth")]
    pub default_link_depth: usize,

    #[serde(default)]
    pub traverse_links: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_size: default_max_size(),
            max_depth: default_max_depth(),
            default_output_format: default_output_format(),
            default_excludes: default_excludes(),
            default_tokenizer: default_tokenizer_type(),
            default_tokenizer_model: default_tokenizer_model(),
            default_link_depth: default_link_depth(),
            traverse_links: false,
        }
    }
}

fn default_tokenizer_type() -> String {
    "tiktoken".to_string()
}

fn default_tokenizer_model() -> String {
    "gpt2".to_string()
}

fn default_max_size() -> u64 {
    10 * 1024 * 1024 // 10MB
}

fn default_max_depth() -> usize {
    20
}

fn default_output_format() -> BackwardsCompatOutputFormat {
    BackwardsCompatOutputFormat(OutputFormat::Both)
}

fn default_excludes() -> Vec<Exclude> {
    vec![
        // Version control
        Exclude::Pattern("**/.git/**".to_string()),
        Exclude::Pattern("**/.svn/**".to_string()),
        Exclude::Pattern("**/.hg/**".to_string()),
        // Build artifacts and dependencies
        Exclude::Pattern("**/target/**".to_string()),
        Exclude::Pattern("**/node_modules/**".to_string()),
        Exclude::Pattern("**/dist/**".to_string()),
        Exclude::Pattern("**/build/**".to_string()),
    ]
}

fn default_link_depth() -> usize {
    1
}

pub fn load_config() -> anyhow::Result<Config> {
    let config_path = get_config_path()?;

    if !config_path.exists() {
        let config = Config::default();
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        let config_str = toml::to_string_pretty(&config)?;
        std::fs::write(&config_path, config_str)?;
        return Ok(config);
    }

    let config_str = std::fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config_str)?;
    Ok(config)
}

pub fn get_config_path() -> anyhow::Result<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config).join("glimpse/config.toml"));
        }

        if let Some(home) = dirs::home_dir() {
            let xdg_config = home.join(".config/glimpse/config.toml");
            if xdg_config.exists() {
                return Ok(xdg_config);
            }
        }
    }

    // Fall back to platform-specific directory
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join("glimpse");
    Ok(config_dir.join("config.toml"))
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct RepoConfig {
    pub include: Option<Vec<String>>,
    pub exclude: Option<Vec<Exclude>>,
    pub max_size: Option<u64>,
    pub max_depth: Option<usize>,
    pub output: Option<BackwardsCompatOutputFormat>,
    pub file: Option<PathBuf>,
    pub hidden: Option<bool>,
    pub no_ignore: Option<bool>,
}

pub fn save_repo_config(path: &Path, repo_config: &RepoConfig) -> anyhow::Result<()> {
    let config_str = toml::to_string_pretty(repo_config)?;
    std::fs::write(path, config_str)?;
    Ok(())
}

pub fn load_repo_config(path: &Path) -> anyhow::Result<RepoConfig> {
    if path.exists() {
        let config_str = std::fs::read_to_string(path)?;
        let config: RepoConfig = toml::from_str(&config_str)?;
        Ok(config)
    } else {
        Ok(RepoConfig::default())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GlobalConfig {
    /// Projects where the user has declined to save local config
    pub declined_projects: HashSet<String>,
}

pub fn get_global_config_path() -> anyhow::Result<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config).join("glimpse/global.toml"));
        }

        if let Some(home) = dirs::home_dir() {
            let xdg_config = home.join(".config/glimpse/global.toml");
            return Ok(xdg_config);
        }
    }

    // Fall back to platform-specific directory
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join("glimpse");
    Ok(config_dir.join("global.toml"))
}

pub fn load_global_config() -> anyhow::Result<GlobalConfig> {
    let config_path = get_global_config_path()?;
    
    if !config_path.exists() {
        return Ok(GlobalConfig::default());
    }

    let config_str = std::fs::read_to_string(config_path)?;
    let config: GlobalConfig = toml::from_str(&config_str)?;
    Ok(config)
}

pub fn save_global_config(global_config: &GlobalConfig) -> anyhow::Result<()> {
    let config_path = get_global_config_path()?;
    
    // Ensure the directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let config_str = toml::to_string_pretty(global_config)?;
    std::fs::write(config_path, config_str)?;
    Ok(())
}

pub fn mark_project_declined(project_path: &Path) -> anyhow::Result<()> {
    let mut global_config = load_global_config()?;
    let canonical_path = project_path.canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf())
        .to_string_lossy()
        .to_string();
    
    global_config.declined_projects.insert(canonical_path);
    save_global_config(&global_config)?;
    Ok(())
}

pub fn is_project_declined(project_path: &Path) -> bool {
    let global_config = load_global_config().unwrap_or_default();
    let canonical_path = project_path.canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf())
        .to_string_lossy()
        .to_string();
    
    global_config.declined_projects.contains(&canonical_path)
}

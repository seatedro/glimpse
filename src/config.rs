use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::cli::Exclude;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_max_size")]
    pub max_size: u64,

    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    #[serde(default = "default_output_format")]
    pub default_output_format: String,

    #[serde(default)]
    pub default_excludes: Vec<Exclude>,

    #[serde(default = "default_tokenizer_type")]
    pub default_tokenizer: String,

    #[serde(default = "default_tokenizer_model")]
    pub default_tokenizer_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_size: default_max_size(),
            max_depth: default_max_depth(),
            default_output_format: default_output_format(),
            default_excludes: default_excludes(),
            default_tokenizer: default_tokenizer_type(),
            default_tokenizer_model: default_tokenizer_model(),
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

fn default_output_format() -> String {
    "both".to_string()
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

fn get_config_path() -> anyhow::Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
        .join("glimpse");
    Ok(config_dir.join("config.toml"))
}

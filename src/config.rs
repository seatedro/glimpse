use std::path::PathBuf;

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
                    "both" | _ => OutputFormat::Both, // Default to Both for unknown values
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

#[derive(Debug, Serialize, Deserialize)]
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_size: default_max_size(),
            max_depth: default_max_depth(),
            default_output_format: default_output_format().into(),
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

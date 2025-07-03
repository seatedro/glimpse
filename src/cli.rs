use crate::config::Config;
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, ValueEnum, Serialize, Deserialize)]
pub enum OutputFormat {
    Tree,
    Files,
    Both,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum TokenizerType {
    Tiktoken,
    #[clap(name = "huggingface")]
    HuggingFace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Exclude {
    File(PathBuf),
    Pattern(String),
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "glimpse",
    about = "A blazingly fast tool for peeking at codebases",
    version
)]
pub struct Cli {
    /// Files or directories to analyze (multiple allowed), or a single URL/git repository
    #[arg(default_value = ".")]
    pub paths: Vec<String>,

    /// Print the config file path and exit
    #[arg(long)]
    pub config_path: bool,

    /// Additional patterns to include (e.g. "*.rs,*.go")
    #[arg(short, long, value_delimiter = ',')]
    pub include: Option<Vec<String>>,

    /// Additional patterns to exclude
    #[arg(short, long, value_parser = parse_exclude, value_delimiter = ',')]
    pub exclude: Option<Vec<Exclude>>,

    /// Maximum file size in bytes
    #[arg(short, long)]
    pub max_size: Option<u64>,

    /// Maximum directory depth
    #[arg(long)]
    pub max_depth: Option<usize>,

    /// Output format (tree, files, or both)
    #[arg(short, long, value_enum)]
    pub output: Option<OutputFormat>,

    /// Output file path (optional)
    #[arg(short = 'f', long, num_args = 0..=1, default_missing_value = "GLIMPSE.md")]
    pub file: Option<PathBuf>,

    /// Init glimpse config file
    #[arg(long, default_value_t = false)]
    pub config: bool,

    /// Print to stdout instead
    #[arg(short, long)]
    pub print: bool,

    /// Number of threads for parallel processing
    #[arg(short, long)]
    pub threads: Option<usize>,

    /// Show hidden files and directories
    #[arg(short = 'H', long)]
    pub hidden: bool,

    /// Don't respect .gitignore files
    #[arg(long)]
    pub no_ignore: bool,

    /// Ignore Token Count
    #[arg(long)]
    pub no_tokens: bool,

    /// Tokenizer to use (tiktoken or huggingface)
    #[arg(long, value_enum)]
    pub tokenizer: Option<TokenizerType>,

    /// Model to use for HuggingFace tokenizer
    #[arg(long)]
    pub model: Option<String>,

    /// Path to local tokenizer file
    #[arg(long)]
    pub tokenizer_file: Option<PathBuf>,

    /// Interactive mode
    #[arg(long)]
    pub interactive: bool,

    /// Output as Pdf
    #[arg(long)]
    pub pdf: Option<PathBuf>,

    /// Traverse sublinks when processing URLs
    #[arg(long)]
    pub traverse_links: bool,

    /// Maximum depth to traverse sublinks (default: 1)
    #[arg(long)]
    pub link_depth: Option<usize>,

    /// Output in XML format for better LLM compatibility
    #[arg(short = 'x', long)]
    pub xml: bool,
}

impl Cli {
    pub fn parse_with_config(config: &Config) -> anyhow::Result<Self> {
        let mut cli = Self::parse();

        // Apply config defaults if CLI args aren't specified
        cli.max_size = cli.max_size.or(Some(config.max_size));
        cli.max_depth = cli.max_depth.or(Some(config.max_depth));
        cli.output = cli.output.or(Some(OutputFormat::from(
            config.default_output_format.clone(),
        )));

        // Merge excludes from config and CLI
        if let Some(mut excludes) = cli.exclude.take() {
            excludes.extend(config.default_excludes.clone());
            cli.exclude = Some(excludes);
        } else {
            cli.exclude = Some(config.default_excludes.clone());
        }

        // Set default tokenizer if none specified but token counting is enabled
        if !cli.no_tokens && cli.tokenizer.is_none() {
            cli.tokenizer = Some(match config.default_tokenizer.as_str() {
                "huggingface" => TokenizerType::HuggingFace,
                _ => TokenizerType::Tiktoken,
            });
        }

        // Set default model for HuggingFace if none specified
        if cli
            .tokenizer
            .as_ref()
            .is_some_and(|t| matches!(t, TokenizerType::HuggingFace))
            && cli.model.is_none()
            && cli.tokenizer_file.is_none()
        {
            cli.model = Some(config.default_tokenizer_model.clone());
        }

        Ok(cli)
    }

    pub fn with_path(&self, path: &str) -> Self {
        let mut new_cli = self.clone();
        new_cli.paths = vec![path.to_string()];
        new_cli
    }

    pub fn validate_args(&self, is_url: bool) -> anyhow::Result<()> {
        if is_url {
            return Ok(());
        }
        if self.paths.is_empty() {
            return Err(anyhow::anyhow!("No paths provided"));
        }
        for input in &self.paths {
            if !input.starts_with("http://") && !input.starts_with("https://") {
                let path = PathBuf::from(input);
                if !path.exists() {
                    return Err(anyhow::anyhow!("Path '{}' does not exist", input));
                }
            }
        }
        Ok(())
    }
}

fn parse_exclude(value: &str) -> Result<Exclude, String> {
    let path = PathBuf::from(value);
    if path.exists() {
        Ok(Exclude::File(path))
    } else {
        Ok(Exclude::Pattern(value.to_string()))
    }
}

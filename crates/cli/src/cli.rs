use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

use glimpse_core::{BackwardsCompatOutputFormat, Config, Exclude, OutputFormat, TokenizerType};

#[derive(Debug, Clone, ValueEnum, Serialize, Deserialize)]
pub enum CliOutputFormat {
    Tree,
    Files,
    Both,
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Tree => OutputFormat::Tree,
            CliOutputFormat::Files => OutputFormat::Files,
            CliOutputFormat::Both => OutputFormat::Both,
        }
    }
}

impl From<OutputFormat> for CliOutputFormat {
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Tree => CliOutputFormat::Tree,
            OutputFormat::Files => CliOutputFormat::Files,
            OutputFormat::Both => CliOutputFormat::Both,
        }
    }
}

impl From<BackwardsCompatOutputFormat> for CliOutputFormat {
    fn from(format: BackwardsCompatOutputFormat) -> Self {
        let output_format: OutputFormat = format.into();
        output_format.into()
    }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CliTokenizerType {
    Tiktoken,
    #[clap(name = "huggingface")]
    HuggingFace,
}

impl From<CliTokenizerType> for TokenizerType {
    fn from(t: CliTokenizerType) -> Self {
        match t {
            CliTokenizerType::Tiktoken => TokenizerType::Tiktoken,
            CliTokenizerType::HuggingFace => TokenizerType::HuggingFace,
        }
    }
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "glimpse",
    about = "A blazingly fast tool for peeking at codebases",
    version
)]
pub struct Cli {
    #[arg(default_value = ".")]
    pub paths: Vec<String>,

    #[arg(long)]
    pub config_path: bool,

    #[arg(short, long, value_delimiter = ',')]
    pub include: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',')]
    pub only_include: Option<Vec<String>>,

    #[arg(short, long, value_parser = parse_exclude, value_delimiter = ',')]
    pub exclude: Option<Vec<Exclude>>,

    #[arg(short, long)]
    pub max_size: Option<u64>,

    #[arg(long)]
    pub max_depth: Option<usize>,

    #[arg(short, long, value_enum)]
    pub output: Option<CliOutputFormat>,

    #[arg(short = 'f', long, num_args = 0..=1, default_missing_value = "GLIMPSE.md")]
    pub file: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    pub config: bool,

    #[arg(short, long)]
    pub print: bool,

    #[arg(short, long)]
    pub threads: Option<usize>,

    #[arg(short = 'H', long)]
    pub hidden: bool,

    #[arg(long)]
    pub no_ignore: bool,

    #[arg(long)]
    pub no_tokens: bool,

    #[arg(long, value_enum)]
    pub tokenizer: Option<CliTokenizerType>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub tokenizer_file: Option<PathBuf>,

    #[arg(long)]
    pub interactive: bool,

    #[arg(long)]
    pub pdf: Option<PathBuf>,

    #[arg(long)]
    pub traverse_links: bool,

    #[arg(long)]
    pub link_depth: Option<usize>,

    #[arg(short = 'x', long)]
    pub xml: bool,
}

impl Cli {
    pub fn parse_with_config(config: &Config) -> anyhow::Result<Self> {
        let mut cli = Self::parse();

        cli.max_size = cli.max_size.or(Some(config.max_size));
        cli.max_depth = cli.max_depth.or(Some(config.max_depth));
        let output_format: OutputFormat = config.default_output_format.clone().into();
        cli.output = cli.output.or(Some(CliOutputFormat::from(output_format)));

        if let Some(mut excludes) = cli.exclude.take() {
            excludes.extend(config.default_excludes.clone());
            cli.exclude = Some(excludes);
        } else {
            cli.exclude = Some(config.default_excludes.clone());
        }

        if !cli.no_tokens && cli.tokenizer.is_none() {
            cli.tokenizer = Some(match config.default_tokenizer.as_str() {
                "huggingface" => CliTokenizerType::HuggingFace,
                _ => CliTokenizerType::Tiktoken,
            });
        }

        if cli
            .tokenizer
            .as_ref()
            .is_some_and(|t| matches!(t, CliTokenizerType::HuggingFace))
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
        if self.include.is_some() && self.only_include.is_some() {
            return Err(anyhow::anyhow!(
                "Cannot use both --include and --only-include flags together. Use --include for additive behavior (add to source files) or --only-include for replacement behavior (only specified patterns)."
            ));
        }

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

    pub fn get_output_format(&self) -> Option<OutputFormat> {
        self.output.clone().map(|f| f.into())
    }

    pub fn get_tokenizer_type(&self) -> Option<TokenizerType> {
        self.tokenizer.clone().map(|t| t.into())
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

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

use glimpse::{Config, Exclude, OutputFormat, TokenizerType};

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

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Generate call graph for a function
    #[command(name = "code")]
    Code(CodeArgs),

    /// Manage the code index
    #[command(name = "index")]
    Index(IndexArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct CodeArgs {
    /// Target function in file:function format (e.g., src/main.rs:main or :main)
    #[arg(required = true)]
    pub target: String,

    /// Project root directory
    #[arg(short, long, default_value = ".")]
    pub root: PathBuf,

    /// Include callers (reverse call graph)
    #[arg(long)]
    pub callers: bool,

    /// Maximum depth to traverse
    #[arg(short, long)]
    pub depth: Option<usize>,

    /// Output file (default: stdout)
    #[arg(short = 'f', long)]
    pub file: Option<PathBuf>,

    /// Strict mode: only resolve calls via imports (no global name matching)
    #[arg(long)]
    pub strict: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub command: IndexCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum IndexCommand {
    /// Build or update the index for a project
    Build {
        /// Project root directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Force rebuild (ignore existing index)
        #[arg(short, long)]
        force: bool,
    },

    /// Clear the index for a project
    Clear {
        /// Project root directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Show index status and stats
    Status {
        /// Project root directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct FunctionTarget {
    pub file: Option<PathBuf>,
    pub function: String,
}

impl FunctionTarget {
    pub fn parse(target: &str) -> anyhow::Result<Self> {
        if let Some((file, func)) = target.rsplit_once(':') {
            if file.is_empty() {
                Ok(Self {
                    file: None,
                    function: func.to_string(),
                })
            } else {
                Ok(Self {
                    file: Some(PathBuf::from(file)),
                    function: func.to_string(),
                })
            }
        } else {
            Ok(Self {
                file: None,
                function: target.to_string(),
            })
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
    #[command(subcommand)]
    pub command: Option<Commands>,

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
        cli.output = cli
            .output
            .or(Some(config.default_output_format.clone().into()));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_target_parse_with_file() {
        let target = FunctionTarget::parse("src/main.rs:main").unwrap();
        assert_eq!(target.file, Some(PathBuf::from("src/main.rs")));
        assert_eq!(target.function, "main");
    }

    #[test]
    fn test_function_target_parse_without_file() {
        let target = FunctionTarget::parse(":main").unwrap();
        assert_eq!(target.file, None);
        assert_eq!(target.function, "main");
    }

    #[test]
    fn test_function_target_parse_function_only() {
        let target = FunctionTarget::parse("main").unwrap();
        assert_eq!(target.file, None);
        assert_eq!(target.function, "main");
    }

    #[test]
    fn test_function_target_parse_nested_path() {
        let target = FunctionTarget::parse("src/code/graph.rs:build").unwrap();
        assert_eq!(target.file, Some(PathBuf::from("src/code/graph.rs")));
        assert_eq!(target.function, "build");
    }

    #[test]
    fn test_function_target_parse_windows_path() {
        let target = FunctionTarget::parse("C:\\src\\main.rs:main").unwrap();
        assert_eq!(target.file, Some(PathBuf::from("C:\\src\\main.rs")));
        assert_eq!(target.function, "main");
    }
}

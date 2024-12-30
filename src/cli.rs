use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "glimpse",
    about = "A blazingly fast tool for peeking at codebases",
    version
)]
pub struct Cli {
    /// Directory to analyze
    #[arg(value_parser = validate_path)]
    pub path: PathBuf,

    /// Additional patterns to include (e.g. "*.rs,*.go")
    #[arg(short, long, value_delimiter = ',')]
    pub include: Option<Vec<String>>,

    /// Additional patterns to exclude
    #[arg(short, long, value_delimiter = ',')]
    pub exclude: Option<Vec<String>>,

    /// Maximum file size in bytes
    #[arg(short, long, default_value = "10485760")] // 10MB
    pub max_size: u64,

    /// Maximum directory depth
    #[arg(long, default_value = "20")]
    pub max_depth: usize,

    /// Output format (tree, files, or both)
    #[arg(short, long, default_value = "both")]
    pub output: String,

    /// Number of threads for parallel processing
    #[arg(short, long)]
    pub threads: Option<usize>,

    /// Show hidden files and directories
    #[arg(short = 'H', long)]
    pub hidden: bool,

    /// Don't respect .gitignore files
    #[arg(long)]
    pub no_ignore: bool,
}

fn validate_path(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);
    if path_buf.exists() {
        Ok(path_buf)
    } else {
        Err(format!("Path '{}' does not exist", path))
    }
}

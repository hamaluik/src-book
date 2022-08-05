use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    #[clap(short, long)]
    /// An optional title for the book
    pub title: Option<String>,

    #[clap(short, long)]
    /// A list of authors for the source code
    pub authors: Vec<String>,

    #[clap(short, long)]
    /// The path to a git repository to take book contents from
    pub git_repository: Option<PathBuf>,

    #[clap(short, long)]
    /// A list of source files to explicitely include
    pub source_files: Vec<PathBuf>,

    #[clap(short, long)]
    /// Include this to prevent interactive prompting and only use command-line provided parameters
    pub no_prompt: bool,
}

use clap::{Parser, Subcommand};

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Generates a src-book.toml config file
    Config,
    /// Renders the book according to the contents of the src-book.toml config file
    Render,
}

#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

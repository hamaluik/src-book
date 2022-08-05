use crate::source::GitRepository;
use anyhow::{anyhow, Context, Result};
use cli::Cli;
use dialoguer::Input;
use dialoguer::{theme::ColorfulTheme, Confirm};
use source::{AuthorBuilder, Source, SourceProvider};
use std::path::{Path, PathBuf};

mod cli;
mod source;

#[derive(Debug, Default)]
struct Configuration {
    source: Source,
}

fn main() -> Result<()> {
    use clap::Parser;
    let mut cli = Cli::parse();

    let theme = ColorfulTheme {
        ..ColorfulTheme::default()
    };

    let mut config: Configuration = Configuration::default();

    if let Some(title) = cli.title.take() {
        config.source.title = Some(title);
    } else if !cli.no_prompt {
        let title = Input::with_theme(&theme)
            .with_prompt("Book title")
            .default("".to_string())
            .allow_empty(true)
            .interact()
            .with_context(|| "Failed to obtain title")?;
        if !title.is_empty() {
            config.source.title = Some(title);
        }
    }
    for (i, author) in cli.authors.into_iter().enumerate() {
        config.source.authors.push(
            AuthorBuilder::default()
                .identifier(author)
                .prominence(usize::MAX - i)
                .build()
                .with_context(|| "Failed to build author")?,
        );
    }
    config.source.source_files = cli.source_files.iter().map(Into::into).collect();

    let repo = if cli.git_repository.is_some() {
        cli.git_repository.take()
    } else if !cli.no_prompt {
        if Confirm::with_theme(&theme)
            .with_prompt("Do you wish to load data from a repository directory?")
            .interact()
            .with_context(|| "Failed to interact")?
        {
            let repo = Input::with_theme(&theme)
                .with_prompt("Repository directory")
                .default(".".to_string())
                .interact()
                .with_context(|| "Failed to obtain repository path")?;
            let repo = PathBuf::from(repo);
            if !repo.exists() || !repo.is_dir() {
                return Err(anyhow!("Path '{}' isn't a directory!", repo.display()));
            }
            Some(repo)
        } else {
            None
        }
    } else {
        None
    };
    if let Some(repo) = repo {
        use globset::{Glob, GlobMatcher};
        let mut block_globs: Vec<GlobMatcher> = Vec::default();

        if !cli.no_prompt {
            if Confirm::with_theme(&theme)
                .with_prompt(
                    "Do you wish to specifically block some files allowed by your .gitignore?",
                )
                .interact()?
            {
                'block: loop {
                    if !block_globs.is_empty() {
                        println!(
                            "Blocked globs: [{}]",
                            block_globs
                                .iter()
                                .map(|gm| gm.glob().glob().to_string())
                                .collect::<Vec<String>>()
                                .join("], [")
                        );
                    }
                    let glob: String = Input::with_theme(&theme)
                        .with_prompt("Glob syntax of files you want to specifically block")
                        .allow_empty(true)
                        .interact()?;
                    if glob.trim().is_empty() {
                        break 'block;
                    }

                    let glob = Glob::new(&glob)
                        .with_context(|| "Failed to parse glob!")?
                        .compile_matcher();
                    block_globs.push(glob);
                }
            }
        }

        let repo = GitRepository::load(&repo, block_globs)
            .with_context(|| format!("Failed to load git repository at {}", repo.display()))?;

        repo.apply(&mut config.source)
            .with_context(|| "Failed to load information from repository")?;
    }

    // TODO: license

    println!("Config: {config:#?}");

    Ok(())
}

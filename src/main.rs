use crate::sinks::{FontFamily, SyntaxTheme};
use crate::source::GitRepository;
use anyhow::{anyhow, Context, Result};
use cli::Cli;
use dialoguer::Input;
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use serde::{Deserialize, Serialize};
use sinks::PDF;
use source::{AuthorBuilder, Source};
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::ExitCode;

mod cli;
mod highlight;
mod sinks {
    mod pdf;
    pub use pdf::*;
}
mod source;

#[derive(Deserialize, Serialize)]
struct Configuration {
    source: Source,
    pdf: Option<PDF>,
}

fn sort_paths(root: Option<PathBuf>, mut a: Vec<&OsStr>, mut b: Vec<&OsStr>) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    let root_a = a.remove(0);
    let root_b = b.remove(0);

    let root_a = match &root {
        Some(root) => root.join(root_a),
        None => PathBuf::from(root_a),
    };
    let root_b = match &root {
        Some(root) => root.join(root_b),
        None => PathBuf::from(root_b),
    };

    match (root_a.is_file(), root_b.is_file()) {
        (true, true) => return root_a.cmp(&root_b),
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        _ => {}
    }

    match root_a.cmp(&root_b) {
        Ordering::Equal => match a.len().cmp(&b.len()) {
            Ordering::Equal => sort_paths(Some(root_a), a, b),
            o => o,
        },
        o => o,
    }
}

fn main() -> ExitCode {
    if let Err(e) = try_main() {
        eprintln!("{}: {e:#}", console::style("Error").red());
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn try_main() -> Result<()> {
    use clap::Parser;
    let cli = Cli::parse();

    match &cli.command {
        cli::Commands::Config => {
            let theme = ColorfulTheme {
                ..ColorfulTheme::default()
            };

            let title = Input::with_theme(&theme)
                .with_prompt("Book title")
                .default("".to_string())
                .allow_empty(false)
                .interact()
                .with_context(|| "Failed to obtain title")?;

            let repo_path = Input::with_theme(&theme)
                .with_prompt("Repository directory")
                .default(".".to_string())
                .interact()
                .with_context(|| "Failed to obtain repository path")?;
            let repo_path = PathBuf::from(repo_path);
            if !repo_path.exists() || !repo_path.is_dir() {
                return Err(anyhow!("Path '{}' isn't a directory!", repo_path.display()));
            }
            use globset::{Glob, GlobMatcher};
            let mut block_globs: Vec<GlobMatcher> = Vec::default();

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

            let repo = GitRepository::load(&repo_path, block_globs).with_context(|| {
                format!("Failed to load git repository at {}", repo_path.display())
            })?;

            let mut authors = repo.authors.clone();

            println!(
                "Authors: [{}]",
                authors
                    .iter()
                    .map(|author| author.to_string())
                    .collect::<Vec<String>>()
                    .join("], [")
            );
            if Confirm::with_theme(&theme)
                .with_prompt("Do you wish to add more authors?")
                .interact()?
            {
                let mut author_i = 0;
                'authors: loop {
                    let author: String = Input::with_theme(&theme)
                        .with_prompt("Additional author (leave blank to move on)")
                        .allow_empty(true)
                        .interact()?;
                    if author.trim().is_empty() {
                        break 'authors;
                    }
                    authors.push(
                        AuthorBuilder::default()
                            .identifier(author)
                            .prominence(usize::MAX - author_i)
                            .build()
                            .with_context(|| "Failed to build author")?,
                    );
                    author_i += 1;
                }
            }
            authors.sort();

            let mut licenses: Vec<String> = Vec::default();
            'licenses: loop {
                if !licenses.is_empty() {
                    println!("Licenses: [{}]", licenses.join("], ["));
                }
                let license: String = Input::with_theme(&theme)
                    .with_prompt("SPDX license of the repository (leave empty for done)")
                    .allow_empty(true)
                    .interact()?;
                if license.trim().is_empty() {
                    break 'licenses;
                }

                licenses.push(license.trim().to_string());
            }

            let mut source_files: Vec<PathBuf> = repo
                .source_files
                .iter()
                .filter(|&f| f != &PathBuf::from("src-book.toml"))
                .map(Clone::clone)
                .collect();
            source_files.sort_by(|a, b| {
                let a: Vec<_> = a.iter().collect();
                let b: Vec<_> = b.iter().collect();
                sort_paths(None, a, b)
            });

            let source = Source {
                title: Some(title),
                authors,
                source_files,
                licenses,
                repository: repo_path,
            };

            let mut pdf = None;
            if Confirm::with_theme(&theme)
                .with_prompt("Do you want to render to PDF?")
                .interact()?
            {
                let outfile: String = Input::with_theme(&theme)
                    .with_prompt("Output pdf file")
                    .allow_empty(false)
                    .interact()?;
                let mut outfile = PathBuf::from(outfile);
                let ext = outfile
                    .extension()
                    .map(std::ffi::OsStr::to_ascii_lowercase)
                    .unwrap_or_default();
                if ext != *"pdf" {
                    outfile.set_extension("pdf");
                }

                let syntax_theme = FuzzySelect::with_theme(&theme)
                    .with_prompt("Syntax highlighting theme")
                    .items(SyntaxTheme::all())
                    .default(0)
                    .interact()?;
                let syntax_theme = SyntaxTheme::all()[syntax_theme];

                let font = FuzzySelect::with_theme(&theme)
                    .with_prompt("Font family")
                    .items(FontFamily::all())
                    .default(0)
                    .interact()?;
                let font = FontFamily::all()[font];

                pdf = Some(PDF {
                    outfile,
                    theme: syntax_theme,
                    font,
                });
            }

            let config = Configuration { source, pdf };

            let config = toml::to_string_pretty(&config)
                .with_context(|| "Failed to convert configuration to TOML")?;

            let config_path = PathBuf::from("src-book.toml");
            if config_path.exists()
                && !Confirm::with_theme(&theme)
                    .with_prompt("src-book.toml already exists, do you want to override it?")
                    .interact()?
            {
                println!("Configuration:");
                println!("{}", config);
            } else {
                std::fs::write("src-book.toml", config)
                    .with_context(|| "Failed to write configuration file")?;
                println!("src-book.toml written!");
            }
        }
        cli::Commands::Render => {
            println!("Loading configuration...");
            let contents = std::fs::read_to_string("src-book.toml")
                .with_context(|| "Failed to load src-book.toml contents")?;
            let config = toml::from_str(&contents).with_context(|| "Failed to parse TOML")?;

            let Configuration { source, pdf } = config;

            if let Some(pdf) = pdf {
                println!("Rendering PDF to {}...", pdf.outfile.display());
                pdf.render(&source)
                    .with_context(|| "Failed to render PDF")?;
            }

            println!("Done!");
        }
    }

    Ok(())
}

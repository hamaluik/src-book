//! Interactive configuration wizard for creating `src-book.toml`.
//!
//! The wizard collects book metadata, repository settings, and PDF output options
//! through a series of prompts. It extracts authors from git commit history and
//! allows manual additions with prominence ranking.

use crate::detection::{detect_defaults, DetectedDefaults};
use crate::file_ordering::{sort_paths, sort_with_entrypoint};
use crate::sinks::{SyntaxTheme, PDF};
use crate::source::{AuthorBuilder, CommitOrder, GitRepository, Source};
use anyhow::{anyhow, Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect, Input};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Complete configuration for a src-book project.
#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub source: Source,
    pub pdf: Option<PDF>,
}

/// Run the interactive configuration wizard.
///
/// Prompts the user for book metadata, repository settings, and PDF output options,
/// then writes `src-book.toml` to the current directory.
pub fn run() -> Result<()> {
    let theme = ColorfulTheme {
        ..ColorfulTheme::default()
    };

    // get repo path first so we can detect defaults
    let repo_path = Input::with_theme(&theme)
        .with_prompt("Repository directory")
        .default(".".to_string())
        .interact()
        .with_context(|| "Failed to obtain repository path")?;
    let repo_path = PathBuf::from(repo_path);
    if !repo_path.exists() || !repo_path.is_dir() {
        return Err(anyhow!("Path '{}' isn't a directory!", repo_path.display()));
    }

    // detect defaults from project conventions
    let DetectedDefaults {
        title: detected_title,
        entrypoint: detected_entrypoint,
        licenses: detected_licenses,
    } = detect_defaults(&repo_path);

    let title = Input::with_theme(&theme)
        .with_prompt("Book title")
        .with_initial_text(detected_title.unwrap_or_default())
        .allow_empty(false)
        .interact()
        .with_context(|| "Failed to obtain title")?;
    use globset::{Glob, GlobMatcher};
    let mut block_globs: Vec<GlobMatcher> = Vec::default();

    if Confirm::with_theme(&theme)
        .with_prompt("Do you wish to specifically block some files allowed by your .gitignore?")
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
        format!(
            "Failed to load git repository at {}",
            repo_path.display()
        )
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

    // pre-populate with detected licenses
    let mut licenses: Vec<String> = detected_licenses;
    'licenses: loop {
        if !licenses.is_empty() {
            println!("Licences: [{}]", licenses.join("], ["));
        }
        let license: String = Input::with_theme(&theme)
            .with_prompt("SPDX licence of the repository (leave empty for done)")
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

    // ask for entrypoint file to control ordering
    // default to yes if we detected an entrypoint
    let entrypoint = if Confirm::with_theme(&theme)
        .with_prompt(
            "Do you want to specify an entrypoint file (e.g., src/main.rs) to control file ordering?",
        )
        .default(detected_entrypoint.is_some())
        .interact()?
    {
        // sort files first so the selection list is in a predictable order
        source_files.sort_by(|a, b| {
            let a: Vec<_> = a.iter().collect();
            let b: Vec<_> = b.iter().collect();
            sort_paths(None, a, b)
        });

        let file_strings: Vec<String> = source_files
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        // pre-select detected entrypoint if it exists in file list
        let default_idx = detected_entrypoint
            .as_ref()
            .and_then(|ep| source_files.iter().position(|f| f == ep))
            .unwrap_or(0);

        let selection = FuzzySelect::with_theme(&theme)
            .with_prompt("Select entrypoint file (files in its directory will be listed first)")
            .items(&file_strings)
            .default(default_idx)
            .interact()?;

        Some(source_files[selection].clone())
    } else {
        None
    };

    // sort files with entrypoint priority
    sort_with_entrypoint(&mut source_files, entrypoint.as_ref());

    // ask about commit history ordering
    let commit_order_options: Vec<String> = CommitOrder::all()
        .iter()
        .map(|o| o.to_string())
        .collect();
    let commit_order_idx = FuzzySelect::with_theme(&theme)
        .with_prompt("Commit history order")
        .items(&commit_order_options)
        .default(0)
        .interact()?;
    let commit_order = CommitOrder::all()[commit_order_idx];

    let source = Source {
        title: Some(title),
        authors,
        source_files,
        licenses,
        repository: repo_path,
        entrypoint,
        commit_order,
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

        let base_font_size: f32 = Input::with_theme(&theme)
            .with_prompt("Base font size in points")
            .default(10.0)
            .interact()?;

        // calculate derived font sizes from base, rounded to integers
        let font_size_title_pt = (base_font_size * 3.2).round();
        let font_size_heading_pt = (base_font_size * 2.4).round();
        let font_size_subheading_pt = (base_font_size * 1.2).round();
        let font_size_body_pt = base_font_size.round();
        let font_size_small_pt = (base_font_size * 0.8).round();

        // ask about booklet generation
        let booklet_outfile = if Confirm::with_theme(&theme)
            .with_prompt("Generate a print-ready booklet PDF for saddle-stitch binding?")
            .default(false)
            .interact()?
        {
            let booklet_path: String = Input::with_theme(&theme)
                .with_prompt("Booklet output file")
                .default(
                    outfile.with_extension("").to_string_lossy().to_string() + "-booklet.pdf",
                )
                .interact()?;
            Some(PathBuf::from(booklet_path))
        } else {
            None
        };

        let (booklet_signature_size, booklet_sheet_width_in, booklet_sheet_height_in) =
            if booklet_outfile.is_some() {
                let sig_size: u32 = Input::with_theme(&theme)
                    .with_prompt("Pages per signature (must be divisible by 4)")
                    .default(16)
                    .validate_with(|input: &u32| {
                        if *input % 4 == 0 && *input > 0 {
                            Ok(())
                        } else {
                            Err("Signature size must be a positive multiple of 4")
                        }
                    })
                    .interact()?;

                let sheet_width: f32 = Input::with_theme(&theme)
                    .with_prompt(
                        "Physical sheet width in inches (e.g., 11.0 for US Letter landscape)",
                    )
                    .default(11.0)
                    .interact()?;

                let sheet_height: f32 = Input::with_theme(&theme)
                    .with_prompt(
                        "Physical sheet height in inches (e.g., 8.5 for US Letter landscape)",
                    )
                    .default(8.5)
                    .interact()?;

                (sig_size, sheet_width, sheet_height)
            } else {
                (16, 11.0, 8.5)
            };

        pdf = Some(PDF {
            outfile,
            theme: syntax_theme,
            font_size_title_pt,
            font_size_heading_pt,
            font_size_subheading_pt,
            font_size_body_pt,
            font_size_small_pt,
            booklet_outfile,
            booklet_signature_size,
            booklet_sheet_width_in,
            booklet_sheet_height_in,
            ..PDF::default()
        });
    }

    let config = Configuration { source, pdf };

    let config =
        toml::to_string_pretty(&config).with_context(|| "Failed to convert configuration to TOML")?;

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

    Ok(())
}

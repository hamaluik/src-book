use crate::sinks::SyntaxTheme;
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
    pub use pdf::{SyntaxTheme, PDF};
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

/// Sort files with entrypoint-aware ordering to create a logical reading flow.
///
/// When reading source code as a book, starting from the main entry point (e.g., `main.rs`)
/// and then seeing related files in the same directory creates a natural progression.
/// This mirrors how developers typically explore unfamiliar codebases.
///
/// Ordering priority:
/// 1. Entrypoint file first (the logical starting point)
/// 2. Other files in the entrypoint's directory (immediate context)
/// 3. Subdirectories of the entrypoint's directory (related modules)
/// 4. Everything else (sorted alphabetically)
fn sort_with_entrypoint(files: &mut [PathBuf], entrypoint: Option<&PathBuf>) {
    // First, do the standard sort
    files.sort_by(|a, b| {
        let a: Vec<_> = a.iter().collect();
        let b: Vec<_> = b.iter().collect();
        sort_paths(None, a, b)
    });

    // If no entrypoint, we're done
    let entrypoint = match entrypoint {
        Some(e) => e,
        None => return,
    };

    // Get the entrypoint's parent directory
    let entrypoint_dir = entrypoint.parent();

    // Sort with entrypoint priority
    files.sort_by(|a, b| {
        let a_is_entrypoint = a == entrypoint;
        let b_is_entrypoint = b == entrypoint;

        // Entrypoint always comes first
        if a_is_entrypoint && !b_is_entrypoint {
            return Ordering::Less;
        }
        if b_is_entrypoint && !a_is_entrypoint {
            return Ordering::Greater;
        }

        // Check if files are in the entrypoint's directory or its subdirectories
        let a_in_entrypoint_dir = entrypoint_dir
            .map(|dir| a.starts_with(dir))
            .unwrap_or(false);
        let b_in_entrypoint_dir = entrypoint_dir
            .map(|dir| b.starts_with(dir))
            .unwrap_or(false);

        // Files in entrypoint directory come before files outside it
        match (a_in_entrypoint_dir, b_in_entrypoint_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                // Both in or both out of entrypoint dir - use standard sort
                let a: Vec<_> = a.iter().collect();
                let b: Vec<_> = b.iter().collect();
                sort_paths(None, a, b)
            }
        }
    });
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

            // Ask for entrypoint file to control ordering
            let entrypoint = if Confirm::with_theme(&theme)
                .with_prompt("Do you want to specify an entrypoint file (e.g., src/main.rs) to control file ordering?")
                .interact()?
            {
                // Sort files first so the selection list is in a predictable order
                source_files.sort_by(|a, b| {
                    let a: Vec<_> = a.iter().collect();
                    let b: Vec<_> = b.iter().collect();
                    sort_paths(None, a, b)
                });

                let file_strings: Vec<String> = source_files
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();

                let selection = FuzzySelect::with_theme(&theme)
                    .with_prompt("Select entrypoint file (files in its directory will be listed first)")
                    .items(&file_strings)
                    .default(0)
                    .interact()?;

                Some(source_files[selection].clone())
            } else {
                None
            };

            // Sort files with entrypoint priority
            sort_with_entrypoint(&mut source_files, entrypoint.as_ref());

            let source = Source {
                title: Some(title),
                authors,
                source_files,
                licenses,
                repository: repo_path,
                entrypoint,
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

                // Calculate derived font sizes from base, rounded to integers
                let font_size_title_pt = (base_font_size * 3.2).round();
                let font_size_heading_pt = (base_font_size * 2.4).round();
                let font_size_subheading_pt = (base_font_size * 1.2).round();
                let font_size_body_pt = base_font_size.round();
                let font_size_small_pt = (base_font_size * 0.8).round();

                // Ask about booklet generation
                let booklet_outfile = if Confirm::with_theme(&theme)
                    .with_prompt("Generate a print-ready booklet PDF for saddle-stitch binding?")
                    .default(false)
                    .interact()?
                {
                    let booklet_path: String = Input::with_theme(&theme)
                        .with_prompt("Booklet output file")
                        .default(
                            outfile.with_extension("").to_string_lossy().to_string()
                                + "-booklet.pdf",
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
                            .with_prompt("Physical sheet width in inches (e.g., 11.0 for US Letter landscape)")
                            .default(11.0)
                            .interact()?;

                        let sheet_height: f32 = Input::with_theme(&theme)
                            .with_prompt("Physical sheet height in inches (e.g., 8.5 for US Letter landscape)")
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
            let config: Configuration =
                toml::from_str(&contents).with_context(|| "Failed to parse TOML")?;

            let Configuration { source, pdf } = config;

            if let Some(pdf) = pdf {
                println!("Rendering PDF to {}...", pdf.outfile.display());
                let stats = pdf
                    .render(&source)
                    .with_context(|| "Failed to render PDF")?;

                println!("Done!\n");
                println!("  Main PDF:    {}", pdf.outfile.display());

                if let (Some(booklet_path), Some(sheets)) =
                    (&pdf.booklet_outfile, stats.booklet_sheets)
                {
                    println!("  Booklet PDF: {}\n", booklet_path.display());

                    let booklet_pages = stats.page_count / 2 + stats.page_count % 2;
                    let sheets_per_sig = pdf.booklet_signature_size / 4;
                    let booklet_pages_per_sig = pdf.booklet_signature_size / 2;

                    println!("Booklet info:");
                    println!("  Original pages:   {}", stats.page_count);
                    println!(
                        "  Booklet pages:    {} (2 original pages per booklet page)",
                        booklet_pages
                    );
                    println!(
                        "  Sheets needed:    {} (4 original pages per sheet)",
                        sheets
                    );
                    println!(
                        "  Signature size:   {} original pages ({} sheets per signature)\n",
                        pdf.booklet_signature_size, sheets_per_sig
                    );

                    println!("To print the booklet:");
                    println!("  1. Print double-sided, flip on short edge");
                    println!(
                        "  2. Print {} booklet pages at a time (one {}-page signature = {} sheets)",
                        booklet_pages_per_sig, pdf.booklet_signature_size, sheets_per_sig
                    );
                    println!(
                        "  3. For each signature: nest the {} sheets together and fold in half",
                        sheets_per_sig
                    );
                    println!("  4. Stack all signatures and sew/staple along the spine");
                }
            } else {
                println!("No PDF output configured.");
            }
        }
    }

    Ok(())
}

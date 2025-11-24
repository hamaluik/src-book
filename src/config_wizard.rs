//! Configuration wizard for creating `src-book.toml`.
//!
//! The wizard collects book metadata, repository settings, frontmatter file selection,
//! and PDF output options. It extracts authors from git commit history and allows manual
//! additions with prominence ranking.
//!
//! ## Modes
//!
//! - **Interactive (default)**: Prompts the user through a series of dialoguer prompts
//! - **Non-interactive (`--yes`)**: Uses auto-detected values and sensible defaults
//! - **Template-based (`--config-from`)**: Loads PDF settings from existing config file (implies `--yes`)
//!
//! ## Theme Preview
//!
//! In interactive mode, selecting a syntax highlighting theme displays a live preview
//! of the theme rendered in the terminal using 24-bit ANSI colours. The preview shows
//! a short Rust code snippet on a white background (simulating paper) so users can see
//! how their code will appear before confirming their choice. After viewing the preview,
//! users can confirm or go back to select a different theme.
//!
//! Theme preview requires a terminal with true colour (24-bit) support. Most modern
//! terminals support this, including macOS Terminal, iTerm2, Windows Terminal, and
//! common Linux terminal emulators.
//!
//! ## Non-Interactive Mode
//!
//! Useful for CI pipelines and scripting. Auto-detection from [`crate::detection`] provides:
//! - Title from directory name (title-cased)
//! - Entrypoint from common conventions (`src/main.rs`, `src/lib.rs`, etc.)
//! - Licences from manifest files or LICENSE text
//! - Frontmatter from root-level documentation files
//!
//! When `--config-from` is used, the template's PDF settings (theme, page size, margins)
//! are preserved while the repository is re-scanned for current files and authors.
//!
//! ## Caveats
//!
//! - Non-interactive mode always overwrites existing `src-book.toml` without prompting
//! - Optional features (booklet, binary hex) are disabled in non-interactive mode unless
//!   a template with those features enabled is provided via `--config-from`
//! - Block globs require interactive mode or `--config-from` to specify
//! - Theme preview is skipped in non-interactive mode

use crate::cli::ConfigArgs;
use crate::detection::{detect_defaults, detect_frontmatter, DetectedDefaults};
use crate::file_ordering::{sort_paths, sort_with_entrypoint};
use crate::sinks::{
    BinaryHexConfig, BookletConfig, ColophonConfig, FontSizesConfig, FooterConfig, HeaderConfig,
    MarginsConfig, MetadataConfig, NumberingConfig, PageConfig, PageSize, Position, RulePosition,
    SyntaxTheme, TitlePageConfig, TitlePageImagePosition, PDF,
};
use crate::source::{AuthorBuilder, CommitOrder, GitRepository, Source};
use anyhow::{anyhow, Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect, Input, MultiSelect, Select};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// Complete configuration for a src-book project.
#[derive(Deserialize, Serialize)]
pub struct Configuration {
    pub source: Source,
    pub pdf: Option<PDF>,
}

/// Load a template configuration from an existing `src-book.toml` file.
///
/// Used by `--config-from` to apply a "golden" config's PDF settings to a new repository.
/// The template's source file lists are ignored; only PDF settings are preserved.
fn load_template(path: &PathBuf) -> Result<Configuration> {
    let contents =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
}

/// Attempt to load an existing `src-book.toml` from the current directory.
///
/// Returns `None` if the file doesn't exist or fails to parse (silent failure).
/// Used to pre-populate defaults when editing an existing configuration.
fn load_existing_config() -> Option<Configuration> {
    let path = std::path::Path::new("src-book.toml");
    if !path.exists() {
        return None;
    }

    let contents = std::fs::read_to_string(path).ok()?;
    let mut config: Configuration = toml::from_str(&contents).ok()?;

    // apply legacy field migrations if present
    if let Some(ref mut pdf) = config.pdf {
        pdf.apply_legacy_fields();
    }

    Some(config)
}

/// Print a syntax-highlighted preview of the given theme to the terminal.
///
/// Uses 24-bit ANSI colour codes for true colour display. The preview shows a short
/// Rust snippet demonstrating keywords, strings, comments, and function calls.
/// Background is set to white to simulate appearance on paper, rendered as a
/// full rectangle with padding.
fn print_theme_preview(theme: SyntaxTheme, ss: &SyntaxSet, ts: &ThemeSet) {
    let sample = r#"fn main() {
    let message = "Hello, world!";
    println!("{}", message); // output
}"#;

    let syntax = ss
        .find_syntax_by_extension("rs")
        .expect("can find rust syntax");
    let theme = &ts.themes[theme.name()];

    // ANSI escape for white background (24-bit colour)
    const WHITE_BG: &str = "\x1b[48;2;255;255;255m";
    const RESET: &str = "\x1b[0m";
    const PADDING: usize = 2;

    // calculate the width needed for the rectangle
    let max_line_len = sample.lines().map(|l| l.len()).max().unwrap_or(0);
    let box_width = max_line_len + PADDING * 2;

    println!();

    // top padding row
    println!("{WHITE_BG}{:box_width$}{RESET}", "");

    let mut h = HighlightLines::new(syntax, theme);
    for line in sample.lines() {
        // add newline for syntect's highlighter state tracking
        let line_with_newline = format!("{}\n", line);
        let ranges = h
            .highlight_line(&line_with_newline, ss)
            .expect("can highlight line");

        // build highlighted string without the trailing newline
        let mut escaped = String::new();
        for (style, text) in ranges {
            let text = text.trim_end_matches('\n');
            if !text.is_empty() {
                let fg = style.foreground;
                escaped.push_str(&format!("\x1b[38;2;{};{};{}m{}", fg.r, fg.g, fg.b, text));
            }
        }

        // calculate right padding to fill the rectangle
        let right_pad = max_line_len - line.len() + PADDING;

        // left padding + highlighted content + right padding (all on white bg)
        println!("{WHITE_BG}{:PADDING$}{escaped}{:right_pad$}{RESET}", "", "");
    }

    // bottom padding row
    println!("{WHITE_BG}{:box_width$}{RESET}", "");
}

/// Run the configuration wizard.
///
/// # Arguments
///
/// * `args` - CLI arguments controlling wizard behaviour:
///   - `yes`: Skip all prompts, use detected defaults
///   - `config_from`: Load PDF settings from existing config file (implies `yes`)
///   - `output`: Override PDF output path
///
/// # Non-Interactive Behaviour
///
/// When `args.yes` is true or `args.config_from` is provided, the wizard:
/// - Uses detected title, entrypoint, licences, frontmatter, and authors
/// - Applies sensible defaults for all PDF settings
/// - Overwrites existing `src-book.toml` without confirmation
///
/// Priority for PDF settings: `--output` flag > template > detected defaults
pub fn run(args: &ConfigArgs) -> Result<()> {
    let non_interactive = args.yes || args.config_from.is_some();
    let template = args
        .config_from
        .as_ref()
        .map(load_template)
        .transpose()?;

    // load existing config to use as defaults (ignored when --config-from is used)
    let existing = if template.is_none() {
        load_existing_config()
    } else {
        None
    };

    let theme = ColorfulTheme {
        ..ColorfulTheme::default()
    };

    // get repo path first so we can detect defaults
    let repo_path = if non_interactive {
        PathBuf::from(".")
    } else {
        let path: String = Input::with_theme(&theme)
            .with_prompt("Repository directory")
            .default(".".to_string())
            .interact()
            .with_context(|| "Failed to obtain repository path")?;
        PathBuf::from(path)
    };
    if !repo_path.exists() || !repo_path.is_dir() {
        return Err(anyhow!("Path '{}' isn't a directory!", repo_path.display()));
    }

    // detect defaults from project conventions
    let DetectedDefaults {
        title: detected_title,
        entrypoint: detected_entrypoint,
        licenses: detected_licenses,
    } = detect_defaults(&repo_path);

    let title: String = if non_interactive {
        // prefer template title, then detected, then directory name
        template
            .as_ref()
            .and_then(|t| t.source.title.clone())
            .or(detected_title)
            .unwrap_or_else(|| {
                repo_path
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "Untitled".to_string())
            })
    } else {
        // prefer existing title, then detected, then empty
        let default_title = existing
            .as_ref()
            .and_then(|e| e.source.title.clone())
            .or(detected_title)
            .unwrap_or_default();
        Input::with_theme(&theme)
            .with_prompt("Book title")
            .default(default_title)
            .allow_empty(false)
            .interact()
            .with_context(|| "Failed to obtain title")?
    };
    use globset::{Glob, GlobMatcher};
    let mut block_globs: Vec<GlobMatcher> = Vec::default();

    // in non-interactive mode, use template's block globs if available
    if non_interactive {
        if let Some(ref t) = template {
            for glob_str in &t.source.block_globs {
                let glob = Glob::new(glob_str)
                    .with_context(|| format!("failed to parse glob: {}", glob_str))?
                    .compile_matcher();
                block_globs.push(glob);
            }
        }
    } else {
        // pre-populate with existing globs
        if let Some(ref e) = existing {
            for glob_str in &e.source.block_globs {
                if let Ok(glob) = Glob::new(glob_str) {
                    block_globs.push(glob.compile_matcher());
                }
            }
        }

        // ask about blocking more files (default yes if there are existing globs)
        if Confirm::with_theme(&theme)
            .with_prompt(if block_globs.is_empty() {
                "Do you wish to specifically block some files allowed by your .gitignore?"
            } else {
                "Edit blocked file patterns?"
            })
            .default(!block_globs.is_empty())
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

    // check for submodules and prompt if any exist
    let submodule_paths = GitRepository::submodule_paths(&repo_path);
    let exclude_submodules = if non_interactive {
        // use template setting if available, otherwise default to true
        template
            .as_ref()
            .map(|t| t.source.exclude_submodules)
            .unwrap_or(true)
    } else if !submodule_paths.is_empty() {
        println!(
            "Detected {} git submodule(s): {}",
            submodule_paths.len(),
            submodule_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        // use existing setting as default, otherwise true
        let default_exclude = existing
            .as_ref()
            .map(|e| e.source.exclude_submodules)
            .unwrap_or(true);
        Confirm::with_theme(&theme)
            .with_prompt("Exclude git submodules from the book?")
            .default(default_exclude)
            .interact()?
    } else {
        true // default to true even if no submodules, for consistency
    };

    let repo = GitRepository::load(&repo_path, block_globs.clone(), exclude_submodules)
        .with_context(|| format!("Failed to load git repository at {}", repo_path.display()))?;

    let mut authors = repo.authors.clone();

    // in non-interactive mode, skip adding extra authors
    if !non_interactive {
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
    }
    authors.sort();

    // pre-populate with detected licences (or template licences in non-interactive mode)
    let licences: Vec<String> = if non_interactive {
        // prefer template licences if available, otherwise use detected
        template
            .as_ref()
            .map(|t| t.source.licences.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or(detected_licenses)
    } else {
        // prefer existing licences, then detected
        let mut licences = existing
            .as_ref()
            .map(|e| e.source.licences.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or(detected_licenses);
        'licences: loop {
            if !licences.is_empty() {
                println!("Licences: [{}]", licences.join("], ["));
            }
            let licence: String = Input::with_theme(&theme)
                .with_prompt("SPDX licence of the repository (leave empty for done)")
                .allow_empty(true)
                .interact()?;
            if licence.trim().is_empty() {
                break 'licences;
            }

            licences.push(licence.trim().to_string());
        }
        licences
    };

    let mut source_files: Vec<PathBuf> = repo
        .source_files
        .iter()
        .filter(|&f| f != &PathBuf::from("src-book.toml"))
        .map(Clone::clone)
        .collect();

    // detect and select frontmatter files
    let detected_frontmatter = detect_frontmatter(&source_files);
    let frontmatter_files = if non_interactive {
        // in non-interactive mode, select all detected frontmatter files
        let selected = detected_frontmatter.clone();
        source_files.retain(|f| !selected.contains(f));
        selected
    } else if !detected_frontmatter.is_empty() {
        let frontmatter_strings: Vec<String> = detected_frontmatter
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        println!(
            "Detected {} potential frontmatter file(s): {}",
            detected_frontmatter.len(),
            frontmatter_strings.join(", ")
        );

        // pre-select files that were in existing config, otherwise default to true
        let existing_frontmatter = existing
            .as_ref()
            .map(|e| &e.source.frontmatter_files);
        let defaults: Vec<bool> = detected_frontmatter
            .iter()
            .map(|f| {
                existing_frontmatter
                    .map(|ef| ef.contains(f))
                    .unwrap_or(true)
            })
            .collect();
        let selections = MultiSelect::with_theme(&theme)
            .with_prompt("Select files for the frontmatter section (before source code)")
            .items(&frontmatter_strings)
            .defaults(&defaults)
            .interact()?;

        let selected: Vec<PathBuf> = selections
            .into_iter()
            .map(|i| detected_frontmatter[i].clone())
            .collect();

        // remove selected frontmatter from source files
        source_files.retain(|f| !selected.contains(f));

        selected
    } else {
        Vec::new()
    };

    // ask for entrypoint file to control ordering
    // in non-interactive mode, use detected entrypoint if available
    let existing_entrypoint = existing
        .as_ref()
        .map(|e| &e.source.entrypoint)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);

    let entrypoint = if non_interactive {
        detected_entrypoint
    } else if Confirm::with_theme(&theme)
        .with_prompt(
            "Do you want to specify an entrypoint file (e.g., src/main.rs) to control file ordering?",
        )
        .default(existing_entrypoint.is_some() || detected_entrypoint.is_some())
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

        // prefer existing entrypoint, then detected, then first file
        let default_idx = existing_entrypoint
            .as_ref()
            .and_then(|ep| source_files.iter().position(|f| f == ep))
            .or_else(|| {
                detected_entrypoint
                    .as_ref()
                    .and_then(|ep| source_files.iter().position(|f| f == ep))
            })
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
    let commit_order = if non_interactive {
        // use template setting if available, otherwise default to NewestFirst
        template
            .as_ref()
            .map(|t| t.source.commit_order)
            .unwrap_or(CommitOrder::NewestFirst)
    } else {
        let commit_order_options: Vec<String> =
            CommitOrder::all().iter().map(|o| o.to_string()).collect();
        // pre-select existing commit order if available
        let default_idx = existing
            .as_ref()
            .and_then(|e| {
                CommitOrder::all()
                    .iter()
                    .position(|&o| o == e.source.commit_order)
            })
            .unwrap_or(0);
        let commit_order_idx = FuzzySelect::with_theme(&theme)
            .with_prompt("Commit history order")
            .items(&commit_order_options)
            .default(default_idx)
            .interact()?;
        CommitOrder::all()[commit_order_idx]
    };

    // convert GlobMatchers to strings for serialisation
    let block_glob_strings: Vec<String> = block_globs
        .iter()
        .map(|gm| gm.glob().glob().to_string())
        .collect();

    let source = Source {
        title: Some(title),
        authors,
        frontmatter_files,
        source_files,
        licences,
        repository: repo_path,
        block_globs: block_glob_strings,
        exclude_submodules,
        entrypoint: entrypoint
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        commit_order,
        ..Default::default()
    };

    // in non-interactive mode, always enable PDF output
    // use --output flag, template, or default to "book.pdf"
    let existing_pdf = existing.as_ref().and_then(|e| e.pdf.as_ref());
    let should_render_pdf = if non_interactive {
        true
    } else {
        Confirm::with_theme(&theme)
            .with_prompt("Do you want to render to PDF?")
            .default(existing_pdf.is_some())
            .interact()?
    };

    let mut pdf = None;
    if should_render_pdf {
        let outfile = if non_interactive {
            // priority: --output flag > template > default
            args.output
                .clone()
                .or_else(|| template.as_ref().and_then(|t| t.pdf.as_ref()).map(|p| p.outfile.clone()))
                .unwrap_or_else(|| PathBuf::from("book.pdf"))
        } else {
            // use existing outfile as default
            let default_outfile = existing_pdf
                .map(|p| p.outfile.display().to_string())
                .unwrap_or_default();
            let outfile_str: String = Input::with_theme(&theme)
                .with_prompt("Output pdf file")
                .default(default_outfile)
                .allow_empty(false)
                .interact()?;
            let mut outfile = PathBuf::from(outfile_str);
            let ext = outfile
                .extension()
                .map(std::ffi::OsStr::to_ascii_lowercase)
                .unwrap_or_default();
            if ext != *"pdf" {
                outfile.set_extension("pdf");
            }
            outfile
        };

        let syntax_theme = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.theme)
                .unwrap_or(SyntaxTheme::all()[0])
        } else {
            // load syntax and theme sets for preview
            let (ss, _): (SyntaxSet, _) = bincode::serde::decode_from_slice(
                crate::highlight::SERIALIZED_SYNTAX,
                bincode::config::standard(),
            )
            .expect("can deserialize syntaxes");
            let (ts, _): (ThemeSet, _) = bincode::serde::decode_from_slice(
                crate::highlight::SERIALIZED_THEMES,
                bincode::config::standard(),
            )
            .expect("can deserialize themes");

            // preview-then-confirm loop
            // pre-select existing theme if available
            let mut default_idx = existing_pdf
                .and_then(|p| SyntaxTheme::all().iter().position(|&t| t == p.theme))
                .unwrap_or(0);
            loop {
                let idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Syntax highlighting theme")
                    .items(SyntaxTheme::all())
                    .default(default_idx)
                    .interact()?;
                let selected = SyntaxTheme::all()[idx];

                print_theme_preview(selected, &ss, &ts);

                if Confirm::with_theme(&theme)
                    .with_prompt(format!("Use {}?", selected))
                    .default(true)
                    .interact()?
                {
                    break selected;
                }
                default_idx = idx;
            }
        };

        let (page_width_in, page_height_in) = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| (p.page.width_in, p.page.height_in))
                .unwrap_or((5.5, 8.5)) // Half Letter default
        } else {
            // find matching page size preset for existing dimensions
            let default_page_size_idx = existing_pdf
                .and_then(|p| {
                    PageSize::all().iter().position(|ps| {
                        ps.dimensions_in()
                            .map(|(w, h)| (w - p.page.width_in).abs() < 0.01 && (h - p.page.height_in).abs() < 0.01)
                            .unwrap_or(false)
                    })
                })
                .or_else(|| {
                    // if no preset matches but we have existing dimensions, select Custom
                    existing_pdf.and_then(|_| {
                        PageSize::all()
                            .iter()
                            .position(|ps| ps.dimensions_in().is_none())
                    })
                })
                .unwrap_or(0);

            let page_size_idx = FuzzySelect::with_theme(&theme)
                .with_prompt("Page size")
                .items(PageSize::all())
                .default(default_page_size_idx)
                .interact()?;
            let page_size = PageSize::all()[page_size_idx];

            if let Some(dims) = page_size.dimensions_in() {
                dims
            } else {
                // custom dimensions - use existing values as defaults
                let default_width = existing_pdf.map(|p| p.page.width_in).unwrap_or(5.5);
                let default_height = existing_pdf.map(|p| p.page.height_in).unwrap_or(8.5);
                let width: f32 = Input::with_theme(&theme)
                    .with_prompt("Page width in inches")
                    .default(default_width)
                    .interact()?;
                let height: f32 = Input::with_theme(&theme)
                    .with_prompt("Page height in inches")
                    .default(default_height)
                    .interact()?;
                (width, height)
            }
        };

        let base_font_size: f32 = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.fonts.body_pt)
                .unwrap_or(8.0)
        } else {
            let default_font_size = existing_pdf.map(|p| p.fonts.body_pt).unwrap_or(8.0);
            Input::with_theme(&theme)
                .with_prompt("Base font size in points")
                .default(default_font_size)
                .interact()?
        };

        // calculate derived font sizes from base, rounded to integers
        let font_size_title_pt = (base_font_size * 3.2).round();
        let font_size_heading_pt = (base_font_size * 2.4).round();
        let font_size_subheading_pt = (base_font_size * 1.2).round();
        let font_size_body_pt = base_font_size.round();
        let font_size_small_pt = (base_font_size * 0.8).round();

        // ask about booklet generation
        // in non-interactive mode, skip booklet unless template has it configured
        let existing_booklet_enabled = existing_pdf
            .map(|p| !p.booklet.outfile.is_empty())
            .unwrap_or(false);
        let booklet_outfile: String = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.booklet.outfile.clone())
                .unwrap_or_default()
        } else if Confirm::with_theme(&theme)
            .with_prompt("Generate a print-ready booklet PDF for saddle-stitch binding?")
            .default(existing_booklet_enabled)
            .interact()?
        {
            let default_booklet_path = existing_pdf
                .map(|p| p.booklet.outfile.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| outfile.with_extension("").to_string_lossy().to_string() + "-booklet.pdf");
            let booklet_path: String = Input::with_theme(&theme)
                .with_prompt("Booklet output file")
                .default(default_booklet_path)
                .interact()?;
            booklet_path
        } else {
            String::new()
        };

        let (booklet_signature_size, booklet_sheet_width_in, booklet_sheet_height_in) =
            if non_interactive {
                // use template settings if available
                template
                    .as_ref()
                    .and_then(|t| t.pdf.as_ref())
                    .map(|p| (p.booklet.signature_size, p.booklet.sheet_width_in, p.booklet.sheet_height_in))
                    .unwrap_or((16, 11.0, 8.5))
            } else if !booklet_outfile.is_empty() {
                let default_sig_size = existing_pdf.map(|p| p.booklet.signature_size).unwrap_or(16);
                let sig_size: u32 = Input::with_theme(&theme)
                    .with_prompt("Pages per signature (must be divisible by 4)")
                    .default(default_sig_size)
                    .validate_with(|input: &u32| {
                        if *input % 4 == 0 && *input > 0 {
                            Ok(())
                        } else {
                            Err("Signature size must be a positive multiple of 4")
                        }
                    })
                    .interact()?;

                let default_sheet_width = existing_pdf.map(|p| p.booklet.sheet_width_in).unwrap_or(11.0);
                let sheet_width: f32 = Input::with_theme(&theme)
                    .with_prompt(
                        "Physical sheet width in inches (e.g., 11.0 for US Letter landscape)",
                    )
                    .default(default_sheet_width)
                    .interact()?;

                let default_sheet_height = existing_pdf.map(|p| p.booklet.sheet_height_in).unwrap_or(8.5);
                let sheet_height: f32 = Input::with_theme(&theme)
                    .with_prompt(
                        "Physical sheet height in inches (e.g., 8.5 for US Letter landscape)",
                    )
                    .default(default_sheet_height)
                    .interact()?;

                (sig_size, sheet_width, sheet_height)
            } else {
                (16, 11.0, 8.5)
            };

        // ask about binary hex rendering
        // in non-interactive mode, skip hex rendering unless template has it enabled
        let existing_hex_enabled = existing_pdf.map(|p| p.binary_hex.enabled).unwrap_or(false);
        let render_binary_hex = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.binary_hex.enabled)
                .unwrap_or(false)
        } else {
            let enabled = Confirm::with_theme(&theme)
                .with_prompt("Render binary files as hex dumps instead of placeholders?")
                .default(existing_hex_enabled)
                .interact()?;

            if enabled {
                println!(
                    "Warning: Rendering binary files as hex will drastically increase book size and rendering time."
                );
            }
            enabled
        };

        let (binary_hex_max_bytes, font_size_hex_pt) = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| (p.binary_hex.max_bytes, p.binary_hex.font_size_pt))
                .unwrap_or((Some(65536), 5.0))
        } else if render_binary_hex {
            // convert existing max_bytes to KB for the prompt default
            let default_max_kb = existing_pdf
                .and_then(|p| p.binary_hex.max_bytes)
                .map(|b| (b / 1024) as u32)
                .unwrap_or(64);
            let max_kb: u32 = Input::with_theme(&theme)
                .with_prompt("Maximum KB to include from binary files (0 for unlimited)")
                .default(default_max_kb)
                .interact()?;
            let max_bytes = if max_kb == 0 {
                None
            } else {
                Some(max_kb as usize * 1024)
            };

            let default_hex_font = existing_pdf.map(|p| p.binary_hex.font_size_pt).unwrap_or(5.0);
            let hex_font_size: f32 = Input::with_theme(&theme)
                .with_prompt("Font size for hex dump in points")
                .default(default_hex_font)
                .interact()?;

            (max_bytes, hex_font_size)
        } else {
            (Some(65536), 5.0)
        };

        // header/footer customisation
        // in non-interactive mode, use template settings or defaults
        let (
            header_template,
            header_position,
            header_rule,
            footer_template,
            footer_position,
            footer_rule,
        ) = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| (
                    p.header.template.clone(),
                    p.header.position,
                    p.header.rule,
                    p.footer.template.clone(),
                    p.footer.position,
                    p.footer.rule,
                ))
                .unwrap_or_else(|| (
                    "{file}".to_string(),
                    Position::Outer,
                    RulePosition::Below,
                    "{n}".to_string(),
                    Position::Outer,
                    RulePosition::None,
                ))
        } else if Confirm::with_theme(&theme)
            .with_prompt("Customise headers and footers?")
            .default(existing_pdf.is_some())
            .interact()?
        {
            println!("Templates support placeholders: {{file}}, {{title}}, {{n}} (page number), {{total}}");

            let default_header_template = existing_pdf
                .map(|p| p.header.template.clone())
                .unwrap_or_else(|| "{file}".to_string());
            let header_template: String = Input::with_theme(&theme)
                .with_prompt("Header template (empty to disable)")
                .default(default_header_template)
                .allow_empty(true)
                .interact()?;

            let header_position = if !header_template.is_empty() {
                let default_pos_idx = existing_pdf
                    .and_then(|p| Position::all().iter().position(|&pos| pos == p.header.position))
                    .unwrap_or(0);
                let pos_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Header position")
                    .items(Position::all())
                    .default(default_pos_idx)
                    .interact()?;
                Position::all()[pos_idx]
            } else {
                Position::Outer
            };

            let header_rule = if !header_template.is_empty() {
                let default_rule_idx = existing_pdf
                    .and_then(|p| RulePosition::all().iter().position(|&r| r == p.header.rule))
                    .unwrap_or(2); // default to Below
                let rule_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Header rule (horizontal line)")
                    .items(RulePosition::all())
                    .default(default_rule_idx)
                    .interact()?;
                RulePosition::all()[rule_idx]
            } else {
                RulePosition::None
            };

            let default_footer_template = existing_pdf
                .map(|p| p.footer.template.clone())
                .unwrap_or_else(|| "{n}".to_string());
            let footer_template: String = Input::with_theme(&theme)
                .with_prompt("Footer template (empty to disable)")
                .default(default_footer_template)
                .allow_empty(true)
                .interact()?;

            let footer_position = if !footer_template.is_empty() {
                let default_pos_idx = existing_pdf
                    .and_then(|p| Position::all().iter().position(|&pos| pos == p.footer.position))
                    .unwrap_or(0);
                let pos_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Footer position")
                    .items(Position::all())
                    .default(default_pos_idx)
                    .interact()?;
                Position::all()[pos_idx]
            } else {
                Position::Outer
            };

            let footer_rule = if !footer_template.is_empty() {
                let default_rule_idx = existing_pdf
                    .and_then(|p| RulePosition::all().iter().position(|&r| r == p.footer.rule))
                    .unwrap_or(0); // default to None
                let rule_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Footer rule (horizontal line)")
                    .items(RulePosition::all())
                    .default(default_rule_idx)
                    .interact()?;
                RulePosition::all()[rule_idx]
            } else {
                RulePosition::None
            };

            // section numbering uses sensible defaults:
            // - frontmatter: Roman numerals (i, ii, iii...)
            // - source: Arabic numerals (1, 2, 3...)
            // - appendix: Arabic numerals (1, 2, 3...)

            (
                header_template,
                header_position,
                header_rule,
                footer_template,
                footer_position,
                footer_rule,
            )
        } else {
            // defaults
            (
                "{file}".to_string(),
                Position::Outer,
                RulePosition::Below,
                "{n}".to_string(),
                Position::Outer,
                RulePosition::None,
            )
        };

        // colophon/statistics page customisation
        // in non-interactive mode, enable colophon with default template (or use template's setting)
        let existing_colophon_enabled = existing_pdf
            .map(|p| !p.colophon.template.is_empty())
            .unwrap_or(true);
        let colophon_template = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.colophon.template.clone())
                .unwrap_or_else(crate::sinks::default_colophon_template)
        } else if Confirm::with_theme(&theme)
            .with_prompt("Include a colophon/statistics page after the title page?")
            .default(existing_colophon_enabled)
            .interact()?
        {
            // check if existing template differs from default
            let existing_template = existing_pdf.map(|p| p.colophon.template.clone());
            let has_custom_template = existing_template
                .as_ref()
                .map(|t| !t.is_empty() && *t != crate::sinks::default_colophon_template())
                .unwrap_or(false);

            if Confirm::with_theme(&theme)
                .with_prompt("Customise the colophon template?")
                .default(has_custom_template)
                .interact()?
            {
                println!("Available placeholders:");
                println!("  {{title}}         - Book title");
                println!("  {{authors}}       - Author list");
                println!("  {{licences}}      - Licence identifiers");
                println!("  {{remotes}}       - Git remotes (name: url)");
                println!("  {{generated_date}} - Current date");
                println!("  {{tool_version}}  - src-book version");
                println!("  {{file_count}}    - Number of source files");
                println!("  {{line_count}}    - Total lines of code");
                println!("  {{total_bytes}}   - Total file size");
                println!("  {{commit_count}}  - Number of commits");
                println!("  {{date_range}}    - First to last commit date");
                println!("  {{language_stats}} - File/line counts by extension");
                println!("  {{commit_chart}}  - Commit activity histogram");

                let default_template = existing_template
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(crate::sinks::default_colophon_template);
                println!("\nDefault template:");
                println!("---");
                println!("{}", default_template);
                println!("---");

                let custom: String = Input::with_theme(&theme)
                    .with_prompt("Enter custom template (or press Enter for default)")
                    .default(default_template)
                    .allow_empty(true)
                    .interact()?;

                custom
            } else {
                // use existing template if available, otherwise default
                existing_template
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(crate::sinks::default_colophon_template)
            }
        } else {
            // disabled
            String::new()
        };

        // Title page customisation: template with placeholders, optional image.
        // In interactive mode, users can customise layout and add logos/cover art.
        // In non-interactive mode, use template settings or sensible defaults.
        // The dimension calculation helps users understand monospace block limits.
        let (title_page_template, title_page_image, title_page_image_position, title_page_image_max_height_in) =
            if non_interactive {
                let template_pdf = template.as_ref().and_then(|t| t.pdf.as_ref());
                (
                    template_pdf
                        .map(|p| p.title_page.template.clone())
                        .unwrap_or_else(crate::sinks::default_title_page_template),
                    template_pdf.map(|p| p.title_page.image.clone()).unwrap_or_default(),
                    template_pdf
                        .map(|p| p.title_page.image_position)
                        .unwrap_or_default(),
                    template_pdf
                        .map(|p| p.title_page.image_max_height_in)
                        .unwrap_or(2.0),
                )
            } else {
                // check if existing config has customisations
                let existing_title_page = existing_pdf.map(|p| &p.title_page);
                let has_custom_title_page = existing_title_page
                    .map(|tp| {
                        tp.template != crate::sinks::default_title_page_template()
                            || !tp.image.is_empty()
                    })
                    .unwrap_or(false);

                if Confirm::with_theme(&theme)
                    .with_prompt("Customise title page layout?")
                    .default(has_custom_title_page)
                    .interact()?
                {
                    println!("Available placeholders:");
                    println!("  {{title}}    - Book title (rendered in title font)");
                    println!("  {{authors}}  - Author list (one per line)");
                    println!("  {{licences}} - Licence identifiers");
                    println!("  {{date}}     - Current date (YYYY-MM-DD)");
                    println!();
                    println!("Use ``` fences for monospace blocks (ASCII art, sample output):");
                    println!("  ```");
                    println!("  Your monospace text here");
                    println!("  ```");
                    println!();
                    // calculate approximate max dimensions for monospace blocks
                    // monospace character width is ~0.6 of font size
                    let char_width_pt = font_size_body_pt * 0.6;
                    let page_width_pt = page_width_in * 72.0;
                    let max_chars = (page_width_pt / char_width_pt).floor() as usize;
                    // height: estimate usable space after title/authors (rough approximation)
                    let page_height_pt = page_height_in * 72.0;
                    let title_space_pt = font_size_title_pt * 1.2 * 2.0; // title + spacing
                    let author_estimate_pt = font_size_body_pt * 1.2 * 4.0; // ~4 lines for authors
                    let line_height_pt = font_size_body_pt * 1.2;
                    let usable_height_pt = page_height_pt - title_space_pt - author_estimate_pt - 72.0; // 1" margin
                    let max_lines = (usable_height_pt / line_height_pt).floor() as usize;
                    println!(
                        "Monospace block limits: ~{} chars wide, ~{} lines tall (approximate)",
                        max_chars, max_lines
                    );

                    let default_template = existing_title_page
                        .map(|tp| tp.template.clone())
                        .unwrap_or_else(crate::sinks::default_title_page_template);
                    println!("\nDefault template:");
                    println!("---");
                    println!("{}", default_template);
                    println!("---");

                    let custom_template: String = Input::with_theme(&theme)
                        .with_prompt("Enter custom template (or press Enter for default)")
                        .default(default_template)
                        .allow_empty(true)
                        .interact()?;

                    // image configuration
                    let existing_has_image = existing_title_page
                        .map(|tp| !tp.image.is_empty())
                        .unwrap_or(false);
                    let (image_path, image_position, image_max_height) = if Confirm::with_theme(&theme)
                        .with_prompt("Add an image to the title page?")
                        .default(existing_has_image)
                        .interact()?
                    {
                        let default_image_path = existing_title_page
                            .map(|tp| tp.image.clone())
                            .unwrap_or_default();
                        let path: String = Input::with_theme(&theme)
                            .with_prompt("Image path (relative or absolute)")
                            .default(default_image_path)
                            .interact()?;

                        let positions = TitlePageImagePosition::all();
                        let default_pos_idx = existing_title_page
                            .and_then(|tp| positions.iter().position(|&p| p == tp.image_position))
                            .unwrap_or(0);
                        let position_idx = Select::with_theme(&theme)
                            .with_prompt("Image position")
                            .items(positions)
                            .default(default_pos_idx)
                            .interact()?;
                        let position = positions[position_idx];

                        let default_max_height = existing_title_page
                            .map(|tp| tp.image_max_height_in)
                            .unwrap_or(2.0);
                        let max_height: f32 = Input::with_theme(&theme)
                            .with_prompt("Maximum image height (inches)")
                            .default(default_max_height)
                            .interact()?;

                        (path, position, max_height)
                    } else {
                        (String::new(), TitlePageImagePosition::default(), 2.0)
                    };

                    (custom_template, image_path, image_position, image_max_height)
                } else {
                    // use existing values if available, otherwise defaults
                    (
                        existing_title_page
                            .map(|tp| tp.template.clone())
                            .unwrap_or_else(crate::sinks::default_title_page_template),
                        existing_title_page
                            .map(|tp| tp.image.clone())
                            .unwrap_or_default(),
                        existing_title_page
                            .map(|tp| tp.image_position)
                            .unwrap_or_default(),
                        existing_title_page
                            .map(|tp| tp.image_max_height_in)
                            .unwrap_or(2.0),
                    )
                }
            };

        // PDF document metadata (subject and keywords) for the document info dictionary.
        // these appear in PDF viewers under "Properties" and can help with organisation.
        // in non-interactive mode, use template settings if available; otherwise empty.
        let existing_has_metadata = existing_pdf
            .map(|p| !p.metadata.subject.is_empty() || !p.metadata.keywords.is_empty())
            .unwrap_or(false);
        let (subject, keywords) = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| (p.metadata.subject.clone(), p.metadata.keywords.clone()))
                .unwrap_or_default()
        } else if Confirm::with_theme(&theme)
            .with_prompt("Add PDF metadata (subject/keywords for document properties)?")
            .default(existing_has_metadata)
            .interact()?
        {
            let default_subject = existing_pdf
                .map(|p| p.metadata.subject.clone())
                .unwrap_or_default();
            let subject: String = Input::with_theme(&theme)
                .with_prompt("Document subject/description (empty to skip)")
                .default(default_subject)
                .allow_empty(true)
                .interact()?;

            let default_keywords = existing_pdf
                .map(|p| p.metadata.keywords.clone())
                .unwrap_or_default();
            let keywords: String = Input::with_theme(&theme)
                .with_prompt("Keywords (comma-separated, empty to skip)")
                .default(default_keywords)
                .allow_empty(true)
                .interact()?;

            (subject, keywords)
        } else {
            // preserve existing metadata if not customising
            (
                existing_pdf.map(|p| p.metadata.subject.clone()).unwrap_or_default(),
                existing_pdf.map(|p| p.metadata.keywords.clone()).unwrap_or_default(),
            )
        };

        pdf = Some(PDF {
            outfile,
            font: "SourceCodePro".to_string(),
            theme: syntax_theme,
            page: PageConfig {
                width_in: page_width_in,
                height_in: page_height_in,
            },
            margins: MarginsConfig::default(),
            fonts: FontSizesConfig {
                title_pt: font_size_title_pt,
                heading_pt: font_size_heading_pt,
                subheading_pt: font_size_subheading_pt,
                body_pt: font_size_body_pt,
                small_pt: font_size_small_pt,
            },
            header: HeaderConfig {
                template: header_template,
                position: header_position,
                rule: header_rule,
            },
            footer: FooterConfig {
                template: footer_template,
                position: footer_position,
                rule: footer_rule,
            },
            title_page: TitlePageConfig {
                template: title_page_template,
                image: title_page_image,
                image_position: title_page_image_position,
                image_max_height_in: title_page_image_max_height_in,
            },
            colophon: ColophonConfig {
                template: colophon_template,
            },
            metadata: MetadataConfig { subject, keywords },
            booklet: BookletConfig {
                outfile: booklet_outfile,
                signature_size: booklet_signature_size,
                sheet_width_in: booklet_sheet_width_in,
                sheet_height_in: booklet_sheet_height_in,
            },
            binary_hex: BinaryHexConfig {
                enabled: render_binary_hex,
                max_bytes: binary_hex_max_bytes,
                font_size_pt: font_size_hex_pt,
            },
            numbering: NumberingConfig::default(),
            ..Default::default()
        });
    }

    let config = Configuration { source, pdf };

    let config_str = toml::to_string_pretty(&config)
        .with_context(|| "Failed to convert configuration to TOML")?;

    let config_path = PathBuf::from("src-book.toml");

    // in non-interactive mode, always overwrite
    let should_write = if non_interactive {
        true
    } else if config_path.exists() {
        Confirm::with_theme(&theme)
            .with_prompt("src-book.toml already exists, do you want to override it?")
            .interact()?
    } else {
        true
    };

    if should_write {
        std::fs::write("src-book.toml", &config_str)
            .with_context(|| "Failed to write configuration file")?;
        if non_interactive {
            println!("src-book.toml written (non-interactive mode)");
        } else {
            println!("src-book.toml written!");
        }
    } else {
        println!("Configuration:");
        println!("{}", config_str);
    }

    Ok(())
}

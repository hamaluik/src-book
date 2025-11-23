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

use crate::cli::ConfigArgs;
use crate::detection::{detect_defaults, detect_frontmatter, DetectedDefaults};
use crate::file_ordering::{sort_paths, sort_with_entrypoint};
use crate::sinks::{PageSize, Position, RulePosition, SyntaxTheme, PDF};
use crate::source::{AuthorBuilder, CommitOrder, GitRepository, Source};
use anyhow::{anyhow, Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect, Input, MultiSelect};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        Input::with_theme(&theme)
            .with_prompt("Book title")
            .default(detected_title.unwrap_or_default())
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
    } else if Confirm::with_theme(&theme)
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
        Confirm::with_theme(&theme)
            .with_prompt("Exclude git submodules from the book?")
            .default(true)
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

    // pre-populate with detected licenses (or template licenses in non-interactive mode)
    let licenses: Vec<String> = if non_interactive {
        // prefer template licences if available, otherwise use detected
        template
            .as_ref()
            .map(|t| t.source.licenses.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or(detected_licenses)
    } else {
        let mut licenses = detected_licenses;
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
        licenses
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

        let defaults: Vec<bool> = detected_frontmatter.iter().map(|_| true).collect();
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
    let entrypoint = if non_interactive {
        detected_entrypoint
    } else if Confirm::with_theme(&theme)
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
    let commit_order = if non_interactive {
        // use template setting if available, otherwise default to NewestFirst
        template
            .as_ref()
            .map(|t| t.source.commit_order)
            .unwrap_or(CommitOrder::NewestFirst)
    } else {
        let commit_order_options: Vec<String> =
            CommitOrder::all().iter().map(|o| o.to_string()).collect();
        let commit_order_idx = FuzzySelect::with_theme(&theme)
            .with_prompt("Commit history order")
            .items(&commit_order_options)
            .default(0)
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
        licenses,
        repository: repo_path,
        block_globs: block_glob_strings,
        exclude_submodules,
        entrypoint,
        commit_order,
    };

    // in non-interactive mode, always enable PDF output
    // use --output flag, template, or default to "book.pdf"
    let should_render_pdf = if non_interactive {
        true
    } else {
        Confirm::with_theme(&theme)
            .with_prompt("Do you want to render to PDF?")
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
            let outfile_str: String = Input::with_theme(&theme)
                .with_prompt("Output pdf file")
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
            let idx = FuzzySelect::with_theme(&theme)
                .with_prompt("Syntax highlighting theme")
                .items(SyntaxTheme::all())
                .default(0)
                .interact()?;
            SyntaxTheme::all()[idx]
        };

        let (page_width_in, page_height_in) = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| (p.page_width_in, p.page_height_in))
                .unwrap_or((5.5, 8.5)) // Half Letter default
        } else {
            let page_size_idx = FuzzySelect::with_theme(&theme)
                .with_prompt("Page size")
                .items(PageSize::all())
                .default(0)
                .interact()?;
            let page_size = PageSize::all()[page_size_idx];

            if let Some(dims) = page_size.dimensions_in() {
                dims
            } else {
                // custom dimensions
                let width: f32 = Input::with_theme(&theme)
                    .with_prompt("Page width in inches")
                    .default(5.5)
                    .interact()?;
                let height: f32 = Input::with_theme(&theme)
                    .with_prompt("Page height in inches")
                    .default(8.5)
                    .interact()?;
                (width, height)
            }
        };

        let base_font_size: f32 = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.font_size_body_pt)
                .unwrap_or(8.0)
        } else {
            Input::with_theme(&theme)
                .with_prompt("Base font size in points")
                .default(8.0)
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
        let booklet_outfile = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .and_then(|p| p.booklet_outfile.clone())
        } else if Confirm::with_theme(&theme)
            .with_prompt("Generate a print-ready booklet PDF for saddle-stitch binding?")
            .default(false)
            .interact()?
        {
            let booklet_path: String = Input::with_theme(&theme)
                .with_prompt("Booklet output file")
                .default(outfile.with_extension("").to_string_lossy().to_string() + "-booklet.pdf")
                .interact()?;
            Some(PathBuf::from(booklet_path))
        } else {
            None
        };

        let (booklet_signature_size, booklet_sheet_width_in, booklet_sheet_height_in) =
            if non_interactive {
                // use template settings if available
                template
                    .as_ref()
                    .and_then(|t| t.pdf.as_ref())
                    .map(|p| (p.booklet_signature_size, p.booklet_sheet_width_in, p.booklet_sheet_height_in))
                    .unwrap_or((16, 11.0, 8.5))
            } else if booklet_outfile.is_some() {
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

        // ask about binary hex rendering
        // in non-interactive mode, skip hex rendering unless template has it enabled
        let render_binary_hex = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.render_binary_hex)
                .unwrap_or(false)
        } else {
            let enabled = Confirm::with_theme(&theme)
                .with_prompt("Render binary files as hex dumps instead of placeholders?")
                .default(false)
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
                .map(|p| (p.binary_hex_max_bytes, p.font_size_hex_pt))
                .unwrap_or((Some(65536), 5.0))
        } else if render_binary_hex {
            let max_kb: u32 = Input::with_theme(&theme)
                .with_prompt("Maximum KB to include from binary files (0 for unlimited)")
                .default(64)
                .interact()?;
            let max_bytes = if max_kb == 0 {
                None
            } else {
                Some(max_kb as usize * 1024)
            };

            let hex_font_size: f32 = Input::with_theme(&theme)
                .with_prompt("Font size for hex dump in points")
                .default(5.0)
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
                    p.header_template.clone(),
                    p.header_position,
                    p.header_rule,
                    p.footer_template.clone(),
                    p.footer_position,
                    p.footer_rule,
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
            .default(false)
            .interact()?
        {
            println!("Templates support placeholders: {{file}}, {{title}}, {{n}} (page number), {{total}}");

            let header_template: String = Input::with_theme(&theme)
                .with_prompt("Header template (empty to disable)")
                .default("{file}".to_string())
                .allow_empty(true)
                .interact()?;

            let header_position = if !header_template.is_empty() {
                let pos_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Header position")
                    .items(Position::all())
                    .default(0)
                    .interact()?;
                Position::all()[pos_idx]
            } else {
                Position::Outer
            };

            let header_rule = if !header_template.is_empty() {
                let rule_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Header rule (horizontal line)")
                    .items(RulePosition::all())
                    .default(2) // default to Below
                    .interact()?;
                RulePosition::all()[rule_idx]
            } else {
                RulePosition::None
            };

            let footer_template: String = Input::with_theme(&theme)
                .with_prompt("Footer template (empty to disable)")
                .default("{n}".to_string())
                .allow_empty(true)
                .interact()?;

            let footer_position = if !footer_template.is_empty() {
                let pos_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Footer position")
                    .items(Position::all())
                    .default(0)
                    .interact()?;
                Position::all()[pos_idx]
            } else {
                Position::Outer
            };

            let footer_rule = if !footer_template.is_empty() {
                let rule_idx = FuzzySelect::with_theme(&theme)
                    .with_prompt("Footer rule (horizontal line)")
                    .items(RulePosition::all())
                    .default(0) // default to None
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
        let colophon_template = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| p.colophon_template.clone())
                .unwrap_or_else(crate::sinks::default_colophon_template)
        } else if Confirm::with_theme(&theme)
            .with_prompt("Include a colophon/statistics page after the title page?")
            .default(true)
            .interact()?
        {
            if Confirm::with_theme(&theme)
                .with_prompt("Customise the colophon template?")
                .default(false)
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

                let default_template = crate::sinks::default_colophon_template();
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
                // use default template
                crate::sinks::default_colophon_template()
            }
        } else {
            // disabled
            String::new()
        };

        // PDF document metadata (subject and keywords) for the document info dictionary.
        // these appear in PDF viewers under "Properties" and can help with organisation.
        // in non-interactive mode, use template settings if available; otherwise omit.
        let (subject, keywords) = if non_interactive {
            template
                .as_ref()
                .and_then(|t| t.pdf.as_ref())
                .map(|p| (p.subject.clone(), p.keywords.clone()))
                .unwrap_or((None, None))
        } else if Confirm::with_theme(&theme)
            .with_prompt("Add PDF metadata (subject/keywords for document properties)?")
            .default(false)
            .interact()?
        {
            let subject_input: String = Input::with_theme(&theme)
                .with_prompt("Document subject/description (empty to skip)")
                .allow_empty(true)
                .interact()?;
            let subject = if subject_input.trim().is_empty() {
                None
            } else {
                Some(subject_input)
            };

            let keywords_input: String = Input::with_theme(&theme)
                .with_prompt("Keywords (comma-separated, empty to skip)")
                .allow_empty(true)
                .interact()?;
            let keywords = if keywords_input.trim().is_empty() {
                None
            } else {
                Some(keywords_input)
            };

            (subject, keywords)
        } else {
            (None, None)
        };

        pdf = Some(PDF {
            outfile,
            theme: syntax_theme,
            page_width_in,
            page_height_in,
            font_size_title_pt,
            font_size_heading_pt,
            font_size_subheading_pt,
            font_size_body_pt,
            font_size_small_pt,
            booklet_outfile,
            booklet_signature_size,
            booklet_sheet_width_in,
            booklet_sheet_height_in,
            render_binary_hex,
            binary_hex_max_bytes,
            font_size_hex_pt,
            header_template,
            header_position,
            header_rule,
            footer_template,
            footer_position,
            footer_rule,
            colophon_template,
            subject,
            keywords,
            ..PDF::default()
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

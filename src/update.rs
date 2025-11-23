//! Update command for refreshing file lists in an existing config.
//!
//! When files are added or removed from a repository, the `src-book.toml` config
//! becomes stale. Rather than re-running the full config wizard (which would require
//! re-entering all settings), this command re-scans the repository while preserving
//! existing configuration like PDF settings, title, and licenses.
//!
//! The command:
//! - Re-scans using stored `block_globs` and `exclude_submodules` settings
//! - Refreshes the author list from git commit history
//! - Keeps existing frontmatter files that still exist
//! - Prompts user to select newly detected frontmatter candidates
//! - Handles missing entrypoints interactively

use crate::config_wizard::Configuration;
use crate::detection::detect_frontmatter;
use crate::file_ordering::sort_with_entrypoint;
use crate::source::GitRepository;
use anyhow::{Context, Result};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, FuzzySelect, MultiSelect};
use globset::Glob;
use std::collections::HashSet;
use std::path::PathBuf;

/// Run the update command.
///
/// Loads the existing `src-book.toml`, re-scans the repository for file changes,
/// refreshes authors from git, and prompts interactively when new frontmatter
/// is detected or the entrypoint is missing.
pub fn run() -> Result<()> {
    let theme = ColorfulTheme::default();

    // load existing config
    let contents = std::fs::read_to_string("src-book.toml")
        .with_context(|| "Failed to load src-book.toml - run 'src-book config' first")?;
    let mut config: Configuration =
        toml::from_str(&contents).with_context(|| "Failed to parse src-book.toml")?;

    let source = &mut config.source;

    // build glob matchers from stored patterns
    let block_globs = source
        .block_globs
        .iter()
        .map(|pattern| {
            Glob::new(pattern)
                .with_context(|| format!("Invalid glob pattern: {}", pattern))
                .map(|g| g.compile_matcher())
        })
        .collect::<Result<Vec<_>>>()?;

    // re-scan the repository
    println!("Scanning repository...");
    let repo = GitRepository::load(&source.repository, block_globs, source.exclude_submodules)
        .with_context(|| {
            format!(
                "Failed to load git repository at {}",
                source.repository.display()
            )
        })?;

    // refresh authors from git
    source.authors = repo.authors;
    source.authors.sort();
    println!(
        "  Found {} author(s)",
        source.authors.len()
    );

    // get all discovered files (excluding src-book.toml itself)
    let mut discovered_files: Vec<PathBuf> = repo
        .source_files
        .into_iter()
        .filter(|f| f != &PathBuf::from("src-book.toml"))
        .collect();

    let discovered_set: HashSet<_> = discovered_files.iter().cloned().collect();

    // track existing frontmatter that still exists
    let existing_frontmatter: Vec<PathBuf> = source
        .frontmatter_files
        .iter()
        .filter(|f| discovered_set.contains(*f))
        .cloned()
        .collect();

    let removed_frontmatter: Vec<_> = source
        .frontmatter_files
        .iter()
        .filter(|f| !discovered_set.contains(*f))
        .collect();
    if !removed_frontmatter.is_empty() {
        println!(
            "  Removed {} frontmatter file(s): {}",
            removed_frontmatter.len(),
            removed_frontmatter
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // detect new frontmatter candidates from files not already in frontmatter
    let existing_frontmatter_set: HashSet<_> = source.frontmatter_files.iter().cloned().collect();
    let new_candidates: Vec<PathBuf> = detect_frontmatter(&discovered_files)
        .into_iter()
        .filter(|f| !existing_frontmatter_set.contains(f))
        .collect();

    // prompt for new frontmatter if any detected
    let new_frontmatter = if !new_candidates.is_empty() {
        let candidate_strings: Vec<String> = new_candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect();

        println!(
            "\nDetected {} new potential frontmatter file(s): {}",
            new_candidates.len(),
            candidate_strings.join(", ")
        );

        let defaults: Vec<bool> = new_candidates.iter().map(|_| true).collect();
        let selections = MultiSelect::with_theme(&theme)
            .with_prompt("Select new frontmatter files to add")
            .items(&candidate_strings)
            .defaults(&defaults)
            .interact()?;

        selections
            .into_iter()
            .map(|i| new_candidates[i].clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    // combine existing and new frontmatter
    let mut frontmatter_files = existing_frontmatter;
    frontmatter_files.extend(new_frontmatter);

    // remove frontmatter from source file candidates
    let frontmatter_set: HashSet<_> = frontmatter_files.iter().cloned().collect();
    discovered_files.retain(|f| !frontmatter_set.contains(f));

    // handle entrypoint
    let entrypoint = if let Some(ref ep) = source.entrypoint {
        if discovered_files.contains(ep) {
            // entrypoint still exists
            Some(ep.clone())
        } else {
            // entrypoint was removed
            println!(
                "\nEntrypoint '{}' no longer exists in the repository.",
                ep.display()
            );
            if Confirm::with_theme(&theme)
                .with_prompt("Do you want to select a new entrypoint?")
                .default(true)
                .interact()?
            {
                select_entrypoint(&theme, &discovered_files)?
            } else {
                None
            }
        }
    } else {
        // no existing entrypoint, keep it that way
        None
    };

    // sort files with entrypoint priority
    sort_with_entrypoint(&mut discovered_files, entrypoint.as_ref());

    // calculate change counts before reassigning
    let old_source_set: HashSet<_> = source.source_files.iter().cloned().collect();
    let new_source_set: HashSet<_> = discovered_files.iter().cloned().collect();

    let added_count = discovered_files
        .iter()
        .filter(|f| !old_source_set.contains(*f))
        .count();
    let removed_count = source
        .source_files
        .iter()
        .filter(|f| !new_source_set.contains(*f))
        .count();

    // update source fields
    let frontmatter_count = frontmatter_files.len();
    let source_count = discovered_files.len();
    source.frontmatter_files = frontmatter_files;
    source.source_files = discovered_files;
    source.entrypoint = entrypoint;
    let author_count = source.authors.len();

    // end mutable borrow before serialising
    let _ = source;

    // write back
    let config_str = toml::to_string_pretty(&config)
        .with_context(|| "Failed to serialise configuration to TOML")?;
    std::fs::write("src-book.toml", config_str)
        .with_context(|| "Failed to write src-book.toml")?;

    // report changes
    println!("\nUpdated src-book.toml:");
    println!(
        "  Source files: {} (+{} added, -{} removed)",
        source_count, added_count, removed_count
    );
    println!("  Frontmatter:  {} file(s)", frontmatter_count);
    println!("  Authors:      {} author(s)", author_count);

    Ok(())
}

/// Prompt user to select an entrypoint from the file list.
fn select_entrypoint(theme: &ColorfulTheme, files: &[PathBuf]) -> Result<Option<PathBuf>> {
    let file_strings: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();

    let selection = FuzzySelect::with_theme(theme)
        .with_prompt("Select entrypoint file")
        .items(&file_strings)
        .interact()?;

    Ok(Some(files[selection].clone()))
}

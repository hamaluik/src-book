//! Entrypoint-aware file ordering for logical reading flow.
//!
//! When reading source code as a book, the order of files matters. Starting from the
//! main entry point (e.g., `main.rs` or `lib.rs`) and then seeing related files in
//! the same directory creates a natural progression that mirrors how developers
//! typically explore unfamiliar codebases.
//!
//! This module provides two sorting strategies:
//! - `sort_paths`: basic files-before-directories ordering at each level
//! - `sort_with_entrypoint`: prioritises the entrypoint file, its siblings, then subdirectories

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::path::PathBuf;

/// Sort file paths with files-before-directories ordering within each level.
///
/// This provides a natural reading order where files at each directory level
/// appear before subdirectories.
pub fn sort_paths(root: Option<PathBuf>, mut a: Vec<&OsStr>, mut b: Vec<&OsStr>) -> Ordering {
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
pub fn sort_with_entrypoint(files: &mut [PathBuf], entrypoint: Option<&PathBuf>) {
    // first, do the standard sort
    files.sort_by(|a, b| {
        let a: Vec<_> = a.iter().collect();
        let b: Vec<_> = b.iter().collect();
        sort_paths(None, a, b)
    });

    // if no entrypoint, we're done
    let entrypoint = match entrypoint {
        Some(e) => e,
        None => return,
    };

    // get the entrypoint's parent directory
    let entrypoint_dir = entrypoint.parent();

    // sort with entrypoint priority
    files.sort_by(|a, b| {
        let a_is_entrypoint = a == entrypoint;
        let b_is_entrypoint = b == entrypoint;

        // entrypoint always comes first
        if a_is_entrypoint && !b_is_entrypoint {
            return Ordering::Less;
        }
        if b_is_entrypoint && !a_is_entrypoint {
            return Ordering::Greater;
        }

        // check if files are in the entrypoint's directory or its subdirectories
        let a_in_entrypoint_dir = entrypoint_dir
            .map(|dir| a.starts_with(dir))
            .unwrap_or(false);
        let b_in_entrypoint_dir = entrypoint_dir
            .map(|dir| b.starts_with(dir))
            .unwrap_or(false);

        // files in entrypoint directory come before files outside it
        match (a_in_entrypoint_dir, b_in_entrypoint_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => {
                // both in or both out of entrypoint dir - use standard sort
                let a: Vec<_> = a.iter().collect();
                let b: Vec<_> = b.iter().collect();
                sort_paths(None, a, b)
            }
        }
    });
}

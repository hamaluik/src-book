//! Auto-detection of project defaults for the config wizard.
//!
//! Probes a repository to suggest sensible defaults for title, entrypoint,
//! and licenses based on common project conventions.

use std::path::{Path, PathBuf};

/// Detected default values for a project.
#[derive(Debug, Default)]
pub struct DetectedDefaults {
    pub title: Option<String>,
    pub entrypoint: Option<PathBuf>,
    pub licenses: Vec<String>,
}

/// Detect sensible defaults from a repository path.
pub fn detect_defaults(repo_path: &Path) -> DetectedDefaults {
    DetectedDefaults {
        title: detect_title(repo_path),
        entrypoint: detect_entrypoint(repo_path),
        licenses: detect_licenses(repo_path),
    }
}

/// Detect frontmatter files from a list of repository files.
///
/// Frontmatter files are documentation and metadata files that should appear
/// before source code in the book. Returns files in a sensible reading order:
/// README first, then other docs, then manifest files, then LICENSE last.
pub fn detect_frontmatter(files: &[PathBuf]) -> Vec<PathBuf> {
    // ordered by reading priority (README first, LICENSE last)
    let patterns: &[&[&str]] = &[
        // readme variants - first thing readers should see
        &["README.md", "README", "README.txt", "README.rst"],
        // architecture/design docs
        &["ARCHITECTURE.md", "ARCHITECTURE", "DESIGN.md", "DESIGN"],
        // contribution guidelines
        &["CONTRIBUTING.md", "CONTRIBUTING"],
        // changelog
        &["CHANGELOG.md", "CHANGELOG", "HISTORY.md", "HISTORY"],
        // code of conduct
        &["CODE_OF_CONDUCT.md", "CODE_OF_CONDUCT"],
        // security policy
        &["SECURITY.md", "SECURITY"],
        // manifest files - project metadata
        &["Cargo.toml"],
        &["package.json"],
        &["pyproject.toml", "setup.py"],
        &["go.mod"],
        &["Makefile"],
        // licence files - last because they're standard boilerplate
        &["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "LICENCE.md", "COPYING"],
    ];

    let mut frontmatter = Vec::new();

    for group in patterns {
        for pattern in *group {
            // match root-level files only (no path separators)
            if let Some(file) = files.iter().find(|f| {
                f.to_str()
                    .map(|s| s.eq_ignore_ascii_case(pattern))
                    .unwrap_or(false)
            }) {
                if !frontmatter.contains(file) {
                    frontmatter.push(file.clone());
                }
                break; // only one file per group
            }
        }
    }

    frontmatter
}

/// Detect title from directory name.
///
/// Transforms the directory name into a readable title by replacing
/// hyphens and underscores with spaces and applying title case.
fn detect_title(repo_path: &Path) -> Option<String> {
    let canonical = repo_path.canonicalize().ok()?;
    let dir_name = canonical.file_name()?.to_str()?;

    // replace separators with spaces and title-case
    let title = dir_name
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(title_case_word)
        .collect::<Vec<_>>()
        .join(" ");

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

/// Detect entrypoint based on language conventions.
///
/// Checks for common entrypoint files in order of precedence.
fn detect_entrypoint(repo_path: &Path) -> Option<PathBuf> {
    // ordered by specificity/commonality
    let candidates = [
        // rust
        "src/main.rs",
        "src/lib.rs",
        // python
        "__main__.py",
        "main.py",
        "src/__main__.py",
        // node/typescript
        "src/index.ts",
        "src/index.js",
        "index.ts",
        "index.js",
        // go
        "main.go",
        "cmd/main.go",
    ];

    for candidate in candidates {
        let path = repo_path.join(candidate);
        if path.exists() && path.is_file() {
            return Some(PathBuf::from(candidate));
        }
    }

    None
}

/// Detect licenses from project files.
///
/// Checks manifest files (Cargo.toml, package.json) first, then falls back
/// to parsing LICENSE files for common patterns.
fn detect_licenses(repo_path: &Path) -> Vec<String> {
    let mut licenses = Vec::new();

    // try Cargo.toml first
    if let Some(license) = detect_license_from_cargo_toml(repo_path) {
        licenses.push(license);
    }

    // try package.json
    if licenses.is_empty() {
        if let Some(license) = detect_license_from_package_json(repo_path) {
            licenses.push(license);
        }
    }

    // fall back to LICENSE file parsing
    if licenses.is_empty() {
        if let Some(license) = detect_license_from_license_file(repo_path) {
            licenses.push(license);
        }
    }

    licenses
}

fn detect_license_from_cargo_toml(repo_path: &Path) -> Option<String> {
    let cargo_path = repo_path.join("Cargo.toml");
    let contents = std::fs::read_to_string(cargo_path).ok()?;

    // parse as TOML and extract license field
    let parsed: toml::Value = toml::from_str(&contents).ok()?;
    let license = parsed
        .get("package")?
        .get("license")?
        .as_str()?
        .to_string();

    if license.is_empty() {
        None
    } else {
        Some(license)
    }
}

fn detect_license_from_package_json(repo_path: &Path) -> Option<String> {
    let package_path = repo_path.join("package.json");
    let contents = std::fs::read_to_string(package_path).ok()?;

    // simple JSON parsing for license field
    let parsed: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let license = parsed.get("license")?.as_str()?.to_string();

    if license.is_empty() {
        None
    } else {
        Some(license)
    }
}

fn detect_license_from_license_file(repo_path: &Path) -> Option<String> {
    let license_files = ["LICENSE", "LICENSE.md", "LICENSE.txt", "LICENCE", "LICENCE.md"];

    for filename in license_files {
        let path = repo_path.join(filename);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Some(spdx) = match_license_text(&contents) {
                return Some(spdx);
            }
        }
    }

    None
}

/// Match license file contents to SPDX identifiers.
fn match_license_text(contents: &str) -> Option<String> {
    let contents_lower = contents.to_lowercase();

    // check for common license patterns
    // ordered roughly by popularity

    if contents_lower.contains("mit license") || contents_lower.contains("permission is hereby granted, free of charge") {
        return Some("MIT".to_string());
    }

    if contents_lower.contains("apache license") {
        if contents_lower.contains("version 2.0") {
            return Some("Apache-2.0".to_string());
        }
        return Some("Apache-2.0".to_string()); // assume 2.0 if unspecified
    }

    if contents_lower.contains("gnu general public license") {
        if contents_lower.contains("version 3") {
            return Some("GPL-3.0".to_string());
        }
        if contents_lower.contains("version 2") {
            return Some("GPL-2.0".to_string());
        }
        return Some("GPL-3.0".to_string()); // assume 3.0 if unspecified
    }

    if contents_lower.contains("gnu lesser general public license") {
        if contents_lower.contains("version 3") {
            return Some("LGPL-3.0".to_string());
        }
        if contents_lower.contains("version 2.1") {
            return Some("LGPL-2.1".to_string());
        }
        return Some("LGPL-3.0".to_string());
    }

    if contents_lower.contains("bsd 3-clause") || contents_lower.contains("3-clause bsd") {
        return Some("BSD-3-Clause".to_string());
    }

    if contents_lower.contains("bsd 2-clause") || contents_lower.contains("2-clause bsd") || contents_lower.contains("simplified bsd") {
        return Some("BSD-2-Clause".to_string());
    }

    if contents_lower.contains("mozilla public license") {
        if contents_lower.contains("version 2.0") {
            return Some("MPL-2.0".to_string());
        }
        return Some("MPL-2.0".to_string());
    }

    if contents_lower.contains("the unlicense") || contents_lower.contains("this is free and unencumbered software") {
        return Some("Unlicense".to_string());
    }

    if contents_lower.contains("isc license") {
        return Some("ISC".to_string());
    }

    if contents_lower.contains("boost software license") {
        return Some("BSL-1.0".to_string());
    }

    if contents_lower.contains("creative commons") {
        if contents_lower.contains("cc0") || contents_lower.contains("public domain") {
            return Some("CC0-1.0".to_string());
        }
    }

    if contents_lower.contains("do what the fuck you want") || contents_lower.contains("wtfpl") {
        return Some("WTFPL".to_string());
    }

    if contents_lower.contains("zlib license") {
        return Some("Zlib".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_title_case_simple_word() {
        assert_eq!(title_case_word("hello"), "Hello");
        assert_eq!(title_case_word("WORLD"), "WORLD");
        assert_eq!(title_case_word(""), "");
    }

    #[test]
    fn can_match_mit_license() {
        let mit_text = "MIT License\n\nPermission is hereby granted, free of charge...";
        assert_eq!(match_license_text(mit_text), Some("MIT".to_string()));
    }

    #[test]
    fn can_match_apache_license() {
        let apache_text = "Apache License\nVersion 2.0, January 2004";
        assert_eq!(match_license_text(apache_text), Some("Apache-2.0".to_string()));
    }

    #[test]
    fn can_match_gpl3_license() {
        let gpl_text = "GNU General Public License\nVersion 3, 29 June 2007";
        assert_eq!(match_license_text(gpl_text), Some("GPL-3.0".to_string()));
    }

    #[test]
    fn can_detect_frontmatter_files() {
        let files = vec![
            PathBuf::from("src/main.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("README.md"),
            PathBuf::from("LICENSE"),
            PathBuf::from("Cargo.toml"),
            PathBuf::from("CONTRIBUTING.md"),
        ];

        let frontmatter = detect_frontmatter(&files);

        // should be in order: README, CONTRIBUTING, Cargo.toml, LICENSE
        assert_eq!(frontmatter.len(), 4);
        assert_eq!(frontmatter[0], PathBuf::from("README.md"));
        assert_eq!(frontmatter[1], PathBuf::from("CONTRIBUTING.md"));
        assert_eq!(frontmatter[2], PathBuf::from("Cargo.toml"));
        assert_eq!(frontmatter[3], PathBuf::from("LICENSE"));
    }

    #[test]
    fn frontmatter_ignores_nested_files() {
        let files = vec![
            PathBuf::from("docs/README.md"),
            PathBuf::from("src/LICENSE"),
            PathBuf::from("README.md"),
        ];

        let frontmatter = detect_frontmatter(&files);

        // only root-level README.md should be detected
        assert_eq!(frontmatter.len(), 1);
        assert_eq!(frontmatter[0], PathBuf::from("README.md"));
    }
}

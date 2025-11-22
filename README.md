# src-book

Convert source code repositories into printable PDF books.

Archive your software the old-fashioned way by turning your codebase into a formatted book with syntax highlighting, table of contents, and commit history.

## Features

- Extracts files from git repositories (respects `.gitignore`)
- Syntax highlighting with configurable themes
- Generated PDF includes:
  - Title page with authors
  - Table of contents with clickable links
  - Syntax-highlighted source files
  - Embedded images (PNG, JPG, SVG, etc.)
  - Commit history
  - PDF bookmarks for navigation
- Configurable page dimensions, margins, and fonts

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
```

## Usage

### 1. Create a configuration file

Run the interactive configuration wizard:

```bash
src-book config
```

This will prompt you for:
- Book title
- Repository path
- File globs to exclude
- Authors
- Licenses
- Output PDF settings

The wizard creates a `src-book.toml` configuration file.

### 2. Render the PDF

```bash
src-book render
```

This reads `src-book.toml` and generates the PDF.

## Configuration

Example `src-book.toml`:

```toml
[source]
title = "My Project"
repository = "."
licenses = ["MIT"]

[[source.authors]]
identifier = "Jane Doe <jane@example.com>"

[pdf]
outfile = "my-project.pdf"
theme = "GitHub"
font = "SourceCodePro"
page_width = 8.5
page_height = 11.0
margin_x = 0.5
margin_y = 0.5
```

### Available Syntax Themes

- `Solarized (light)`
- `OneHalfLight`
- `gruvbox (Light) (Hard)`
- `GitHub`

## Requirements

- Rust 1.70+
- The target repository must be a git repository

## License

See repository for license information.

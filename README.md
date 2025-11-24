# src-book

Convert source code repositories into printable books.

Archive your software the old-fashioned way by turning your codebase into a
formatted book with syntax highlighting, table of contents, and commit history.
Output to PDF for printing or EPUB for e-readers.

## Prerequisites

- Rust 1.70 or later
- A git repository to convert

### Dependencies

This project depends on [pdf-gen](../pdf-gen), a sibling crate for PDF
generation. Clone it alongside this repository:

```
projects/
├── pdf-gen/      # PDF generation library
└── src-book/     # This project
```

### Git Submodules

Syntax highlighting themes are included as git submodules. After cloning,
initialise them:

```bash
git submodule update --init --recursive
```

If themes fail to load during build, this is usually the cause.

## Installation

```bash
# Initialise submodules first
git submodule update --init --recursive

# Build and install
cargo install --path .
```

Or build without installing:

```bash
cargo build --release
```

The binary will be at `target/release/src-book`.

## Quick Start

```bash
# Navigate to your project
cd /path/to/your/project

# Run the interactive configuration wizard
src-book config

# Generate the book
src-book render
```

The wizard creates a `src-book.toml` configuration file, then `render` produces
your PDF and/or EPUB.

## Features

### Output Formats

- **PDF**: Print-ready document with optional booklet layout for saddle-stitch
  binding
- **EPUB**: E-reader compatible format with syntax highlighting and navigation

### Book Contents

- Title page with authors and licences
- Table of contents with clickable links
- Frontmatter section for documentation (README, LICENSE, etc.)
- Syntax-highlighted source files
- Embedded images (PNG, JPG, SVG)
- Commit history appendix
- Hierarchical bookmarks for navigation

### Layout and Typography

- Configurable page dimensions and margins
- Asymmetric margins for booklet binding
- Bundled monospace fonts (Source Code Pro, Fira Mono)
- Custom font support
- Section-specific page numbering (Roman numerals for frontmatter, etc.)
- Customisable headers and footers with template placeholders

### Smart Defaults

- Extracts files from git repositories (respects `.gitignore`)
- Auto-detects project title, entrypoint, and licences
- Entrypoint-aware file ordering for logical reading flow
- Optional submodule exclusion
- Layout capacity analysis to prevent line wrapping issues

## Usage

### 1. Configure

Run the interactive wizard to create `src-book.toml`:

```bash
src-book config
```

The wizard prompts for:
- Book title
- Repository path and file exclusions
- Frontmatter file selection
- Authors and licences
- Output format settings (PDF, EPUB, or both)
- Page size, fonts, and theme

For CI/scripting, use non-interactive mode:

```bash
src-book config --yes              # Use detected defaults
src-book config --yes -o book.pdf  # Override output path
```

### 2. Update (optional)

If files have changed in your repository, refresh the file lists without
re-running the full wizard:

```bash
src-book update
```

This re-scans the repository while preserving your settings.

### 3. Render

Generate the configured outputs:

```bash
src-book render
```

Before rendering, the tool displays layout information showing characters per
line for PDF output, giving you a chance to cancel and adjust settings if
needed.

## Configuration Reference

The `src-book.toml` file is organised into sections. Below are the key options;
the wizard generates a complete file with all available settings.

### Source Settings

```toml
[source]
title = "My Project"
repository = "."
licences = ["MIT"]
commit_order = "NewestFirst"  # NewestFirst | OldestFirst | Disabled
entrypoint = "src/main.rs"
block_globs = ["*.generated.rs"]
exclude_submodules = true
frontmatter_files = ["README.md", "LICENSE"]
source_files = ["src/main.rs", "src/lib.rs"]

[[source.authors]]
identifier = "Jane Doe <jane@example.com>"
```

### PDF Settings

```toml
[pdf]
outfile = "my-project.pdf"
font = "SourceCodePro"
theme = "Solarized (light)"

[pdf.page]
width_in = 5.5
height_in = 8.5

[pdf.margins]
top_in = 0.5
bottom_in = 0.25
inner_in = 0.25   # gutter side
outer_in = 0.125

[pdf.fonts]
title_pt = 32.0
heading_pt = 24.0
subheading_pt = 12.0
body_pt = 10.0
small_pt = 8.0

[pdf.header]
template = "{file}"
position = "Outer"
rule = "Below"

[pdf.footer]
template = "{n}"
position = "Outer"
rule = "None"

[pdf.numbering.frontmatter]
style = "RomanLower"
start = 1

[pdf.numbering.source]
style = "Arabic"
start = 1
```

### EPUB Settings

```toml
[epub]
outfile = "my-project.epub"
theme = "GitHub"

[epub.cover]
template = "{title}\n\n{authors}"
image = ""

[epub.metadata]
language = "en"
```

### Booklet Settings

For saddle-stitch binding, enable booklet output:

```toml
[pdf.booklet]
outfile = "my-project-booklet.pdf"
signature_size = 16
sheet_width_in = 11.0
sheet_height_in = 8.5
```

## Template Placeholders

Headers, footers, title pages, and cover pages support these placeholders:

| Placeholder        | Description                          | Available In            |
|--------------------|--------------------------------------|-------------------------|
| `{title}`          | Book title                           | All templates           |
| `{authors}`        | Formatted author list                | Title, cover, colophon  |
| `{licences}`       | Licence identifiers                  | Title, cover, colophon  |
| `{date}`           | Current date                         | Title, cover            |
| `{file}`           | Current file path                    | Header, footer          |
| `{n}`              | Page number (section-formatted)      | Header, footer          |
| `{total}`          | Section page count                   | Header, footer          |
| `{remotes}`        | Git remote URLs                      | Colophon                |
| `{file_count}`     | Number of files                      | Colophon                |
| `{line_count}`     | Total lines of code                  | Colophon                |
| `{commit_count}`   | Number of commits                    | Colophon                |
| `{language_stats}` | Lines per language breakdown         | Colophon                |
| `{commit_chart}`   | ASCII commit activity histogram      | Colophon                |

## Available Themes

- Solarized (light)
- OneHalfLight
- gruvbox (Light) (Hard)
- GitHub

All themes are light-coloured, optimised for printing on white paper.

## Bundled Fonts

| Font           | Variants                                |
|----------------|-----------------------------------------|
| SourceCodePro  | Regular, Bold, Italic, BoldItalic       |
| FiraMono       | Regular, Bold (italic falls back)       |

### Custom Fonts

Place `.ttf` files next to `src-book.toml`:

```toml
[pdf]
font = "./MyFont"
```

The tool looks for:
- `MyFont-Regular.ttf` (required)
- `MyFont-Bold.ttf` (optional)
- `MyFont-Italic.ttf` or `MyFont-It.ttf` (optional)
- `MyFont-BoldItalic.ttf` or `MyFont-BoldIt.ttf` (optional)

## Booklet Printing

The booklet PDF uses saddle-stitch imposition. When printed double-sided and
folded, pages appear in the correct order.

### Printing Instructions

1. Print double-sided, flipping on the **short edge**
2. Print one signature at a time (16 pages = 4 sheets by default)
3. For each signature: nest the sheets and fold in half
4. Stack all signatures and bind along the spine

### Signatures

A signature is a group of nested, folded sheets. The `signature_size` setting
(must be divisible by 4) controls pages per signature:

- 16 pages = 4 sheets (common for small booklets)
- 32 or 48 pages for thicker books with proper binding

The tool pads the final signature with blank pages if needed.

## Frontmatter

Frontmatter files appear in their own section before source code, giving readers
context before diving into implementation. The wizard auto-detects common
candidates:

- README, ARCHITECTURE, CONTRIBUTING, CHANGELOG
- CODE_OF_CONDUCT, SECURITY
- Manifest files (Cargo.toml, package.json, pyproject.toml, go.mod)
- Licence files

## Entrypoint and File Ordering

Specifying an entrypoint (e.g., `src/main.rs`) sorts files for logical reading:

1. The entrypoint file first
2. Other files in the entrypoint's directory
3. Subdirectories of the entrypoint's directory
4. Everything else alphabetically

## Licence

Apache-2.0

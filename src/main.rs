use anyhow::{Context, Result};
use cli::Cli;
use config_wizard::Configuration;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::ExitCode;

mod cli;
mod config_wizard;
mod detection;
mod file_ordering;
mod highlight;
mod sinks {
    mod pdf;
    pub use pdf::{
        default_colophon_template, default_title_page_template, PageSize, Position, RulePosition,
        SyntaxTheme, TitlePageImagePosition, PDF,
    };
}
mod source;
mod update;

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
        cli::Commands::Config(args) => config_wizard::run(args),
        cli::Commands::Update => update::run(),
        cli::Commands::Render => {
            println!("Loading configuration...");
            let contents = std::fs::read_to_string("src-book.toml")
                .with_context(|| "Failed to load src-book.toml contents")?;
            let config: Configuration =
                toml::from_str(&contents).with_context(|| "Failed to parse TOML")?;

            let Configuration { source, pdf } = config;

            if let Some(pdf) = pdf {
                let total_files = source.frontmatter_files.len() + source.source_files.len();
                let progress = ProgressBar::new(total_files as u64);
                progress.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                        .expect("can parse progress style")
                        .progress_chars("#>-"),
                );
                progress.set_message("Rendering PDF...");

                let stats = pdf
                    .render(&source, &progress)
                    .with_context(|| "Failed to render PDF")?;

                println!();
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

            Ok(())
        }
    }
}

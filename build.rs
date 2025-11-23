use anyhow::{Context, Result};
use std::{fs::File, io::BufReader, path::PathBuf};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

fn load_theme<P: Into<PathBuf>>(themes: &mut ThemeSet, path: P) -> Result<()> {
    let path: PathBuf = path.into();
    let file = File::open(&path)
        .with_context(|| format!("Failed to open file `{}` for reading", path.display()))?;
    let mut reader = BufReader::new(file);
    let theme = ThemeSet::load_from_reader(&mut reader)
        .with_context(|| format!("Failed to parse theme `{}`", path.display()))?;

    let theme_name = theme.name.clone().unwrap_or_else(|| {
        path.file_stem()
            .expect("file has a stem")
            .to_string_lossy()
            .to_string()
    });
    themes.themes.insert(theme_name, theme);
    Ok(())
}

fn main() -> Result<()> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR is set"));

    // generate syntaxes.bin from syntect's defaults
    let syntax_path = out_dir.join("syntaxes.bin");
    let ss = SyntaxSet::load_defaults_newlines();
    let syntax_bytes = bincode::serde::encode_to_vec(&ss, bincode::config::standard())
        .with_context(|| "Failed to serialize syntaxset to bincode")?;
    std::fs::write(&syntax_path, syntax_bytes)
        .with_context(|| "Failed to write serialized syntaxes")?;

    // generate themes.bin from bundled theme files
    let themes_path = out_dir.join("themes.bin");
    let mut themes = ThemeSet::new();
    load_theme(
        &mut themes,
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/themes/Solarized/Solarized (light).tmTheme"
        ),
    )?;
    load_theme(
        &mut themes,
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/themes/onehalf/sublimetext/OneHalfLight.tmTheme"
        ),
    )?;
    load_theme(
        &mut themes,
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/themes/gruvbox-tmTheme/gruvbox (Light) (Hard).tmTheme"
        ),
    )?;
    load_theme(
        &mut themes,
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/themes/github-sublime-theme/GitHub.tmTheme"
        ),
    )?;

    let themes_bytes = bincode::serde::encode_to_vec(&themes, bincode::config::standard())
        .with_context(|| "Failed to serialize themeset to bincode")?;
    std::fs::write(&themes_path, themes_bytes)
        .with_context(|| "Failed to write serialized themes")?;

    Ok(())
}

use anyhow::{Context, Result};
use std::{fs::File, io::BufReader, path::PathBuf};
use syntect::highlighting::ThemeSet;

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
    let themebin_path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/themes.bin"));
    if themebin_path.exists() {
        return Ok(());
    }

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

    let themes =
        bincode::serialize(&themes).with_context(|| "Failed to serialize themeset to bincode")?;
    std::fs::write(themebin_path, themes)
        .with_context(|| "Failed to write serialized themese to bincode")?;

    Ok(())
}

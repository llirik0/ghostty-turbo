use std::{
    env, fs,
    path::{Path, PathBuf},
};

use eframe::egui::Color32;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct ThemeCatalog {
    themes: Vec<AppTheme>,
    active_index: usize,
}

impl ThemeCatalog {
    pub fn load(search_root: &Path) -> Self {
        let mut themes = resolve_themes_dir(search_root)
            .map(|dir| load_themes_from_dir(&dir))
            .unwrap_or_default();

        if themes.is_empty() {
            themes.push(AppTheme::fallback_tokyo_night());
        }

        themes.sort_by(|left, right| left.name.cmp(&right.name));

        let active_index = themes
            .iter()
            .position(|theme| theme.slug == "tokyo-night")
            .unwrap_or(0);

        Self {
            themes,
            active_index,
        }
    }

    pub fn active(&self) -> &AppTheme {
        &self.themes[self.active_index]
    }

    pub fn themes(&self) -> &[AppTheme] {
        &self.themes
    }

    pub fn set_active_by_slug(&mut self, slug: &str) -> bool {
        let Some(index) = self.themes.iter().position(|theme| theme.slug == slug) else {
            return false;
        };

        if index == self.active_index {
            return false;
        }

        self.active_index = index;
        true
    }
}

#[derive(Clone, Debug)]
pub struct AppTheme {
    pub slug: String,
    pub name: String,
    pub directory: Option<PathBuf>,
    pub preview: Option<PathBuf>,
    pub background: Option<PathBuf>,
    pub integrations: Vec<PathBuf>,
    pub palette: ThemePalette,
}

impl AppTheme {
    fn fallback_tokyo_night() -> Self {
        Self {
            slug: "tokyo-night".into(),
            name: "Tokyo Night".into(),
            directory: None,
            preview: None,
            background: None,
            integrations: Vec::new(),
            palette: ThemePalette::from_file(ThemeFile {
                accent: "#7aa2f7".into(),
                cursor: "#c0caf5".into(),
                foreground: "#a9b1d6".into(),
                background: "#1a1b26".into(),
                selection_foreground: "#c0caf5".into(),
                selection_background: "#7aa2f7".into(),
                color0: "#32344a".into(),
                color1: "#f7768e".into(),
                color2: "#9ece6a".into(),
                color3: "#e0af68".into(),
                color4: "#7aa2f7".into(),
                color5: "#ad8ee6".into(),
                color6: "#449dab".into(),
                color7: "#787c99".into(),
                color8: "#444b6a".into(),
                color9: "#ff7a93".into(),
                color10: "#b9f27c".into(),
                color11: "#ff9e64".into(),
                color12: "#7da6ff".into(),
                color13: "#bb9af7".into(),
                color14: "#0db9d7".into(),
                color15: "#acb0d0".into(),
            }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThemePalette {
    pub accent: Color32,
    #[allow(dead_code)]
    pub cursor: Color32,
    pub foreground: Color32,
    pub background: Color32,
    pub selection_foreground: Color32,
    pub selection_background: Color32,
    pub colors: [Color32; 16],
}

impl ThemePalette {
    pub fn panel_bg(&self) -> Color32 {
        mix(self.background, self.colors[0], 0.56)
    }

    pub fn card_bg(&self) -> Color32 {
        mix(self.panel_bg(), self.colors[8], 0.34)
    }

    pub fn elevated_bg(&self) -> Color32 {
        mix(self.card_bg(), self.foreground, 0.06)
    }

    pub fn terminal_bg(&self) -> Color32 {
        mix(self.background, self.colors[0], 0.32)
    }

    pub fn border(&self) -> Color32 {
        with_alpha(mix(self.accent, self.foreground, 0.22), 132)
    }

    pub fn strong_border(&self) -> Color32 {
        with_alpha(self.accent, 212)
    }

    pub fn muted_text(&self) -> Color32 {
        mix(self.foreground, self.background, 0.44)
    }

    pub fn chrome_text(&self) -> Color32 {
        mix(self.foreground, self.background, 0.18)
    }

    pub fn selected_fill(&self) -> Color32 {
        mix(self.accent, self.background, 0.30)
    }

    pub fn hover_fill(&self) -> Color32 {
        mix(self.accent, self.background, 0.18)
    }

    pub fn status_fill(&self, status: &str) -> Color32 {
        if status.contains('?') {
            return mix(self.colors[12], self.background, 0.16);
        }
        if status.contains('D') {
            return mix(self.colors[1], self.background, 0.16);
        }
        if status.contains('A') {
            return mix(self.colors[2], self.background, 0.16);
        }
        if status.contains('R') {
            return mix(self.colors[13], self.background, 0.16);
        }

        mix(self.foreground, self.background, 0.10)
    }

    fn from_file(file: ThemeFile) -> Self {
        Self {
            accent: parse_hex(&file.accent),
            cursor: parse_hex(&file.cursor),
            foreground: parse_hex(&file.foreground),
            background: parse_hex(&file.background),
            selection_foreground: parse_hex(&file.selection_foreground),
            selection_background: parse_hex(&file.selection_background),
            colors: [
                parse_hex(&file.color0),
                parse_hex(&file.color1),
                parse_hex(&file.color2),
                parse_hex(&file.color3),
                parse_hex(&file.color4),
                parse_hex(&file.color5),
                parse_hex(&file.color6),
                parse_hex(&file.color7),
                parse_hex(&file.color8),
                parse_hex(&file.color9),
                parse_hex(&file.color10),
                parse_hex(&file.color11),
                parse_hex(&file.color12),
                parse_hex(&file.color13),
                parse_hex(&file.color14),
                parse_hex(&file.color15),
            ],
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ThemeFile {
    accent: String,
    cursor: String,
    foreground: String,
    background: String,
    selection_foreground: String,
    selection_background: String,
    color0: String,
    color1: String,
    color2: String,
    color3: String,
    color4: String,
    color5: String,
    color6: String,
    color7: String,
    color8: String,
    color9: String,
    color10: String,
    color11: String,
    color12: String,
    color13: String,
    color14: String,
    color15: String,
}

fn load_themes_from_dir(dir: &Path) -> Vec<AppTheme> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| load_theme(entry.path()))
        .collect()
}

fn load_theme(path: PathBuf) -> Option<AppTheme> {
    if !path.is_dir() {
        return None;
    }

    let slug = path.file_name()?.to_string_lossy().to_string();
    let colors_path = path.join("colors.toml");
    let contents = fs::read_to_string(colors_path).ok()?;
    let file: ThemeFile = toml::from_str(&contents).ok()?;

    Some(AppTheme {
        slug: slug.clone(),
        name: display_name(&slug),
        preview: first_existing(&[
            path.join("preview.png"),
            path.join("preview.jpg"),
            path.join("preview.jpeg"),
        ]),
        background: first_file_in(&path.join("backgrounds")),
        integrations: collect_integrations(&path),
        directory: Some(path),
        palette: ThemePalette::from_file(file),
    })
}

fn resolve_themes_dir(search_root: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(dir) = env::var_os("GHOSTTY_SHELL_THEMES_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    candidates.push(search_root.join("themes"));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../themes"));
    if let Some(exe_dir) = env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    {
        candidates.push(exe_dir.join("themes"));
    }

    candidates.into_iter().find(|path| path.is_dir())
}

fn collect_integrations(path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };

    let mut items = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|entry| entry.is_file())
        .filter(|entry| entry.file_name().and_then(|name| name.to_str()) != Some("colors.toml"))
        .collect::<Vec<_>>();
    items.sort();
    items
}

fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.is_file()).cloned()
}

fn first_file_in(dir: &Path) -> Option<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };

    let mut items = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|entry| entry.is_file())
        .collect::<Vec<_>>();
    items.sort();
    items.into_iter().next()
}

fn display_name(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };

            let mut word = String::new();
            word.push(first.to_ascii_uppercase());
            word.push_str(chars.as_str());
            word
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_hex(value: &str) -> Color32 {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 {
        return Color32::WHITE;
    }

    let red = u8::from_str_radix(&value[0..2], 16).unwrap_or(255);
    let green = u8::from_str_radix(&value[2..4], 16).unwrap_or(255);
    let blue = u8::from_str_radix(&value[4..6], 16).unwrap_or(255);
    Color32::from_rgb(red, green, blue)
}

fn mix(a: Color32, b: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let inverse = 1.0 - amount;

    Color32::from_rgb(
        ((a.r() as f32 * inverse) + (b.r() as f32 * amount)).round() as u8,
        ((a.g() as f32 * inverse) + (b.g() as f32 * amount)).round() as u8,
        ((a.b() as f32 * inverse) + (b.b() as f32 * amount)).round() as u8,
    )
}

fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn fallback_tokyo_night_has_expected_slug_and_accent() {
        let theme = AppTheme::fallback_tokyo_night();

        assert_eq!(theme.slug, "tokyo-night");
        assert_eq!(theme.name, "Tokyo Night");
        assert_eq!(theme.palette.accent, Color32::from_rgb(122, 162, 247));
    }

    #[test]
    fn theme_catalog_prefers_tokyo_night_as_default() {
        let temp = TempDir::new().expect("temp dir");
        create_theme(temp.path(), "catppuccin");
        create_theme(temp.path(), "tokyo-night");

        let catalog = ThemeCatalog::load(temp.path());

        assert_eq!(catalog.active().slug, "tokyo-night");
    }

    #[test]
    fn theme_catalog_falls_back_to_first_theme_when_tokyo_missing() {
        let temp = TempDir::new().expect("temp dir");
        create_theme(temp.path(), "catppuccin");
        create_theme(temp.path(), "solarized");

        let catalog = ThemeCatalog::load(temp.path());

        assert_eq!(catalog.active().slug, "catppuccin");
    }

    #[test]
    fn theme_catalog_switches_active_theme_by_slug() {
        let temp = TempDir::new().expect("temp dir");
        create_theme(temp.path(), "catppuccin");
        create_theme(temp.path(), "tokyo-night");
        let mut catalog = ThemeCatalog::load(temp.path());

        assert!(catalog.set_active_by_slug("catppuccin"));
        assert_eq!(catalog.active().slug, "catppuccin");
    }

    #[test]
    fn theme_catalog_rejects_unknown_theme_slug() {
        let temp = TempDir::new().expect("temp dir");
        create_theme(temp.path(), "tokyo-night");
        let mut catalog = ThemeCatalog::load(temp.path());

        assert!(!catalog.set_active_by_slug("missing"));
        assert_eq!(catalog.active().slug, "tokyo-night");
    }

    #[test]
    fn load_theme_reads_preview_background_and_integrations() {
        let temp = TempDir::new().expect("temp dir");
        let theme_dir = create_theme(temp.path(), "tokyo-night");
        fs::write(theme_dir.join("preview.png"), "preview").expect("preview");
        fs::create_dir_all(theme_dir.join("backgrounds")).expect("background dir");
        fs::write(theme_dir.join("backgrounds/2-b.png"), "bg").expect("background 2");
        fs::write(theme_dir.join("backgrounds/1-a.png"), "bg").expect("background 1");
        fs::write(theme_dir.join("neovim.lua"), "return {}").expect("integration");

        let theme = load_theme(theme_dir.clone()).expect("theme");

        assert_eq!(
            theme.preview.as_deref(),
            Some(theme_dir.join("preview.png").as_path())
        );
        assert_eq!(
            theme.background.as_deref(),
            Some(theme_dir.join("backgrounds/1-a.png").as_path())
        );
        assert!(
            theme.integrations.iter().any(|path| {
                path.file_name().and_then(|name| name.to_str()) == Some("neovim.lua")
            })
        );
    }

    #[test]
    fn load_theme_skips_invalid_theme_dir() {
        let temp = TempDir::new().expect("temp dir");
        let invalid = temp.path().join("broken");
        fs::create_dir_all(&invalid).expect("broken theme");

        assert!(load_theme(invalid).is_none());
    }

    #[test]
    fn resolve_themes_dir_uses_search_root_theme_dir() {
        let temp = TempDir::new().expect("temp dir");
        let themes_dir = temp.path().join("themes");
        fs::create_dir_all(&themes_dir).expect("themes dir");

        assert_eq!(
            resolve_themes_dir(temp.path()).as_deref(),
            Some(themes_dir.as_path())
        );
    }

    #[test]
    fn collect_integrations_ignores_colors_file() {
        let temp = TempDir::new().expect("temp dir");
        let theme_dir = create_theme(temp.path(), "tokyo-night");
        fs::write(theme_dir.join("preview.png"), "preview").expect("preview");
        fs::write(theme_dir.join("waybar.css"), "css").expect("waybar");

        let integrations = collect_integrations(&theme_dir);

        assert!(!integrations.iter().any(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("colors.toml")
        }));
        assert!(
            integrations.iter().any(|path| {
                path.file_name().and_then(|name| name.to_str()) == Some("waybar.css")
            })
        );
    }

    #[test]
    fn first_file_in_returns_sorted_first_file() {
        let temp = TempDir::new().expect("temp dir");
        let dir = temp.path().join("backgrounds");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("b.png"), "b").expect("b");
        fs::write(dir.join("a.png"), "a").expect("a");

        assert_eq!(
            first_file_in(&dir).as_deref(),
            Some(dir.join("a.png").as_path())
        );
    }

    #[test]
    fn display_name_formats_slug() {
        assert_eq!(display_name("tokyo-night"), "Tokyo Night");
        assert_eq!(display_name("catppuccin_mocha"), "Catppuccin Mocha");
    }

    #[test]
    fn parse_hex_reads_rgb_value() {
        assert_eq!(parse_hex("#7aa2f7"), Color32::from_rgb(122, 162, 247));
    }

    #[test]
    fn parse_hex_accepts_missing_hash() {
        assert_eq!(parse_hex("1a1b26"), Color32::from_rgb(26, 27, 38));
    }

    #[test]
    fn parse_hex_bad_length_returns_white() {
        assert_eq!(parse_hex("#12345"), Color32::WHITE);
    }

    #[test]
    fn mix_zero_and_one_return_inputs() {
        let a = Color32::from_rgb(10, 20, 30);
        let b = Color32::from_rgb(200, 210, 220);

        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
    }

    #[test]
    fn with_alpha_sets_alpha() {
        assert_eq!(with_alpha(Color32::from_rgb(1, 2, 3), 77).a(), 77);
    }

    #[test]
    fn status_fill_changes_by_status() {
        let palette = AppTheme::fallback_tokyo_night().palette;

        assert_ne!(palette.status_fill("??"), palette.status_fill("A"));
        assert_ne!(palette.status_fill("D"), palette.status_fill("R"));
        assert_ne!(palette.status_fill("M"), palette.status_fill("D"));
    }

    fn create_theme(root: &Path, slug: &str) -> PathBuf {
        let theme_dir = root.join("themes").join(slug);
        fs::create_dir_all(&theme_dir).expect("theme dir");
        fs::write(theme_dir.join("colors.toml"), sample_colors()).expect("colors");
        theme_dir
    }

    fn sample_colors() -> &'static str {
        r##"accent = "#7aa2f7"
cursor = "#c0caf5"
foreground = "#a9b1d6"
background = "#1a1b26"
selection_foreground = "#c0caf5"
selection_background = "#7aa2f7"

color0 = "#32344a"
color1 = "#f7768e"
color2 = "#9ece6a"
color3 = "#e0af68"
color4 = "#7aa2f7"
color5 = "#ad8ee6"
color6 = "#449dab"
color7 = "#787c99"
color8 = "#444b6a"
color9 = "#ff7a93"
color10 = "#b9f27c"
color11 = "#ff9e64"
color12 = "#7da6ff"
color13 = "#bb9af7"
color14 = "#0db9d7"
color15 = "#acb0d0"
"##
    }
}

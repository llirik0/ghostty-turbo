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

    pub fn overlay_line(&self, step: usize) -> Color32 {
        let blend = mix(self.accent, self.colors[13], (step as f32 / 12.0).min(1.0));
        with_alpha(blend, 18 + (step as u8 * 6))
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

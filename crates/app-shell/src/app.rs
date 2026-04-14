use std::{
    env,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use eframe::egui::{
    self, Align, CentralPanel, Color32, Context, CornerRadius, Frame, LayerId, Layout, Order,
    Panel, RichText, ScrollArea, Stroke, TextEdit, pos2,
};

use crate::{
    git::{self, GitChange, GitSnapshot},
    theme::{AppTheme, ThemeCatalog, ThemePalette},
    usage::{self, UsageSnapshot, UsageStatus},
};

pub struct GhosttyShellApp {
    center_mode: CenterMode,
    selected_path: Option<String>,
    themes: ThemeCatalog,
    workspace: WorkspaceSnapshot,
    workspace_rx: Receiver<WorkspaceSnapshot>,
}

impl GhosttyShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let themes = ThemeCatalog::load(&cwd);
        configure_theme(&cc.egui_ctx, &themes.active().palette);

        let workspace = WorkspaceSnapshot::load(&cwd);
        let selected_path = workspace
            .git
            .changes
            .first()
            .map(|change| change.path.clone());

        Self {
            center_mode: CenterMode::Terminal,
            selected_path,
            themes,
            workspace_rx: spawn_workspace_worker(cwd),
            workspace,
        }
    }

    fn active_theme(&self) -> &AppTheme {
        self.themes.active()
    }

    fn drain_updates(&mut self) {
        while let Ok(snapshot) = self.workspace_rx.try_recv() {
            if let Some(path) = self.selected_path.as_deref() {
                if !snapshot
                    .git
                    .changes
                    .iter()
                    .any(|change| change.path == path)
                {
                    self.selected_path = snapshot
                        .git
                        .changes
                        .first()
                        .map(|change| change.path.clone());
                }
            } else {
                self.selected_path = snapshot
                    .git
                    .changes
                    .first()
                    .map(|change| change.path.clone());
            }

            self.workspace = snapshot;
        }
    }

    fn selected_item(&self) -> Option<&GitChange> {
        self.selected_path
            .as_deref()
            .and_then(|path| {
                self.workspace
                    .git
                    .changes
                    .iter()
                    .find(|change| change.path == path)
            })
            .or_else(|| self.workspace.git.changes.first())
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.active_theme().palette.clone();

        Panel::left("repo_changes")
            .resizable(true)
            .default_size(290.0)
            .min_size(240.0)
            .show_inside(ui, |ui| {
                shell_panel(ui, "Changed Files", &theme, |ui| {
                    if self.workspace.git.repo_root.is_none() {
                        empty_state(
                            ui,
                            &theme,
                            "No repo detected",
                            "Launch the shell from inside a git repo and the left rail stops being decorative.",
                        );
                        return;
                    }

                    ui.horizontal_wrapped(|ui| {
                        tag(ui, &theme, "Repo", &self.workspace.git.repo_name);
                        tag(ui, &theme, "Branch", &branch_label(&self.workspace.git));
                    });
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "{} files / +{} -{}",
                            self.workspace.git.changes.len(),
                            self.workspace.git.total_added,
                            self.workspace.git.total_removed
                        ))
                        .color(theme.muted_text()),
                    );
                    ui.add_space(10.0);

                    ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for change in &self.workspace.git.changes {
                                let selected =
                                    self.selected_path.as_deref() == Some(change.path.as_str());
                                let button = egui::Button::new(
                                    RichText::new(format!(
                                        "{}\n{}  +{} -{}",
                                        change.path, change.status, change.added, change.removed
                                    ))
                                    .size(13.5),
                                )
                                .selected(selected)
                                .fill(if selected {
                                    theme.selected_fill()
                                } else {
                                    theme.status_fill(&change.status)
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected {
                                        theme.strong_border()
                                    } else {
                                        theme.border()
                                    },
                                ))
                                .corner_radius(CornerRadius::same(10));

                                if ui.add_sized([ui.available_width(), 48.0], button).clicked() {
                                    self.selected_path = Some(change.path.clone());
                                }

                                ui.add_space(6.0);
                            }
                        });
                });
            });
    }

    fn right_panel(&self, ui: &mut egui::Ui) {
        let theme = &self.active_theme().palette;
        let active_theme = self.active_theme();

        Panel::right("usage_panel")
            .resizable(true)
            .default_size(340.0)
            .min_size(300.0)
            .show_inside(ui, |ui| {
                shell_panel(ui, "Context + Model Usage", theme, |ui| {
                    stat_card(
                        ui,
                        theme,
                        "Theme Pack",
                        &active_theme.name,
                        &format!(
                            "{} / {} / {} extras",
                            active_theme
                                .preview
                                .as_deref()
                                .and_then(|path| path.file_name())
                                .and_then(|name| name.to_str())
                                .unwrap_or("no preview"),
                            active_theme.integrations.len(),
                            active_theme
                                .background
                                .as_deref()
                                .and_then(|path| path.file_name())
                                .and_then(|name| name.to_str())
                                .unwrap_or("no background preview")
                        ),
                    );
                    ui.add_space(8.0);

                    stat_card(
                        ui,
                        theme,
                        "Tracked",
                        "Token totals",
                        &format!(
                            "{} in / {} out / ${:.02}",
                            self.workspace.usage.total_input_tokens,
                            self.workspace.usage.total_output_tokens,
                            self.workspace.usage.total_cost_usd
                        ),
                    );
                    ui.add_space(8.0);

                    stat_card(
                        ui,
                        theme,
                        "Sessions",
                        "Event feed",
                        &format!(
                            "{} sessions / {} events",
                            self.workspace.usage.session_count, self.workspace.usage.event_count
                        ),
                    );
                    ui.add_space(10.0);

                    match self.workspace.usage.status {
                        UsageStatus::Ready | UsageStatus::ParseWarning => {
                            for summary in self.workspace.usage.models.iter().take(4) {
                                stat_card(
                                    ui,
                                    theme,
                                    &summary.provider,
                                    &summary.model,
                                    &format!(
                                        "{} in / {} out / ${:.02} / {} events",
                                        summary.input_tokens,
                                        summary.output_tokens,
                                        summary.cost_usd,
                                        summary.event_count
                                    ),
                                );
                                ui.add_space(8.0);
                            }
                        }
                        UsageStatus::AwaitingFile => empty_state(
                            ui,
                            theme,
                            "No usage log yet",
                            "Feed JSONL events into .ghostty-shell/usage-events.jsonl or set GHOSTTY_SHELL_USAGE_LOG.",
                        ),
                        UsageStatus::AwaitingEvents => empty_state(
                            ui,
                            theme,
                            "Usage log is empty",
                            "The pipe exists. Now give it actual model events.",
                        ),
                        UsageStatus::Error => empty_state(
                            ui,
                            theme,
                            "Usage parsing failed",
                            self.workspace
                                .usage
                                .error
                                .as_deref()
                                .unwrap_or("The event stream is malformed."),
                        ),
                    }

                    ui.add_space(8.0);
                    if let Some(directory) = &active_theme.directory {
                        ui.label(RichText::new("Theme source").size(12.0).color(theme.muted_text()));
                        ui.monospace(directory.display().to_string());
                        ui.add_space(8.0);
                    }

                    ui.label(RichText::new("Watching").size(12.0).color(theme.muted_text()));
                    ui.monospace(self.workspace.usage.source_path.display().to_string());
                });
            });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.active_theme().palette.clone();

        Panel::top("header")
            .resizable(false)
            .exact_size(64.0)
            .show_inside(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.heading(
                        RichText::new("APP SHELL")
                            .size(15.0)
                            .extra_letter_spacing(1.8)
                            .color(theme.foreground),
                    );
                    ui.add_space(12.0);

                    for entry in self.themes.themes().to_vec() {
                        let selected = entry.slug == self.active_theme().slug;
                        let button =
                            egui::Button::new(RichText::new(entry.name.clone()).size(12.5))
                                .fill(if selected {
                                    theme.selected_fill()
                                } else {
                                    theme.card_bg()
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected {
                                        theme.strong_border()
                                    } else {
                                        theme.border()
                                    },
                                ))
                                .corner_radius(CornerRadius::same(999.min(u8::MAX as i32) as u8));

                        if ui.add(button).clicked() {
                            if self.themes.set_active_by_slug(&entry.slug) {
                                configure_theme(ui.ctx(), &self.active_theme().palette);
                            }
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        tag(ui, &theme, "Ghostty", "latest ghostty-org/ghostty");
                        tag(ui, &theme, "Usage", "Live JSONL");
                    });
                });
            });
    }

    fn bottom_bar(&self, ui: &mut egui::Ui) {
        let theme = &self.active_theme().palette;

        Panel::bottom("status")
            .resizable(false)
            .exact_size(42.0)
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "repo {}",
                            non_empty(&self.workspace.git.repo_name, "none")
                        ))
                        .color(theme.chrome_text()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("branch {}", branch_label(&self.workspace.git)))
                            .color(theme.chrome_text()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("cwd {}", self.workspace.cwd.display()))
                            .color(theme.chrome_text()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "file {}",
                            self.selected_item()
                                .map(|change| change.path.as_str())
                                .unwrap_or("none")
                        ))
                        .color(theme.chrome_text()),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "tokens {}",
                            self.workspace.usage.total_input_tokens
                                + self.workspace.usage.total_output_tokens
                        ))
                        .color(theme.chrome_text()),
                    );
                    ui.separator();
                    ui.label(RichText::new("mode").color(theme.chrome_text()));
                    ui.monospace(self.center_mode.title());
                });
            });
    }

    fn center_panel(&mut self, ui: &mut egui::Ui) {
        let theme = self.active_theme().palette.clone();

        CentralPanel::default().show_inside(ui, |ui| {
            shell_panel(ui, "Ghostty Session / File Viewer", &theme, |ui| {
                ui.horizontal(|ui| {
                    for mode in CenterMode::ALL {
                        let selected = self.center_mode == mode;
                        let button = egui::Button::new(RichText::new(mode.title()).size(13.5))
                            .selected(selected)
                            .fill(if selected {
                                theme.selected_fill()
                            } else {
                                theme.card_bg()
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if selected {
                                    theme.strong_border()
                                } else {
                                    theme.border()
                                },
                            ))
                            .corner_radius(CornerRadius::same(9));

                        if ui.add(button).clicked() {
                            self.center_mode = mode;
                        }
                    }
                });

                if let Some(change) = self.selected_item() {
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        tag(ui, &theme, "Focus", &change.path);
                        tag(ui, &theme, "State", &change.status);
                    });
                }

                ui.add_space(12.0);

                match self.center_mode {
                    CenterMode::Terminal => terminal_surface(ui, &self.workspace.git, &theme),
                    CenterMode::Diff => diff_surface(ui, self.selected_item(), &theme),
                    CenterMode::Preview => preview_surface(ui, self.selected_item(), &theme),
                }
            });
        });
    }
}

impl eframe::App for GhosttyShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_updates();
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        paint_backdrop(ui.ctx(), &self.active_theme().palette);

        self.top_bar(ui);
        self.left_panel(ui);
        self.right_panel(ui);
        self.bottom_bar(ui);
        self.center_panel(ui);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CenterMode {
    Terminal,
    Diff,
    Preview,
}

impl CenterMode {
    const ALL: [Self; 3] = [Self::Terminal, Self::Diff, Self::Preview];

    fn title(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Diff => "Diff",
            Self::Preview => "Preview",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct WorkspaceSnapshot {
    cwd: PathBuf,
    git: GitSnapshot,
    usage: UsageSnapshot,
}

impl WorkspaceSnapshot {
    fn load(cwd: &Path) -> Self {
        let git = git::load_snapshot(cwd);
        let usage_root = git.repo_root.as_deref().unwrap_or(cwd);
        let usage = usage::load_snapshot(usage_root);

        Self {
            cwd: cwd.to_path_buf(),
            git,
            usage,
        }
    }
}

fn spawn_workspace_worker(cwd: PathBuf) -> Receiver<WorkspaceSnapshot> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        loop {
            if tx.send(WorkspaceSnapshot::load(&cwd)).is_err() {
                break;
            }

            thread::sleep(Duration::from_secs(2));
        }
    });

    rx
}

fn shell_panel(
    ui: &mut egui::Ui,
    title: &str,
    theme: &ThemePalette,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    Frame::default()
        .fill(theme.panel_bg())
        .stroke(Stroke::new(1.0, theme.border()))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .size(14.0)
                    .strong()
                    .color(theme.foreground),
            );
            ui.add_space(10.0);
            add_contents(ui);
        });
}

fn stat_card(ui: &mut egui::Ui, theme: &ThemePalette, eyebrow: &str, title: &str, body: &str) {
    Frame::default()
        .fill(theme.card_bg())
        .stroke(Stroke::new(1.0, theme.border()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(RichText::new(eyebrow).size(11.5).color(theme.muted_text()));
            ui.label(
                RichText::new(title)
                    .size(16.0)
                    .strong()
                    .color(theme.foreground),
            );
            ui.label(RichText::new(body).size(13.5).color(theme.muted_text()));
        });
}

fn tag(ui: &mut egui::Ui, theme: &ThemePalette, label: &str, value: &str) {
    Frame::default()
        .fill(theme.elevated_bg())
        .stroke(Stroke::new(1.0, theme.border()))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(12.0).color(theme.muted_text()));
                ui.label(
                    RichText::new(value)
                        .size(12.0)
                        .strong()
                        .color(theme.foreground),
                );
            });
        });
}

fn empty_state(ui: &mut egui::Ui, theme: &ThemePalette, title: &str, body: &str) {
    Frame::default()
        .fill(theme.card_bg())
        .stroke(Stroke::new(1.0, theme.border()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .size(15.0)
                    .strong()
                    .color(theme.foreground),
            );
            ui.label(RichText::new(body).size(13.5).color(theme.muted_text()));
        });
}

fn terminal_surface(ui: &mut egui::Ui, git: &GitSnapshot, theme: &ThemePalette) {
    Frame::default()
        .fill(theme.terminal_bg())
        .stroke(Stroke::new(1.0, theme.strong_border()))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(22.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(130.0);
                ui.label(
                    RichText::new("GHOSTTY")
                        .size(28.0)
                        .strong()
                        .color(theme.selection_foreground),
                );
                ui.add_space(14.0);
                ui.label(
                    RichText::new(
                        "Theme system is live. Ghostty surface wiring targets the latest upstream repo next.",
                    )
                    .size(15.0)
                    .color(theme.muted_text()),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "repo {} / branch {} / files {} / theme {}",
                        non_empty(&git.repo_name, "none"),
                        branch_label(git),
                        git.changes.len(),
                        theme_name_hint(theme)
                    ))
                    .size(13.5)
                    .color(theme.muted_text()),
                );
            });
        });
}

fn diff_surface(ui: &mut egui::Ui, change: Option<&GitChange>, theme: &ThemePalette) {
    let mut diff = change
        .map(|change| change.diff.clone())
        .unwrap_or_else(|| "Pick a file on the left and the diff lands here.".into());

    Frame::default()
        .fill(theme.terminal_bg())
        .stroke(Stroke::new(1.0, theme.border()))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            if let Some(change) = change {
                ui.label(
                    RichText::new(format!(
                        "{}  {}  +{} -{}",
                        change.status, change.path, change.added, change.removed
                    ))
                    .strong()
                    .color(theme.foreground),
                );
            } else {
                ui.label(
                    RichText::new("No file selected")
                        .strong()
                        .color(theme.foreground),
                );
            }
            ui.add_space(10.0);
            ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut diff)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(26)
                        .interactive(false)
                        .text_color(theme.foreground),
                );
            });
        });
}

fn preview_surface(ui: &mut egui::Ui, change: Option<&GitChange>, theme: &ThemePalette) {
    let mut preview = change
        .map(|change| change.preview.clone())
        .unwrap_or_else(|| "Pick a file on the left and the file preview lands here.".into());

    Frame::default()
        .fill(theme.terminal_bg())
        .stroke(Stroke::new(1.0, theme.border()))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            if let Some(change) = change {
                ui.label(
                    RichText::new(format!("preview {}", change.path))
                        .strong()
                        .color(theme.foreground),
                );
            } else {
                ui.label(
                    RichText::new("No file selected")
                        .strong()
                        .color(theme.foreground),
                );
            }
            ui.add_space(10.0);
            ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut preview)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(26)
                        .interactive(false)
                        .text_color(theme.foreground),
                );
            });
        });
}

fn configure_theme(ctx: &Context, theme: &ThemePalette) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = theme.background;
    visuals.extreme_bg_color = theme.panel_bg();
    visuals.window_fill = theme.background;
    visuals.faint_bg_color = theme.card_bg();
    visuals.widgets.noninteractive.bg_fill = theme.panel_bg();
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme.border());
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme.foreground);
    visuals.widgets.inactive.bg_fill = theme.card_bg();
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, theme.border());
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, theme.foreground);
    visuals.widgets.hovered.bg_fill = theme.hover_fill();
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, theme.strong_border());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, theme.foreground);
    visuals.widgets.active.bg_fill = theme.selected_fill();
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, theme.strong_border());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, theme.foreground);
    visuals.selection.bg_fill = theme.selection_background;
    visuals.selection.stroke = Stroke::new(1.0, theme.selection_foreground);
    visuals.override_text_color = Some(theme.foreground);
    visuals.code_bg_color = theme.terminal_bg();
    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 14.0;
    ctx.set_global_style(style);
}

fn paint_backdrop(ctx: &Context, theme: &ThemePalette) {
    let rect = ctx.viewport_rect();
    let painter = ctx.layer_painter(LayerId::new(Order::Background, egui::Id::new("theme-bg")));
    painter.rect_filled(rect, 0.0, theme.background);

    let sweep_center = pos2(
        rect.left() + rect.width() * 0.68,
        rect.top() - rect.height() * 0.10,
    );
    for index in 0..13 {
        let radius = rect.width() * 0.10 + index as f32 * 34.0;
        painter.circle_stroke(
            sweep_center,
            radius,
            Stroke::new(1.0, theme.overlay_line(index)),
        );
    }

    for index in 0..14 {
        let offset = index as f32 * 46.0;
        painter.line_segment(
            [
                pos2(rect.left() - rect.width() * 0.08 + offset, rect.top()),
                pos2(rect.left() + rect.width() * 0.56 + offset, rect.bottom()),
            ],
            Stroke::new(1.0, theme.overlay_line(index)),
        );
    }

    painter.circle_filled(
        pos2(rect.right() - 160.0, rect.top() + 120.0),
        3.0,
        theme.cursor,
    );
    painter.line_segment(
        [
            pos2(rect.right() - 178.0, rect.top() + 120.0),
            pos2(rect.right() - 142.0, rect.top() + 120.0),
        ],
        Stroke::new(1.0, theme.overlay_line(8)),
    );
    painter.line_segment(
        [
            pos2(rect.right() - 160.0, rect.top() + 102.0),
            pos2(rect.right() - 160.0, rect.top() + 138.0),
        ],
        Stroke::new(1.0, theme.overlay_line(8)),
    );
}

fn branch_label(git: &GitSnapshot) -> String {
    let branch = non_empty(&git.branch, "detached");
    match (git.ahead, git.behind) {
        (0, 0) => branch.to_owned(),
        (ahead, 0) => format!("{branch} +{ahead}"),
        (0, behind) => format!("{branch} -{behind}"),
        (ahead, behind) => format!("{branch} +{ahead}/-{behind}"),
    }
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn theme_name_hint(theme: &ThemePalette) -> &'static str {
    if theme.accent == Color32::from_rgb(122, 162, 247) {
        "tokyo-night"
    } else {
        "custom"
    }
}

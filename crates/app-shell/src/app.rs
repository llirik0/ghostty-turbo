use std::{
    env,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use eframe::egui::{
    self, Align, CentralPanel, Color32, Context, CornerRadius, Frame, Layout, Panel, RichText,
    ScrollArea, Stroke, TextEdit,
};

use crate::{
    git::{self, GitChange, GitSnapshot},
    usage::{self, UsageSnapshot, UsageStatus},
};

pub struct GhosttyShellApp {
    center_mode: CenterMode,
    selected_path: Option<String>,
    workspace: WorkspaceSnapshot,
    workspace_rx: Receiver<WorkspaceSnapshot>,
}

impl GhosttyShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);

        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let workspace = WorkspaceSnapshot::load(&cwd);
        let selected_path = workspace
            .git
            .changes
            .first()
            .map(|change| change.path.clone());

        Self {
            center_mode: CenterMode::Terminal,
            selected_path,
            workspace_rx: spawn_workspace_worker(cwd),
            workspace,
        }
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
        Panel::left("repo_changes")
            .resizable(true)
            .default_size(270.0)
            .min_size(230.0)
            .show_inside(ui, |ui| {
                shell_panel(ui, "Changed Files", |ui| {
                    if self.workspace.git.repo_root.is_none() {
                        empty_state(
                            ui,
                            "No repo detected",
                            "Launch the shell from inside a git repo and the left rail stops being decorative.",
                        );
                        return;
                    }

                    ui.horizontal_wrapped(|ui| {
                        tag(ui, "Repo", &self.workspace.git.repo_name);
                        tag(ui, "Branch", &branch_label(&self.workspace.git));
                    });
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!(
                            "{} files / +{} -{}",
                            self.workspace.git.changes.len(),
                            self.workspace.git.total_added,
                            self.workspace.git.total_removed
                        ))
                        .color(ink_muted()),
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
                                    Color32::from_rgb(234, 226, 216)
                                } else {
                                    status_fill(&change.status)
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected { ink() } else { border() },
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
        Panel::right("usage_panel")
            .resizable(true)
            .default_size(320.0)
            .min_size(280.0)
            .show_inside(ui, |ui| {
                shell_panel(ui, "Context + Model Usage", |ui| {
                    stat_card(
                        ui,
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
                            "No usage log yet",
                            "Feed JSONL events into .ghostty-shell/usage-events.jsonl or set GHOSTTY_SHELL_USAGE_LOG.",
                        ),
                        UsageStatus::AwaitingEvents => empty_state(
                            ui,
                            "Usage log is empty",
                            "The pipe exists. Now give it actual model events.",
                        ),
                        UsageStatus::Error => empty_state(
                            ui,
                            "Usage parsing failed",
                            self.workspace
                                .usage
                                .error
                                .as_deref()
                                .unwrap_or("The event stream is malformed."),
                        ),
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("Watching").size(12.0).color(ink_muted()));
                    ui.monospace(self.workspace.usage.source_path.display().to_string());
                });
            });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        Panel::top("header")
            .resizable(false)
            .exact_size(58.0)
            .show_inside(ui, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.heading(
                        RichText::new("APP SHELL")
                            .size(15.0)
                            .extra_letter_spacing(1.8)
                            .color(ink()),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        tag(ui, "Usage", "Live JSONL");
                        tag(ui, "Ghostty", "latest ghostty-org/ghostty");
                    });
                });
            });
    }

    fn bottom_bar(&self, ui: &mut egui::Ui) {
        Panel::bottom("status")
            .resizable(false)
            .exact_size(42.0)
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "repo {}",
                        non_empty(&self.workspace.git.repo_name, "none")
                    ));
                    ui.separator();
                    ui.label(format!("branch {}", branch_label(&self.workspace.git)));
                    ui.separator();
                    ui.label(format!("cwd {}", self.workspace.cwd.display()));
                    ui.separator();
                    ui.label(format!(
                        "file {}",
                        self.selected_item()
                            .map(|change| change.path.as_str())
                            .unwrap_or("none")
                    ));
                    ui.separator();
                    ui.label(format!(
                        "tokens {}",
                        self.workspace.usage.total_input_tokens
                            + self.workspace.usage.total_output_tokens
                    ));
                    ui.separator();
                    ui.label("mode");
                    ui.monospace(self.center_mode.title());
                });
            });
    }

    fn center_panel(&mut self, ui: &mut egui::Ui) {
        CentralPanel::default().show_inside(ui, |ui| {
            shell_panel(ui, "Ghostty Session / File Viewer", |ui| {
                ui.horizontal(|ui| {
                    for mode in CenterMode::ALL {
                        let selected = self.center_mode == mode;
                        let button = egui::Button::new(RichText::new(mode.title()).size(13.5))
                            .selected(selected)
                            .fill(if selected {
                                Color32::from_rgb(33, 32, 30)
                            } else {
                                paper()
                            })
                            .stroke(Stroke::new(
                                1.0,
                                if selected {
                                    Color32::from_rgb(33, 32, 30)
                                } else {
                                    border()
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
                        tag(ui, "Focus", &change.path);
                        tag(ui, "State", &change.status);
                    });
                }

                ui.add_space(12.0);

                match self.center_mode {
                    CenterMode::Terminal => terminal_surface(ui, &self.workspace.git),
                    CenterMode::Diff => diff_surface(ui, self.selected_item()),
                    CenterMode::Preview => preview_surface(ui, self.selected_item()),
                }
            });
        });
    }
}

impl eframe::App for GhosttyShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_updates();
        ui.ctx().request_repaint_after(Duration::from_millis(250));

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

fn shell_panel(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    Frame::default()
        .fill(paper())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(16.0)
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(14.0).strong());
            ui.add_space(10.0);
            add_contents(ui);
        });
}

fn stat_card(ui: &mut egui::Ui, eyebrow: &str, title: &str, body: &str) {
    Frame::default()
        .fill(Color32::from_rgb(243, 238, 231))
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(RichText::new(eyebrow).size(11.5).color(ink_muted()));
            ui.label(RichText::new(title).size(16.0).strong());
            ui.label(RichText::new(body).size(13.5).color(ink_muted()));
        });
}

fn tag(ui: &mut egui::Ui, label: &str, value: &str) {
    Frame::default()
        .fill(Color32::from_rgb(243, 238, 231))
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(label).size(12.0).color(ink_muted()));
                ui.label(RichText::new(value).size(12.0).strong());
            });
        });
}

fn empty_state(ui: &mut egui::Ui, title: &str, body: &str) {
    Frame::default()
        .fill(Color32::from_rgb(243, 238, 231))
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(15.0).strong());
            ui.label(RichText::new(body).size(13.5).color(ink_muted()));
        });
}

fn terminal_surface(ui: &mut egui::Ui, git: &GitSnapshot) {
    Frame::default()
        .fill(ink())
        .stroke(Stroke::new(1.0, Color32::from_rgb(64, 62, 58)))
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
                        .color(Color32::from_rgb(243, 238, 231)),
                );
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Shell panes are live. Ghostty surface wiring targets the latest upstream repo next.")
                        .size(15.0)
                        .color(Color32::from_rgb(180, 175, 169)),
                );
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "repo {} / branch {} / files {}",
                        non_empty(&git.repo_name, "none"),
                        branch_label(git),
                        git.changes.len()
                    ))
                    .size(13.5)
                    .color(Color32::from_rgb(180, 175, 169)),
                );
            });
        });
}

fn diff_surface(ui: &mut egui::Ui, change: Option<&GitChange>) {
    let mut diff = change
        .map(|change| change.diff.clone())
        .unwrap_or_else(|| "Pick a file on the left and the diff lands here.".into());

    Frame::default()
        .fill(Color32::from_rgb(248, 245, 239))
        .stroke(Stroke::new(1.0, border()))
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
                    .strong(),
                );
            } else {
                ui.label(RichText::new("No file selected").strong());
            }
            ui.add_space(10.0);
            ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut diff)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(26)
                        .interactive(false),
                );
            });
        });
}

fn preview_surface(ui: &mut egui::Ui, change: Option<&GitChange>) {
    let mut preview = change
        .map(|change| change.preview.clone())
        .unwrap_or_else(|| "Pick a file on the left and the file preview lands here.".into());

    Frame::default()
        .fill(Color32::from_rgb(248, 245, 239))
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            if let Some(change) = change {
                ui.label(RichText::new(format!("preview {}", change.path)).strong());
            } else {
                ui.label(RichText::new("No file selected").strong());
            }
            ui.add_space(10.0);
            ScrollArea::vertical().show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut preview)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(26)
                        .interactive(false),
                );
            });
        });
}

fn configure_theme(ctx: &Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::from_rgb(252, 249, 244);
    visuals.extreme_bg_color = paper();
    visuals.window_fill = Color32::from_rgb(252, 249, 244);
    visuals.widgets.noninteractive.bg_fill = paper();
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border());
    visuals.widgets.inactive.bg_fill = paper();
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border());
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(243, 238, 231);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ink());
    visuals.widgets.active.bg_fill = Color32::from_rgb(234, 226, 216);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ink());
    visuals.selection.bg_fill = Color32::from_rgb(214, 204, 191);
    visuals.selection.stroke = Stroke::new(1.0, ink());
    visuals.override_text_color = Some(ink());
    ctx.set_visuals(visuals);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 14.0;
    ctx.set_global_style(style);
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

fn status_fill(status: &str) -> Color32 {
    if status.contains('?') {
        return Color32::from_rgb(232, 241, 249);
    }
    if status.contains('D') {
        return Color32::from_rgb(249, 232, 232);
    }
    if status.contains('A') {
        return Color32::from_rgb(233, 244, 233);
    }
    if status.contains('R') {
        return Color32::from_rgb(242, 236, 250);
    }

    Color32::from_rgb(246, 242, 237)
}

fn non_empty<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

fn paper() -> Color32 {
    Color32::from_rgb(250, 247, 241)
}

fn border() -> Color32 {
    Color32::from_rgb(211, 203, 193)
}

fn ink() -> Color32 {
    Color32::from_rgb(33, 32, 30)
}

fn ink_muted() -> Color32 {
    Color32::from_rgb(110, 106, 100)
}

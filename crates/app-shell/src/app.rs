use std::{
    env,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui::{
    self, Align, CentralPanel, Context, CornerRadius, Frame, Layout, Panel, Rect, RichText,
    ScrollArea, Stroke, TextEdit,
};

use crate::{
    ghostty::{self, ActionResult as GhosttyActionResult, GhosttyInstallation, WorkspaceRequest},
    ghostty_embed::{EmbeddedGhostty, EmbeddedGhosttySnapshot},
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
    ghostty: GhosttyPanelState,
    embedded_terminal: EmbeddedGhostty,
    ghostty_action_rx: Receiver<GhosttyActionResult>,
    ghostty_action_tx: Sender<GhosttyActionResult>,
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
        let initial_request = WorkspaceRequest::new(workspace.git.repo_root.as_deref(), &cwd);
        let ghostty_installation = ghostty::detect_installation();
        let (ghostty_action_tx, ghostty_action_rx) = mpsc::channel();

        Self {
            center_mode: CenterMode::Terminal,
            selected_path,
            themes,
            workspace_rx: spawn_workspace_worker(cwd),
            workspace,
            ghostty: GhosttyPanelState::new(ghostty_installation),
            embedded_terminal: EmbeddedGhostty::new(cc, &initial_request),
            ghostty_action_rx,
            ghostty_action_tx,
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

        while let Ok(result) = self.ghostty_action_rx.try_recv() {
            self.ghostty.busy = false;
            self.ghostty.last_message = result.summary;
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

    fn refresh_ghostty_installation(&mut self) {
        self.ghostty.installation = ghostty::detect_installation();
        self.ghostty.last_message = self.ghostty.default_message();
        self.ghostty.busy = false;
    }

    fn ghostty_request(&self) -> WorkspaceRequest {
        WorkspaceRequest::new(self.workspace.git.repo_root.as_deref(), &self.workspace.cwd)
    }

    fn embedded_snapshot(&self) -> EmbeddedGhosttySnapshot {
        self.embedded_terminal.snapshot()
    }

    fn launch_ghostty_workspace(&mut self) {
        if self.ghostty.busy {
            return;
        }

        if !self.ghostty.installation.available() {
            self.ghostty.last_message = self.ghostty.default_message();
            return;
        }

        self.ghostty.busy = true;
        self.ghostty.last_message = format!(
            "Opening Ghostty at {}...",
            self.ghostty_request().working_directory.display()
        );

        let installation = self.ghostty.installation.clone();
        let request = self.ghostty_request();
        let tx = self.ghostty_action_tx.clone();
        thread::spawn(move || {
            let _ = tx.send(ghostty::focus_or_launch_workspace(&installation, &request));
        });
    }

    fn left_column(&mut self, ui: &mut egui::Ui) {
        let theme = self.active_theme().palette.clone();

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
                        let selected = self.selected_path.as_deref() == Some(change.path.as_str());
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
    }

    fn right_column(&self, ui: &mut egui::Ui) {
        let theme = &self.active_theme().palette;
        let active_theme = self.active_theme();

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
                ui.label(
                    RichText::new("Theme source")
                        .size(12.0)
                        .color(theme.muted_text()),
                );
                ui.monospace(directory.display().to_string());
                ui.add_space(8.0);
            }

            ui.label(
                RichText::new("Watching")
                    .size(12.0)
                    .color(theme.muted_text()),
            );
            ui.monospace(self.workspace.usage.source_path.display().to_string());
        });
    }

    #[allow(deprecated)]
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let theme = self.active_theme().palette.clone();

        Panel::top("header")
            .resizable(false)
            .exact_size(64.0)
            .show(ui.ctx(), |ui| {
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
                                .corner_radius(CornerRadius::same(255));

                        if ui.add(button).clicked() && self.themes.set_active_by_slug(&entry.slug) {
                            configure_theme(ui.ctx(), &self.active_theme().palette);
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let embedded = self.embedded_snapshot();
                        tag(
                            ui,
                            &theme,
                            "Ghostty",
                            embedded
                                .version
                                .as_deref()
                                .or(self.ghostty.installation.version.as_deref())
                                .unwrap_or("not detected"),
                        );
                        tag(ui, &theme, "Usage", "Live JSONL");
                    });
                });
            });
    }

    #[allow(deprecated)]
    fn bottom_bar(&self, ui: &mut egui::Ui) {
        let theme = &self.active_theme().palette;

        Panel::bottom("status")
            .resizable(false)
            .exact_size(42.0)
            .show(ui.ctx(), |ui| {
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

    fn center_column(&mut self, ui: &mut egui::Ui, frame: &eframe::Frame) {
        let active_theme = self.active_theme().clone();
        let theme = active_theme.palette.clone();

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

            if self.center_mode != CenterMode::Terminal {
                self.embedded_terminal
                    .sync(frame, Rect::NOTHING, false, &self.ghostty_request());
            }

            match self.center_mode {
                CenterMode::Terminal => {
                    self.terminal_surface(ui, frame, &theme, &active_theme.name)
                }
                CenterMode::Diff => diff_surface(ui, self.selected_item(), &theme),
                CenterMode::Preview => preview_surface(ui, self.selected_item(), &theme),
            }
        });
    }

    fn terminal_surface(
        &mut self,
        ui: &mut egui::Ui,
        frame: &eframe::Frame,
        theme: &ThemePalette,
        theme_name: &str,
    ) {
        if self.embedded_terminal.available() {
            let request = self.ghostty_request();
            let snapshot = self.embedded_snapshot();
            let desired_size = egui::vec2(ui.available_width(), 540.0);
            let (rect, _) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

            ui.painter().rect(
                rect,
                CornerRadius::same(18),
                theme.terminal_bg(),
                Stroke::new(1.0, theme.strong_border()),
                egui::StrokeKind::Outside,
            );

            self.embedded_terminal
                .sync(frame, rect.shrink(1.0), true, &request);

            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                tag(ui, theme, snapshot.backend_label, theme_name);
                if let Some(version) = snapshot.version.as_deref() {
                    tag(ui, theme, "Version", version);
                }
                tag(
                    ui,
                    theme,
                    "PWD",
                    &request.working_directory.display().to_string(),
                );
            });
            ui.label(
                RichText::new(snapshot.message)
                    .size(13.0)
                    .color(theme.muted_text()),
            );
            return;
        }

        let request = self.ghostty_request();
        let ghostty = self.ghostty.clone();

        Frame::default()
            .fill(theme.terminal_bg())
            .stroke(Stroke::new(1.0, theme.strong_border()))
            .corner_radius(CornerRadius::same(18))
            .inner_margin(22.0)
            .show(ui, |ui| {
                ui.set_min_height(540.0);
                ui.with_layout(Layout::top_down(Align::Center), |ui| {
                    ui.add_space(90.0);
                    ui.label(
                        RichText::new("GHOSTTY")
                            .size(28.0)
                            .strong()
                            .color(theme.selection_foreground),
                    );
                    ui.add_space(14.0);
                    ui.label(
                        RichText::new("Real bridge is live. This shell can focus an existing Ghostty repo window or open a new one.")
                            .size(15.0)
                            .color(theme.muted_text()),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!(
                            "{} / {} / theme {}",
                            ghostty.installation.launch_mode.label(),
                            ghostty
                                .installation
                                .version
                                .as_deref()
                                .unwrap_or("version unknown"),
                            theme_name
                        ))
                        .size(13.5)
                        .color(theme.muted_text()),
                    );
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!(
                            "repo {} / cwd {}",
                            request.repo_root.display(),
                            request.working_directory.display()
                        ))
                        .size(13.0)
                        .color(theme.muted_text()),
                    );
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        let open_button = egui::Button::new(
                            RichText::new(if ghostty.busy {
                                "Opening..."
                            } else {
                                "Open / Focus Repo Window"
                            })
                            .size(14.0),
                        )
                        .fill(theme.selected_fill())
                        .stroke(Stroke::new(1.0, theme.strong_border()))
                        .corner_radius(CornerRadius::same(10));

                        if ui
                            .add_enabled(
                                ghostty.installation.available() && !ghostty.busy,
                                open_button,
                            )
                            .clicked()
                        {
                            self.launch_ghostty_workspace();
                        }

                        let rescan_button =
                            egui::Button::new(RichText::new("Rescan Ghostty").size(14.0))
                                .fill(theme.card_bg())
                                .stroke(Stroke::new(1.0, theme.border()))
                                .corner_radius(CornerRadius::same(10));

                        if ui.add_enabled(!ghostty.busy, rescan_button).clicked() {
                            self.refresh_ghostty_installation();
                        }
                    });
                    ui.add_space(18.0);
                    stat_card(
                        ui,
                        theme,
                        "Install",
                        ghostty.installation.launch_mode.label(),
                        &ghostty.installation.location_label(),
                    );
                    ui.add_space(10.0);
                    stat_card(ui, theme, "Status", "Last action", &ghostty.last_message);
                });
            });
    }
}

impl eframe::App for GhosttyShellApp {
    #[allow(deprecated)]
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.drain_updates();
        ui.ctx().request_repaint_after(Duration::from_millis(250));

        self.top_bar(ui);
        self.bottom_bar(ui);

        CentralPanel::default().show(ui.ctx(), |ui| {
            let available_width = ui.available_width();
            let height = ui.available_height();
            let gap = 14.0;
            let left_width = 290.0f32.min((available_width * 0.24).max(220.0));
            let right_width = 340.0f32.min((available_width * 0.26).max(260.0));
            let center_width = (available_width - left_width - right_width - gap * 2.0).max(360.0);

            ui.spacing_mut().item_spacing.x = gap;

            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left_width, height),
                    Layout::top_down(Align::Min),
                    |ui| self.left_column(ui),
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(center_width, height),
                    Layout::top_down(Align::Min),
                    |ui| self.center_column(ui, frame),
                );
                ui.allocate_ui_with_layout(
                    egui::vec2(right_width, height),
                    Layout::top_down(Align::Min),
                    |ui| self.right_column(ui),
                );
            });
        });
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

#[derive(Clone, Debug)]
struct GhosttyPanelState {
    installation: GhosttyInstallation,
    last_message: String,
    busy: bool,
}

impl GhosttyPanelState {
    fn new(installation: GhosttyInstallation) -> Self {
        let last_message = default_ghostty_message(&installation);
        Self {
            installation,
            last_message,
            busy: false,
        }
    }

    fn default_message(&self) -> String {
        default_ghostty_message(&self.installation)
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

fn default_ghostty_message(installation: &GhosttyInstallation) -> String {
    if installation.available() {
        format!(
            "{} ready at {}.",
            installation.launch_mode.label(),
            installation.location_label()
        )
    } else {
        "Ghostty was not detected. Install it or set GHOSTTY_APP/GHOSTTY_BIN.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_label_plain_branch() {
        let snapshot = GitSnapshot {
            branch: "main".into(),
            ..Default::default()
        };

        assert_eq!(branch_label(&snapshot), "main");
    }

    #[test]
    fn branch_label_ahead_branch() {
        let snapshot = GitSnapshot {
            branch: "main".into(),
            ahead: 2,
            ..Default::default()
        };

        assert_eq!(branch_label(&snapshot), "main +2");
    }

    #[test]
    fn branch_label_behind_branch() {
        let snapshot = GitSnapshot {
            branch: "main".into(),
            behind: 3,
            ..Default::default()
        };

        assert_eq!(branch_label(&snapshot), "main -3");
    }

    #[test]
    fn branch_label_ahead_and_behind_branch() {
        let snapshot = GitSnapshot {
            branch: "main".into(),
            ahead: 2,
            behind: 1,
            ..Default::default()
        };

        assert_eq!(branch_label(&snapshot), "main +2/-1");
    }

    #[test]
    fn non_empty_keeps_existing_value() {
        assert_eq!(non_empty("tokyo-night", "fallback"), "tokyo-night");
    }

    #[test]
    fn non_empty_uses_fallback_for_blank_value() {
        assert_eq!(non_empty("   ", "fallback"), "fallback");
    }

    #[test]
    fn center_mode_terminal_title() {
        assert_eq!(CenterMode::Terminal.title(), "Terminal");
    }

    #[test]
    fn center_mode_diff_title() {
        assert_eq!(CenterMode::Diff.title(), "Diff");
    }

    #[test]
    fn center_mode_preview_title() {
        assert_eq!(CenterMode::Preview.title(), "Preview");
    }

    #[test]
    fn default_ghostty_message_reports_missing_install() {
        let install = GhosttyInstallation {
            app_path: None,
            binary_path: None,
            version: None,
            launch_mode: ghostty::LaunchMode::Missing,
        };

        assert!(default_ghostty_message(&install).contains("not detected"));
    }

    #[test]
    fn default_ghostty_message_reports_ready_install() {
        let install = GhosttyInstallation {
            app_path: Some(PathBuf::from("/Applications/Ghostty.app")),
            binary_path: Some(PathBuf::from(
                "/Applications/Ghostty.app/Contents/MacOS/ghostty",
            )),
            version: Some("Ghostty 1.3.1".into()),
            launch_mode: ghostty::LaunchMode::AppleScript,
        };

        assert!(default_ghostty_message(&install).contains("ready"));
        assert!(default_ghostty_message(&install).contains("/Applications/Ghostty.app"));
    }
}

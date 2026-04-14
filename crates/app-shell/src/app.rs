use eframe::egui::{
    self, Align, CentralPanel, Color32, Context, CornerRadius, Frame, Layout, Panel, RichText,
    ScrollArea, Stroke, TextEdit,
};

pub struct GhosttyShellApp {
    center_mode: CenterMode,
    selected_change: usize,
    workspace: DemoWorkspace,
}

impl GhosttyShellApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);

        Self {
            center_mode: CenterMode::Terminal,
            selected_change: 0,
            workspace: DemoWorkspace::demo(),
        }
    }

    fn selected_item(&self) -> &DemoChange {
        self.workspace
            .changes
            .get(self.selected_change)
            .unwrap_or(&self.workspace.changes[0])
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        Panel::left("repo_changes")
            .resizable(true)
            .default_size(260.0)
            .min_size(220.0)
            .show_inside(ui, |ui| {
                shell_panel(ui, "Tabs", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        tag(ui, "Repo", &self.workspace.repo_name);
                        tag(ui, "Branch", &self.workspace.branch);
                    });
                });

                ui.add_space(10.0);

                shell_panel(ui, "Changed Files", |ui| {
                    ui.label(
                        RichText::new(format!("{} tracked changes", self.workspace.changes.len()))
                            .color(ink_muted()),
                    );
                    ui.add_space(8.0);

                    ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for (index, change) in self.workspace.changes.iter().enumerate() {
                                let selected = index == self.selected_change;
                                let status = format!("{}  +{} -{}", change.status, change.added, change.removed);
                                let button = egui::Button::new(
                                    RichText::new(format!("{}\n{}", change.path, status)).size(13.5),
                                )
                                .selected(selected)
                                .fill(if selected {
                                    Color32::from_rgb(234, 226, 216)
                                } else {
                                    paper()
                                })
                                .stroke(Stroke::new(
                                    1.0,
                                    if selected { ink() } else { border() },
                                ))
                                .corner_radius(CornerRadius::same(10));

                                if ui.add_sized([ui.available_width(), 46.0], button).clicked() {
                                    self.selected_change = index;
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
            .default_size(300.0)
            .min_size(260.0)
            .show_inside(ui, |ui| {
                shell_panel(ui, "Model Usage", |ui| {
                    ui.label(
                        RichText::new("Context tracking and model usage live here.")
                            .color(ink_muted()),
                    );
                    ui.add_space(8.0);

                    for summary in &self.workspace.usage {
                        stat_card(
                            ui,
                            &summary.provider,
                            &summary.model,
                            &format!(
                                "{} in / {} out / ${:.02}",
                                summary.input_tokens, summary.output_tokens, summary.cost_usd
                            ),
                        );
                        ui.add_space(8.0);
                    }
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
                        tag(ui, "Context", "Pinned");
                        tag(ui, "Surface", "Ghostty session / file viewer");
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
                    ui.label(format!("repo {}", self.workspace.repo_name));
                    ui.separator();
                    ui.label(format!("branch {}", self.workspace.branch));
                    ui.separator();
                    ui.label(format!("cwd {}", self.workspace.cwd));
                    ui.separator();
                    ui.label(format!("file {}", self.selected_item().path));
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
                                if selected { Color32::from_rgb(33, 32, 30) } else { border() },
                            ))
                            .corner_radius(CornerRadius::same(9));

                        if ui.add(button).clicked() {
                            self.center_mode = mode;
                        }
                    }
                });

                ui.add_space(12.0);

                match self.center_mode {
                    CenterMode::Terminal => terminal_surface(ui),
                    CenterMode::Diff => diff_surface(ui, self.selected_item()),
                    CenterMode::Preview => preview_surface(ui, self.selected_item()),
                }
            });
        });
    }
}

impl eframe::App for GhosttyShellApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

struct DemoWorkspace {
    repo_name: String,
    branch: String,
    cwd: String,
    changes: Vec<DemoChange>,
    usage: Vec<DemoUsage>,
}

impl DemoWorkspace {
    fn demo() -> Self {
        Self {
            repo_name: "ghostty-shell".into(),
            branch: "main".into(),
            cwd: "/Users/kirill/Dev/ghostty-".into(),
            changes: vec![
                DemoChange {
                    path: "src/app.rs".into(),
                    status: "M".into(),
                    added: 82,
                    removed: 16,
                    diff: SAMPLE_APP_DIFF.into(),
                    preview: SAMPLE_APP_PREVIEW.into(),
                },
                DemoChange {
                    path: "src/git.rs".into(),
                    status: "A".into(),
                    added: 44,
                    removed: 0,
                    diff: SAMPLE_GIT_DIFF.into(),
                    preview: SAMPLE_GIT_PREVIEW.into(),
                },
                DemoChange {
                    path: "README.md".into(),
                    status: "M".into(),
                    added: 21,
                    removed: 4,
                    diff: SAMPLE_README_DIFF.into(),
                    preview: SAMPLE_README_PREVIEW.into(),
                },
            ],
            usage: vec![
                DemoUsage {
                    provider: "OpenAI".into(),
                    model: "gpt-5.4".into(),
                    input_tokens: 14_220,
                    output_tokens: 3_118,
                    cost_usd: 1.82,
                },
                DemoUsage {
                    provider: "Anthropic".into(),
                    model: "claude-sonnet".into(),
                    input_tokens: 9_448,
                    output_tokens: 2_004,
                    cost_usd: 1.11,
                },
            ],
        }
    }
}

struct DemoChange {
    path: String,
    status: String,
    added: usize,
    removed: usize,
    diff: String,
    preview: String,
}

struct DemoUsage {
    provider: String,
    model: String,
    input_tokens: usize,
    output_tokens: usize,
    cost_usd: f32,
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

fn terminal_surface(ui: &mut egui::Ui) {
    Frame::default()
        .fill(ink())
        .stroke(Stroke::new(1.0, Color32::from_rgb(64, 62, 58)))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(22.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(150.0);
                ui.label(
                    RichText::new("GHOSTTY")
                        .size(28.0)
                        .strong()
                        .color(Color32::from_rgb(243, 238, 231)),
                );
                ui.add_space(14.0);
                ui.label(
                    RichText::new("Terminal embedding lands next. The shell layout is already in place.")
                        .size(15.0)
                        .color(Color32::from_rgb(180, 175, 169)),
                );
            });
        });
}

fn diff_surface(ui: &mut egui::Ui, change: &DemoChange) {
    let mut diff = change.diff.clone();

    Frame::default()
        .fill(Color32::from_rgb(248, 245, 239))
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            ui.label(
                RichText::new(format!("{}  {}  +{} -{}", change.status, change.path, change.added, change.removed))
                    .strong(),
            );
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

fn preview_surface(ui: &mut egui::Ui, change: &DemoChange) {
    let mut preview = change.preview.clone();

    Frame::default()
        .fill(Color32::from_rgb(248, 245, 239))
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_height(540.0);
            ui.label(RichText::new(format!("preview {}", change.path)).strong());
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

const SAMPLE_APP_DIFF: &str = r#"diff --git a/src/app.rs b/src/app.rs
@@ -1,5 +1,16 @@
-fn render() {}
+fn render_shell(ui: &mut Ui) {
+    render_left_rail(ui);
+    render_center_surface(ui);
+    render_right_rail(ui);
+}
+
+fn render_center_surface(ui: &mut Ui) {
+    render_ghostty_placeholder(ui);
+}
"#;

const SAMPLE_GIT_DIFF: &str = r#"diff --git a/src/git.rs b/src/git.rs
+pub fn scan_repository(root: &Path) -> GitSnapshot {
+    let output = Command::new("git")
+        .args(["status", "--porcelain=v2"])
+        .current_dir(root)
+        .output()
+        .expect("git status");
+    parse_status(output.stdout)
+}
"#;

const SAMPLE_README_DIFF: &str = r#"diff --git a/README.md b/README.md
+## Ghostty Shell
+
+Three-pane shell around Ghostty:
+- left changed files
+- center terminal / diff / preview
+- right context + model usage
"#;

const SAMPLE_APP_PREVIEW: &str = r#"use eframe::egui::{self, CentralPanel, SidePanel, TopBottomPanel};

pub fn render_shell(ctx: &egui::Context) {
    TopBottomPanel::top("header").show(ctx, |_ui| {});
    SidePanel::left("changes").show(ctx, |_ui| {});
    SidePanel::right("usage").show(ctx, |_ui| {});
    CentralPanel::default().show(ctx, |_ui| {});
}
"#;

const SAMPLE_GIT_PREVIEW: &str = r#"use std::path::Path;
use std::process::Command;

pub fn repo_root(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()?;
    String::from_utf8(output.stdout).ok()
}
"#;

const SAMPLE_README_PREVIEW: &str = r#"# Ghostty Shell

The terminal stays in the center.
Repo awareness lives on the left.
Usage and context telemetry live on the right.
"#;

use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    AppleScript,
    Cli,
    Missing,
}

impl LaunchMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::AppleScript => "AppleScript bridge",
            Self::Cli => "CLI bridge",
            Self::Missing => "Unavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GhosttyInstallation {
    pub app_path: Option<PathBuf>,
    pub binary_path: Option<PathBuf>,
    pub version: Option<String>,
    pub launch_mode: LaunchMode,
}

impl GhosttyInstallation {
    pub fn available(&self) -> bool {
        self.launch_mode != LaunchMode::Missing
    }

    pub fn location_label(&self) -> String {
        self.app_path
            .as_deref()
            .or(self.binary_path.as_deref())
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not installed".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRequest {
    pub repo_root: PathBuf,
    pub working_directory: PathBuf,
}

impl WorkspaceRequest {
    pub fn new(repo_root: Option<&Path>, cwd: &Path) -> Self {
        Self {
            repo_root: repo_root.unwrap_or(cwd).to_path_buf(),
            working_directory: cwd.to_path_buf(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionResult {
    pub ok: bool,
    pub summary: String,
}

impl ActionResult {
    fn ok(summary: impl Into<String>) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
        }
    }

    fn error(summary: impl Into<String>) -> Self {
        Self {
            ok: false,
            summary: summary.into(),
        }
    }
}

pub fn detect_installation() -> GhosttyInstallation {
    detect_installation_with(
        env::var_os("HOME"),
        env::var_os("GHOSTTY_APP"),
        env::var_os("GHOSTTY_BIN"),
        env::var_os("PATH"),
    )
}

pub fn focus_or_launch_workspace(
    installation: &GhosttyInstallation,
    request: &WorkspaceRequest,
) -> ActionResult {
    match installation.launch_mode {
        LaunchMode::AppleScript => {
            #[cfg(target_os = "macos")]
            {
                match focus_or_launch_with_applescript(request) {
                    Ok(result) => result,
                    Err(error) => {
                        if let Some(app_path) = installation.app_path.as_deref() {
                            match launch_with_open(app_path, request) {
                                Ok(result) => ActionResult::ok(format!(
                                    "{} AppleScript failed first, so I fell back to `open`.",
                                    result.summary
                                )),
                                Err(fallback_error) => ActionResult::error(format!(
                                    "Ghostty automation failed ({error}). Fallback launch failed too ({fallback_error})."
                                )),
                            }
                        } else {
                            ActionResult::error(format!(
                                "Ghostty automation failed and no app bundle fallback exists: {error}"
                            ))
                        }
                    }
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                ActionResult::error(
                    "AppleScript integration is macOS-only, so this install mode is useless here.",
                )
            }
        }
        LaunchMode::Cli => {
            if let Some(binary_path) = installation.binary_path.as_deref() {
                match launch_with_cli(binary_path, request) {
                    Ok(result) => result,
                    Err(error) => ActionResult::error(format!(
                        "Ghostty CLI launch failed for {}: {error}",
                        binary_path.display()
                    )),
                }
            } else {
                ActionResult::error(
                    "Ghostty says it has a CLI bridge, but no binary path was found.",
                )
            }
        }
        LaunchMode::Missing => ActionResult::error(
            "Ghostty is not installed. Set GHOSTTY_APP or GHOSTTY_BIN if it lives somewhere weird.",
        ),
    }
}

fn detect_installation_with(
    home: Option<OsString>,
    app_override: Option<OsString>,
    bin_override: Option<OsString>,
    path_env: Option<OsString>,
) -> GhosttyInstallation {
    let home = home.map(PathBuf::from);
    let app_override = app_override.map(PathBuf::from);
    let bin_override = bin_override.map(PathBuf::from);

    for app_path in candidate_app_paths(home.as_deref(), app_override.as_deref()) {
        if !app_path.exists() {
            continue;
        }

        let binary_path = bin_override
            .clone()
            .filter(|path| path.exists())
            .or_else(|| macos_app_binary(&app_path).filter(|path| path.exists()));

        return GhosttyInstallation {
            version: binary_path
                .as_deref()
                .and_then(|path| read_version(path).ok()),
            app_path: Some(app_path),
            binary_path,
            launch_mode: LaunchMode::AppleScript,
        };
    }

    let binary_path = bin_override
        .filter(|path| path.exists())
        .or_else(|| lookup_path_binary(path_env.as_deref(), "ghostty"));

    if let Some(binary_path) = binary_path {
        return GhosttyInstallation {
            version: read_version(&binary_path).ok(),
            app_path: None,
            binary_path: Some(binary_path),
            launch_mode: LaunchMode::Cli,
        };
    }

    GhosttyInstallation {
        app_path: None,
        binary_path: None,
        version: None,
        launch_mode: LaunchMode::Missing,
    }
}

fn candidate_app_paths(home: Option<&Path>, app_override: Option<&Path>) -> Vec<PathBuf> {
    if let Some(path) = app_override {
        return vec![path.to_path_buf()];
    }

    let mut candidates = Vec::new();
    candidates.push(PathBuf::from("/Applications/Ghostty.app"));

    if let Some(home) = home {
        candidates.push(home.join("Applications/Ghostty.app"));
    }

    candidates
}

fn macos_app_binary(app_path: &Path) -> Option<PathBuf> {
    if app_path.extension().and_then(|ext| ext.to_str()) != Some("app") {
        return None;
    }

    Some(app_path.join("Contents/MacOS/ghostty"))
}

fn lookup_path_binary(path_env: Option<&OsStr>, name: &str) -> Option<PathBuf> {
    let path_env = path_env?;

    for entry in env::split_paths(path_env) {
        let candidate = entry.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn read_version(binary_path: &Path) -> Result<String, String> {
    let output = Command::new(binary_path)
        .arg("+version")
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        parse_version_output(&String::from_utf8_lossy(&output.stdout))
            .ok_or_else(|| "missing version output".into())
    } else {
        Err(stderr_or_stdout(&output))
    }
}

#[cfg(target_os = "macos")]
fn focus_or_launch_with_applescript(request: &WorkspaceRequest) -> Result<ActionResult, String> {
    let mut command = Command::new("osascript");
    for line in apple_script_lines() {
        command.arg("-e").arg(line);
    }
    command
        .arg("--")
        .arg(request.repo_root.display().to_string())
        .arg(request.working_directory.display().to_string());

    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(stderr_or_stdout(&output));
    }

    Ok(parse_apple_script_status(
        request,
        &String::from_utf8_lossy(&output.stdout),
    ))
}

#[cfg(target_os = "macos")]
fn launch_with_open(app_path: &Path, request: &WorkspaceRequest) -> Result<ActionResult, String> {
    let output = Command::new("open")
        .arg("-na")
        .arg(app_path)
        .arg("--args")
        .args(open_args(request))
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(ActionResult::ok(format!(
            "Opened Ghostty at {}.",
            request.working_directory.display()
        )))
    } else {
        Err(stderr_or_stdout(&output))
    }
}

fn launch_with_cli(binary_path: &Path, request: &WorkspaceRequest) -> Result<ActionResult, String> {
    let child = Command::new(binary_path)
        .args(cli_args(request))
        .spawn()
        .map_err(|error| error.to_string())?;

    Ok(ActionResult::ok(format!(
        "Launched Ghostty from {} with pid {}.",
        binary_path.display(),
        child.id()
    )))
}

#[cfg(target_os = "macos")]
fn apple_script_lines() -> &'static [&'static str] {
    &[
        "on run argv",
        "set repoRoot to item 1 of argv",
        "set workingDir to item 2 of argv",
        "set repoPrefix to repoRoot & \"/\"",
        "tell application \"Ghostty\"",
        "activate",
        "set matches to every terminal whose working directory is repoRoot",
        "if (count of matches) = 0 then",
        "set matches to every terminal whose working directory starts with repoPrefix",
        "end if",
        "if (count of matches) > 0 then",
        "set term1 to item 1 of matches",
        "focus term1",
        "return \"focused\"",
        "end if",
        "set cfg to new surface configuration",
        "set initial working directory of cfg to workingDir",
        "set win to new window with configuration cfg",
        "return \"launched\"",
        "end tell",
        "end run",
    ]
}

#[cfg(target_os = "macos")]
fn open_args(request: &WorkspaceRequest) -> Vec<String> {
    vec![
        format!(
            "--working-directory={}",
            request.working_directory.display()
        ),
        "--window-inherit-working-directory=false".into(),
    ]
}

fn cli_args(request: &WorkspaceRequest) -> Vec<String> {
    vec![
        format!(
            "--working-directory={}",
            request.working_directory.display()
        ),
        "--window-inherit-working-directory=false".into(),
    ]
}

fn parse_version_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn parse_apple_script_status(request: &WorkspaceRequest, output: &str) -> ActionResult {
    match output.trim() {
        "focused" => ActionResult::ok(format!(
            "Focused an existing Ghostty repo window for {}.",
            request.repo_root.display()
        )),
        "launched" => ActionResult::ok(format!(
            "Opened a new Ghostty repo window at {}.",
            request.working_directory.display()
        )),
        other if other.is_empty() => ActionResult::ok(format!(
            "Triggered Ghostty for {}.",
            request.working_directory.display()
        )),
        other => ActionResult::ok(format!("Ghostty replied with `{other}`.")),
    }
}

fn stderr_or_stdout(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stdout.is_empty() {
        return stdout;
    }

    format!("process exited with {}", output.status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    use tempfile::TempDir;

    #[test]
    fn workspace_request_uses_repo_root_for_matching() {
        let request =
            WorkspaceRequest::new(Some(Path::new("/repo")), Path::new("/repo/crates/app"));

        assert_eq!(request.repo_root, PathBuf::from("/repo"));
        assert_eq!(request.working_directory, PathBuf::from("/repo/crates/app"));
    }

    #[test]
    fn workspace_request_falls_back_to_cwd_without_repo_root() {
        let request = WorkspaceRequest::new(None, Path::new("/tmp/demo"));

        assert_eq!(request.repo_root, PathBuf::from("/tmp/demo"));
        assert_eq!(request.working_directory, PathBuf::from("/tmp/demo"));
    }

    #[test]
    fn candidate_app_paths_use_override_as_authority() {
        let home = Path::new("/Users/demo");
        let override_path = Path::new("/tmp/Ghostty.app");
        let candidates = candidate_app_paths(Some(home), Some(override_path));

        assert_eq!(candidates, vec![override_path.to_path_buf()]);
    }

    #[test]
    fn candidate_app_paths_include_defaults_without_override() {
        let home = Path::new("/Users/demo");
        let candidates = candidate_app_paths(Some(home), None);

        assert!(candidates.contains(&PathBuf::from("/Applications/Ghostty.app")));
        assert!(candidates.contains(&home.join("Applications/Ghostty.app")));
    }

    #[test]
    fn macos_app_binary_appends_bundle_binary_location() {
        assert_eq!(
            macos_app_binary(Path::new("/Applications/Ghostty.app")).as_deref(),
            Some(Path::new(
                "/Applications/Ghostty.app/Contents/MacOS/ghostty"
            ))
        );
    }

    #[test]
    fn parse_version_output_uses_first_non_empty_line() {
        assert_eq!(
            parse_version_output("\nGhostty 1.3.1\nVersion\n"),
            Some("Ghostty 1.3.1".into())
        );
    }

    #[test]
    fn parse_apple_script_status_reads_focused_reply() {
        let result = parse_apple_script_status(
            &WorkspaceRequest::new(Some(Path::new("/repo")), Path::new("/repo")),
            "focused\n",
        );

        assert!(result.ok);
        assert!(result.summary.contains("Focused"));
    }

    #[test]
    fn parse_apple_script_status_reads_launched_reply() {
        let result = parse_apple_script_status(
            &WorkspaceRequest::new(Some(Path::new("/repo")), Path::new("/repo/crates")),
            "launched\n",
        );

        assert!(result.ok);
        assert!(result.summary.contains("/repo/crates"));
    }

    #[test]
    fn parse_apple_script_status_handles_unknown_reply() {
        let result = parse_apple_script_status(
            &WorkspaceRequest::new(Some(Path::new("/repo")), Path::new("/repo")),
            "bizarre\n",
        );

        assert_eq!(result.summary, "Ghostty replied with `bizarre`.");
    }

    #[test]
    fn stderr_or_stdout_prefers_stderr() {
        let output = std::process::Output {
            status: exit_status(1),
            stdout: b"stdout".to_vec(),
            stderr: b"stderr".to_vec(),
        };

        assert_eq!(stderr_or_stdout(&output), "stderr");
    }

    #[test]
    fn lookup_path_binary_finds_executable_in_path() {
        let temp = TempDir::new().expect("temp dir");
        let bin = temp.path().join("ghostty");
        write_fake_binary(&bin, "#!/bin/sh\nexit 0\n");

        let found = lookup_path_binary(Some(&OsString::from(temp.path())), "ghostty");

        assert_eq!(found.as_deref(), Some(bin.as_path()));
    }

    #[test]
    fn detect_installation_prefers_app_bundle_when_present() {
        let temp = TempDir::new().expect("temp dir");
        let app_path = temp.path().join("Ghostty.app");
        let binary = app_path.join("Contents/MacOS/ghostty");
        fs::create_dir_all(binary.parent().expect("binary parent")).expect("bundle dirs");
        write_fake_binary(&binary, "#!/bin/sh\necho 'Ghostty 9.9.9'\n");

        let install = detect_installation_with(
            Some(OsString::from(temp.path())),
            Some(app_path.clone().into_os_string()),
            None,
            None,
        );

        assert_eq!(install.launch_mode, LaunchMode::AppleScript);
        assert_eq!(install.app_path.as_deref(), Some(app_path.as_path()));
        assert_eq!(install.binary_path.as_deref(), Some(binary.as_path()));
        assert_eq!(install.version.as_deref(), Some("Ghostty 9.9.9"));
    }

    #[test]
    fn detect_installation_falls_back_to_path_binary() {
        let temp = TempDir::new().expect("temp dir");
        let bin = temp.path().join("ghostty");
        write_fake_binary(&bin, "#!/bin/sh\necho 'Ghostty 3.2.1'\n");
        let missing_app = temp.path().join("Missing.app");

        let install = detect_installation_with(
            None,
            Some(missing_app.into_os_string()),
            None,
            Some(OsString::from(temp.path())),
        );

        assert_eq!(install.launch_mode, LaunchMode::Cli);
        assert_eq!(install.binary_path.as_deref(), Some(bin.as_path()));
        assert_eq!(install.version.as_deref(), Some("Ghostty 3.2.1"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_script_contains_focus_and_launch_logic() {
        let script = apple_script_lines().join("\n");

        assert!(script.contains("every terminal whose working directory starts with repoPrefix"));
        assert!(script.contains("set cfg to new surface configuration"));
        assert!(script.contains("set initial working directory of cfg to workingDir"));
        assert!(script.contains("new window with configuration cfg"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn open_args_include_working_directory_and_override_flag() {
        let args = open_args(&WorkspaceRequest::new(
            Some(Path::new("/repo")),
            Path::new("/repo/crates/app"),
        ));

        assert_eq!(args[0], "--working-directory=/repo/crates/app");
        assert_eq!(args[1], "--window-inherit-working-directory=false");
    }

    #[test]
    fn cli_args_include_working_directory_and_override_flag() {
        let args = cli_args(&WorkspaceRequest::new(
            Some(Path::new("/repo")),
            Path::new("/repo/crates/app"),
        ));

        assert_eq!(args[0], "--working-directory=/repo/crates/app");
        assert_eq!(args[1], "--window-inherit-working-directory=false");
    }

    fn write_fake_binary(path: &Path, body: &str) {
        fs::write(path, body).expect("fake binary");
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("permissions");
    }

    fn exit_status(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;

        std::process::ExitStatus::from_raw(code << 8)
    }
}

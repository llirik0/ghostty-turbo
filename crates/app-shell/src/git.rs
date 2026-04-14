use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Default)]
pub struct GitSnapshot {
    pub repo_root: Option<PathBuf>,
    pub repo_name: String,
    pub branch: String,
    pub ahead: usize,
    pub behind: usize,
    pub changes: Vec<GitChange>,
    pub total_added: usize,
    pub total_removed: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GitChange {
    pub path: String,
    pub status: String,
    pub added: usize,
    pub removed: usize,
    pub diff: String,
    pub preview: String,
}

pub fn load_snapshot(cwd: &Path) -> GitSnapshot {
    let cwd = cwd.to_path_buf();
    let Some(repo_root) = repo_root(&cwd) else {
        return GitSnapshot {
            error: Some("Not inside a git repository.".into()),
            ..Default::default()
        };
    };

    let mut snapshot = GitSnapshot {
        repo_name: repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repo")
            .to_owned(),
        repo_root: Some(repo_root.clone()),
        ..Default::default()
    };

    let output = run_git(
        &repo_root,
        ["status", "--short", "--branch", "--untracked-files=all"],
    );

    if !output.status.success() {
        snapshot.error = Some(stderr_or_fallback(&output.stderr, "git status failed"));
        return snapshot;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    if let Some(branch_line) = lines.next() {
        parse_branch_line(branch_line, &mut snapshot);
    }

    for line in lines {
        let Some((status, path)) = parse_status_line(line) else {
            continue;
        };

        let (added, removed) = collect_numstat(&repo_root, &path);
        let diff = collect_diff(&repo_root, &path, &status);
        let preview = read_preview(&repo_root, &path);

        snapshot.total_added += added;
        snapshot.total_removed += removed;
        snapshot.changes.push(GitChange {
            path,
            status,
            added,
            removed,
            diff,
            preview,
        });
    }

    snapshot
        .changes
        .sort_by(|left, right| left.path.to_lowercase().cmp(&right.path.to_lowercase()));

    snapshot
}

fn repo_root(cwd: &Path) -> Option<PathBuf> {
    let output = run_git(cwd, ["rev-parse", "--show-toplevel"]);
    if !output.status.success() {
        return None;
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if root.is_empty() {
        None
    } else {
        Some(PathBuf::from(root))
    }
}

fn parse_branch_line(line: &str, snapshot: &mut GitSnapshot) {
    let branch_line = line.trim_start_matches("## ").trim();

    if branch_line == "HEAD (no branch)" {
        snapshot.branch = "detached".into();
        return;
    }

    if let Some((branch, details)) = branch_line.split_once(" [") {
        snapshot.branch = branch
            .split("...")
            .next()
            .unwrap_or(branch)
            .trim()
            .to_owned();

        let details = details.trim_end_matches(']');
        for item in details.split(',') {
            let item = item.trim();
            if let Some(ahead) = item.strip_prefix("ahead ") {
                snapshot.ahead = ahead.parse().unwrap_or_default();
            }
            if let Some(behind) = item.strip_prefix("behind ") {
                snapshot.behind = behind.parse().unwrap_or_default();
            }
        }
        return;
    }

    snapshot.branch = branch_line
        .split("...")
        .next()
        .unwrap_or(branch_line)
        .trim()
        .to_owned();
}

fn parse_status_line(line: &str) -> Option<(String, String)> {
    if line.trim().is_empty() || line.starts_with("##") {
        return None;
    }

    let raw_status = line.get(..2)?.trim().replace(' ', "");
    let raw_path = line.get(3..)?.trim();
    if raw_path.is_empty() {
        return None;
    }

    let path = raw_path
        .rsplit(" -> ")
        .next()
        .unwrap_or(raw_path)
        .trim_matches('"')
        .to_owned();

    let status = if raw_status.is_empty() {
        "?".into()
    } else {
        raw_status
    };

    Some((status, path))
}

fn collect_numstat(repo_root: &Path, path: &str) -> (usize, usize) {
    let staged = parse_numstat(&String::from_utf8_lossy(
        &run_git(repo_root, ["diff", "--cached", "--numstat", "--", path]).stdout,
    ));
    let unstaged = parse_numstat(&String::from_utf8_lossy(
        &run_git(repo_root, ["diff", "--numstat", "--", path]).stdout,
    ));

    (staged.0 + unstaged.0, staged.1 + unstaged.1)
}

fn parse_numstat(output: &str) -> (usize, usize) {
    output.lines().fold((0, 0), |mut acc, line| {
        let mut parts = line.split('\t');
        let added = parts.next().unwrap_or_default();
        let removed = parts.next().unwrap_or_default();

        acc.0 += added.parse::<usize>().unwrap_or_default();
        acc.1 += removed.parse::<usize>().unwrap_or_default();
        acc
    })
}

fn collect_diff(repo_root: &Path, path: &str, status: &str) -> String {
    let staged = String::from_utf8_lossy(
        &run_git(repo_root, ["diff", "--no-ext-diff", "--cached", "--", path]).stdout,
    )
    .trim()
    .to_owned();
    let unstaged =
        String::from_utf8_lossy(&run_git(repo_root, ["diff", "--no-ext-diff", "--", path]).stdout)
            .trim()
            .to_owned();

    let mut sections = Vec::new();
    if !staged.is_empty() {
        sections.push(staged);
    }
    if !unstaged.is_empty() {
        sections.push(unstaged);
    }

    if sections.is_empty() && status.contains('?') {
        let output = Command::new("git")
            .args(["diff", "--no-index", "--", "/dev/null", path])
            .current_dir(repo_root)
            .output();

        if let Ok(output) = output {
            let diff = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !diff.is_empty() {
                sections.push(diff);
            }
        }
    }

    if sections.is_empty() {
        "No textual diff available yet.".into()
    } else {
        sections.join("\n\n")
    }
}

fn read_preview(repo_root: &Path, path: &str) -> String {
    let file_path = repo_root.join(path);
    let Ok(bytes) = fs::read(&file_path) else {
        return "File is deleted, moved, or not readable from the working tree.".into();
    };

    if bytes.contains(&0) {
        return "Binary or non-text file preview suppressed.".into();
    }

    let mut preview = String::from_utf8_lossy(&bytes).into_owned();
    const LIMIT: usize = 40_000;
    if preview.len() > LIMIT {
        preview.truncate(LIMIT);
        preview.push_str("\n\n... preview truncated ...");
    }
    preview
}

fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git {:?}: {error}", args))
}

fn stderr_or_fallback(stderr: &[u8], fallback: &str) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_owned();
    if text.is_empty() {
        fallback.into()
    } else {
        text
    }
}

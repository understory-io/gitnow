use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{app::App, git_provider::Repository, template_command};

pub struct Worktree {
    app: &'static App,
}

impl Worktree {
    pub fn new(app: &'static App) -> Self {
        Self { app }
    }

    /// Returns the project path and bare path for a repository in worktree mode.
    /// Layout: <project_path>/.bare/ for the bare clone,
    ///         <project_path>/<branch>/ for each worktree.
    pub fn paths(&self, repository: &Repository) -> (PathBuf, PathBuf) {
        let project_path = self
            .app
            .config
            .settings
            .projects
            .directory
            .join(repository.to_rel_path());
        let bare_path = project_path.join(".bare");
        (project_path, bare_path)
    }

    /// Ensures a bare clone exists at `<project_path>/.bare/`.
    /// Skips if already present.
    pub async fn ensure_bare_clone(
        &self,
        repository: &Repository,
        bare_path: &Path,
    ) -> anyhow::Result<()> {
        if bare_path.exists() {
            tracing::info!("bare clone already exists at {}", bare_path.display());
            return Ok(());
        }

        let template = self
            .app
            .config
            .settings
            .worktree
            .as_ref()
            .and_then(|w| w.clone_command.as_deref())
            .unwrap_or(template_command::DEFAULT_WORKTREE_CLONE_COMMAND);

        let bare_path_str = bare_path.display().to_string();
        let context = HashMap::from([
            ("ssh_url", repository.ssh_url.as_str()),
            ("clone_url", repository.clone_url.as_str()),
            ("bare_path", bare_path_str.as_str()),
        ]);

        tracing::info!(
            "bare-cloning {} into {}",
            repository.ssh_url.as_str(),
            bare_path.display()
        );

        let output = template_command::render_and_execute(template, context).await?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or_default();
            anyhow::bail!("failed to bare-clone: {}", stderr);
        }

        Ok(())
    }

    pub async fn list_branches(&self, bare_path: &Path) -> anyhow::Result<Vec<String>> {
        let template = self
            .app
            .config
            .settings
            .worktree
            .as_ref()
            .and_then(|w| w.list_branches_command.as_deref())
            .unwrap_or(template_command::DEFAULT_LIST_BRANCHES_COMMAND);

        let bare_path_str = bare_path.display().to_string();
        let context = HashMap::from([("bare_path", bare_path_str.as_str())]);

        let output = template_command::render_and_execute(template, context).await?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or_default();
            anyhow::bail!("failed to list branches: {}", stderr);
        }

        let stdout = std::str::from_utf8(&output.stdout)?;
        let branches: Vec<String> = stdout
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .filter(|l| !l.contains("HEAD"))
            // Strip origin/ prefix if present (for non-bare repos or custom commands)
            .map(|l| l.strip_prefix("origin/").unwrap_or(l).to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        Ok(branches)
    }

    pub async fn add_worktree(
        &self,
        bare_path: &Path,
        worktree_path: &Path,
        branch: &str,
    ) -> anyhow::Result<()> {
        let template = self
            .app
            .config
            .settings
            .worktree
            .as_ref()
            .and_then(|w| w.add_command.as_deref())
            .unwrap_or(template_command::DEFAULT_WORKTREE_ADD_COMMAND);

        let bare_path_str = bare_path.display().to_string();
        let worktree_path_str = worktree_path.display().to_string();
        let context = HashMap::from([
            ("bare_path", bare_path_str.as_str()),
            ("worktree_path", worktree_path_str.as_str()),
            ("branch", branch),
        ]);

        tracing::info!(
            "creating worktree for branch '{}' at {}",
            branch,
            worktree_path.display()
        );

        let output = template_command::render_and_execute(template, context).await?;

        if !output.status.success() {
            let stderr = std::str::from_utf8(&output.stderr).unwrap_or_default();
            anyhow::bail!("failed to create worktree: {}", stderr);
        }

        Ok(())
    }
}

pub fn sanitize_branch_name(branch: &str) -> String {
    let sanitized = branch.replace('/', "-");
    if let Some(stripped) = sanitized.strip_prefix('.') {
        format!("_{stripped}")
    } else {
        sanitized
    }
}

pub trait WorktreeApp {
    fn worktree(&self) -> Worktree;
}

impl WorktreeApp for &'static App {
    fn worktree(&self) -> Worktree {
        Worktree::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_branch_name() {
        assert_eq!(sanitize_branch_name("feature/login"), "feature-login");
        assert_eq!(sanitize_branch_name("main"), "main");
        assert_eq!(
            sanitize_branch_name("fix/nested/path"),
            "fix-nested-path"
        );
        assert_eq!(sanitize_branch_name(".hidden"), "_hidden");
    }
}

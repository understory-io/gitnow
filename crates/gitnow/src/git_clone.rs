use std::collections::HashMap;

use crate::{app::App, git_provider::Repository, template_command};

#[derive(Debug, Clone)]
pub struct GitClone {
    app: &'static App,
}

impl GitClone {
    pub fn new(app: &'static App) -> Self {
        Self { app }
    }

    pub async fn clone_repo(
        &self,
        repository: &Repository,
        force_refresh: bool,
    ) -> anyhow::Result<()> {
        let project_path = self
            .app
            .config
            .settings
            .projects
            .directory
            .join(repository.to_rel_path());

        if force_refresh {
            tokio::fs::remove_dir_all(&project_path).await?;
        }

        if project_path.exists() {
            tracing::info!(
                "project: {} already exists, skipping clone",
                repository.to_rel_path().display()
            );
            return Ok(());
        }

        let template = self
            .app
            .config
            .settings
            .clone_command
            .as_deref()
            .unwrap_or(template_command::DEFAULT_CLONE_COMMAND);

        tracing::info!(
            "cloning: {} into {}",
            repository.ssh_url.as_str(),
            &project_path.display().to_string(),
        );

        let path_str = project_path.display().to_string();
        let context = HashMap::from([
            ("ssh_url", repository.ssh_url.as_str()),
            ("path", path_str.as_str()),
        ]);

        let output = template_command::render_and_execute(template, context).await?;
        match output.status.success() {
            true => tracing::debug!(
                "cloned {} into {}",
                repository.ssh_url.as_str(),
                &project_path.display().to_string(),
            ),
            false => anyhow::bail!(
                "clone failed for {} into {} ({}): {}",
                repository.ssh_url,
                project_path.display(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        }

        Ok(())
    }
}

pub trait GitCloneApp {
    fn git_clone(&self) -> GitClone;
}

impl GitCloneApp for &'static App {
    fn git_clone(&self) -> GitClone {
        GitClone::new(self)
    }
}

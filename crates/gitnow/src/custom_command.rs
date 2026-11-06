use std::path::Path;

use crate::{
    app::App,
    config::{CommandList, PostUpdateCommandsByVcs},
};

pub struct CustomCommand {
    post_clone: Option<CommandList>,
    post_update: Option<CommandList>,
    post_update_by_vcs: PostUpdateCommandsByVcs,
}

impl CustomCommand {
    pub fn new(app: &App) -> Self {
        Self {
            post_clone: app.config.settings.post_clone_command.clone(),
            post_update: app.config.settings.post_update_command.clone(),
            post_update_by_vcs: app.config.settings.post_update_command_by_vcs.clone(),
        }
    }

    async fn execute_commands(
        commands: &CommandList,
        path: &Path,
        label: &str,
    ) -> anyhow::Result<()> {
        for command in commands.get_commands() {
            let command_parts = command.split(' ').collect::<Vec<_>>();
            let Some((first, rest)) = command_parts.split_first() else {
                return Ok(());
            };

            let mut cmd = tokio::process::Command::new(first);
            cmd.args(rest).current_dir(path).kill_on_drop(true);

            eprintln!("running command: {}", command);

            tracing::info!(
                path = path.display().to_string(),
                cmd = command,
                "running custom {} command",
                label
            );
            let output = cmd.output().await?;
            anyhow::ensure!(
                output.status.success(),
                "{} hook failed in {}: command {:?}, status {}; stderr: {}",
                label,
                path.display(),
                command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
            let stdout = std::str::from_utf8(&output.stdout)?;
            tracing::info!(stdout = stdout, "finished running custom {} command", label);
        }

        Ok(())
    }

    pub async fn execute_post_clone_command(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(ref cmds) = self.post_clone {
            Self::execute_commands(cmds, path, "post clone").await?;
        }
        Ok(())
    }

    pub async fn execute_post_update_command(&self, path: &Path) -> anyhow::Result<()> {
        let override_commands = if path.join(".jj").try_exists()? {
            &self.post_update_by_vcs.jj
        } else {
            &self.post_update_by_vcs.git
        };
        if let Some(cmds) = override_commands.as_ref().or(self.post_update.as_ref()) {
            Self::execute_commands(cmds, path, "post update").await?;
        }
        Ok(())
    }
}

pub trait CustomCommandApp {
    fn custom_command(&self) -> CustomCommand;
}

impl CustomCommandApp for App {
    fn custom_command(&self) -> CustomCommand {
        CustomCommand::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn hooks(config: &str) -> CustomCommand {
        let settings = Config::from_string(config).unwrap().settings;
        CustomCommand {
            post_clone: settings.post_clone_command,
            post_update: settings.post_update_command,
            post_update_by_vcs: settings.post_update_command_by_vcs,
        }
    }

    #[tokio::test]
    async fn update_overrides_follow_checkout_metadata_and_absence_falls_back() {
        let checkout = tempfile::tempdir().unwrap();
        let commands = hooks(
            r#"
            [settings]
            post_update_command = "false"
            post_update_command_by_vcs = { git = ["touch git-hook"], jj = ["touch jj-hook"] }
        "#,
        );
        commands
            .execute_post_update_command(checkout.path())
            .await
            .unwrap();
        assert!(checkout.path().join("git-hook").exists());
        assert!(!checkout.path().join("jj-hook").exists());
        std::fs::create_dir(checkout.path().join(".jj")).unwrap();
        commands
            .execute_post_update_command(checkout.path())
            .await
            .unwrap();
        assert!(checkout.path().join("jj-hook").exists());

        let fallback = hooks(
            r#"
            [settings]
            post_update_command = "false"
            post_update_command_by_vcs = { git = [] }
        "#,
        );
        assert!(
            fallback
                .execute_post_update_command(checkout.path())
                .await
                .is_err()
        );
        std::fs::remove_dir(checkout.path().join(".jj")).unwrap();
        fallback
            .execute_post_update_command(checkout.path())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn failed_hook_stops_the_sequence_and_keeps_the_checkout() {
        let checkout = tempfile::tempdir().unwrap();
        std::fs::write(checkout.path().join("user-file"), "keep me").unwrap();
        let commands = hooks(
            r#"
            [settings]
            post_clone_command = ["false", "touch must-not-run"]
            post_update_command = ["false", "touch must-not-run"]
        "#,
        );
        assert!(
            commands
                .execute_post_clone_command(checkout.path())
                .await
                .is_err()
        );
        assert!(
            commands
                .execute_post_update_command(checkout.path())
                .await
                .is_err()
        );
        assert!(!checkout.path().join("must-not-run").exists());
        assert_eq!(
            std::fs::read_to_string(checkout.path().join("user-file")).unwrap(),
            "keep me"
        );
    }
}

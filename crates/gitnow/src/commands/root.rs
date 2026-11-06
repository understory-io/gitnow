use std::io::IsTerminal;

use crate::{
    app::App,
    cache::{CacheApp, load_repositories},
    chooser::Chooser,
    components::progress_command::ProgressCommand,
    custom_command::CustomCommandApp,
    fuzzy_matcher::{FuzzyMatcher, FuzzyMatcherApp},
    git_clone::GitCloneApp,
    git_provider::Repository,
    interactive::InteractiveApp,
    projects_list::ProjectsListApp,
    shell::ShellApp,
};

#[derive(Debug, Clone)]
pub struct RootCommand {
    app: &'static App,
}

pub struct RootOptions {
    pub cache: bool,
    pub clone: bool,
    pub shell: bool,
    pub force_refresh: bool,
    pub force_cache_update: bool,
}

impl RootCommand {
    pub fn new(app: &'static App) -> Self {
        Self { app }
    }

    pub async fn execute(
        &mut self,
        search: Option<&str>,
        interactive: bool,
        options: RootOptions,
        chooser: &Chooser,
    ) -> anyhow::Result<()> {
        let RootOptions {
            cache,
            clone,
            shell,
            force_refresh,
            force_cache_update,
        } = options;
        tracing::debug!("executing");

        let repositories = if force_cache_update {
            tracing::info!("forcing cache update...");
            let repositories = self.app.projects_list().get_projects().await?;
            self.app.cache().update(&repositories).await?;
            repositories
        } else {
            load_repositories(self.app, cache).await?
        };

        let repo = if interactive {
            self.app
                .interactive()
                .interactive_search(&repositories, search.unwrap_or_default())?
                .ok_or(anyhow::anyhow!("failed to find a repository"))?
        } else {
            match search {
                Some(needle) => {
                    let matched_repos = self
                        .app
                        .fuzzy_matcher()
                        .match_repositories(needle, &repositories);

                    let repo = matched_repos
                        .first()
                        .ok_or(anyhow::anyhow!("failed to find repository"))?;
                    tracing::debug!("selected repo: {}", repo.to_rel_path().display());

                    repo.to_owned()
                }
                None => self
                    .app
                    .interactive()
                    .interactive_search(&repositories, "")?
                    .ok_or(anyhow::anyhow!("failed to find a repository"))?,
            }
        };

        let project_path = self
            .app
            .config
            .settings
            .projects
            .directory
            .join(repo.to_rel_path());
        if !project_path.exists() {
            if clone {
                let git_clone = self.app.git_clone();

                if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() && shell {
                    let mut wrap_cmd =
                        ProgressCommand::new(format!("cloning: {}", repo.to_rel_path().display()));
                    let repo = repo.clone();
                    wrap_cmd
                        .execute(move || async move {
                            git_clone.clone_repo(&repo, force_refresh).await?;

                            Ok(())
                        })
                        .await?;
                } else {
                    eprintln!("cloning repository...");
                    git_clone.clone_repo(&repo, force_refresh).await?;
                }

                self.app
                    .custom_command()
                    .execute_post_clone_command(&project_path)
                    .await?;
            } else {
                tracing::info!("skipping clone for repo: {}", &repo.to_rel_path().display());
            }
        } else {
            tracing::info!("repository already exists");

            self.app
                .custom_command()
                .execute_post_update_command(&project_path)
                .await?;
        }

        if shell {
            self.app.shell().spawn_shell(&repo).await?;
        } else {
            tracing::info!("skipping shell for repo: {}", &repo.to_rel_path().display());
            chooser.set(
                &self
                    .app
                    .config
                    .settings
                    .projects
                    .directory
                    .join(repo.to_rel_path()),
            )?;
        }

        Ok(())
    }
}

pub trait RepositoryMatcher {
    fn match_repositories(&self, pattern: &str, repositories: &[Repository]) -> Vec<Repository>;
}

impl RepositoryMatcher for FuzzyMatcher {
    fn match_repositories(&self, pattern: &str, repositories: &[Repository]) -> Vec<Repository> {
        let labels: Vec<String> = repositories
            .iter()
            .map(|repository| repository.to_rel_path().display().to_string())
            .collect();

        self.match_indices(pattern, &labels)
            .into_iter()
            .map(|index| repositories[index].clone())
            .collect()
    }
}

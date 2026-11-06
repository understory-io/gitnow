#[cfg(not(feature = "example"))]
pub use implementation::*;

#[cfg(feature = "example")]
pub use example_projects::*;

use crate::app::App;

pub trait ProjectsListApp {
    fn projects_list(&self) -> ProjectsList;
}

impl ProjectsListApp for &'static App {
    fn projects_list(&self) -> ProjectsList {
        ProjectsList::new(self)
    }
}

mod implementation {
    use crate::{
        app::App,
        git_provider::{
            Repository, VecRepositoryExt, gitea::GiteaProviderApp, github::GitHubProviderApp,
        },
    };

    pub struct ProjectsList {
        app: &'static App,
    }

    impl ProjectsList {
        pub fn new(app: &'static App) -> Self {
            Self { app }
        }

        pub async fn get_projects(&self) -> anyhow::Result<Vec<Repository>> {
            let mut repositories = Vec::new();

            repositories.extend(self.get_gitea_projects().await?);
            repositories.extend(self.get_github_projects().await?);

            repositories.collect_unique();

            Ok(repositories)
        }

        async fn get_gitea_projects(&self) -> anyhow::Result<Vec<Repository>> {
            let gitea_provider = self.app.gitea_provider();

            let mut repositories = Vec::new();
            for gitea in self.app.config.providers.gitea.iter() {
                if let Some(_user) = &gitea.current_user {
                    let mut repos = gitea_provider
                        .list_repositories_for_current_user(&gitea.url, gitea.access_token.as_ref())
                        .await?;

                    repositories.append(&mut repos);
                }

                for gitea_user in gitea.users.iter() {
                    let mut repos = gitea_provider
                        .list_repositories_for_user(
                            gitea_user.into(),
                            &gitea.url,
                            gitea.access_token.as_ref(),
                        )
                        .await?;

                    repositories.append(&mut repos);
                }

                for gitea_org in gitea.organisations.iter() {
                    let mut repos = gitea_provider
                        .list_repositories_for_organisation(
                            gitea_org.into(),
                            &gitea.url,
                            gitea.access_token.as_ref(),
                        )
                        .await?;

                    repositories.append(&mut repos);
                }
            }

            Ok(repositories)
        }

        async fn get_github_projects(&self) -> anyhow::Result<Vec<Repository>> {
            let github_provider = self.app.github_provider();

            let mut repositories = Vec::new();
            for github in self.app.github_providers().await.iter() {
                if let Some(_user) = &github.current_user {
                    let mut repos = github_provider
                        .list_repositories_for_current_user(
                            github.url.as_ref(),
                            &github.access_token,
                        )
                        .await?;

                    repositories.append(&mut repos);
                }

                for github_user in github.users.iter() {
                    let mut repos = github_provider
                        .list_repositories_for_user(
                            github_user.into(),
                            github.url.as_ref(),
                            &github.access_token,
                        )
                        .await?;

                    repositories.append(&mut repos);
                }

                for github_org in github.organisations.iter() {
                    let mut repos = github_provider
                        .list_repositories_for_organisation(
                            github_org.into(),
                            github.url.as_ref(),
                            &github.access_token,
                        )
                        .await?;

                    repositories.append(&mut repos);
                }
            }

            Ok(repositories)
        }
    }
}

#[cfg(feature = "example")]
mod example_projects;

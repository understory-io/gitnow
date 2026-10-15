use anyhow::Context;
use serde::Deserialize;
use url::Url;

use crate::{app::App, config::GiteaAccessToken};

#[derive(Debug, Deserialize)]
struct GiteaRepo {
    name: Option<String>,
    ssh_url: Option<String>,
    clone_url: Option<String>,
    owner: Option<GiteaUser>,
}

#[derive(Debug, Deserialize)]
struct GiteaUser {
    login: Option<String>,
}

#[derive(Debug)]
pub struct GiteaProvider {
    #[allow(dead_code)]
    app: &'static App,
    client: reqwest::Client,
}

impl GiteaProvider {
    pub fn new(app: &'static App) -> GiteaProvider {
        GiteaProvider {
            app,
            client: reqwest::Client::new(),
        }
    }

    pub async fn list_repositories_for_current_user(
        &self,
        api: &str,
        access_token: Option<&GiteaAccessToken>,
    ) -> anyhow::Result<Vec<super::Repository>> {
        tracing::debug!("fetching gitea repositories for current user");

        let mut repositories = Vec::new();
        let mut page = 1;
        loop {
            let repos: Vec<GiteaRepo> = self
                .request(&format!("{api}/user/repos"), access_token, page)
                .await?;

            if repos.is_empty() {
                break;
            }

            repositories.extend(repos);
            page += 1;
        }

        let provider = &Self::get_domain(api)?;
        Ok(to_repositories(provider, repositories))
    }

    fn get_domain(api: &str) -> anyhow::Result<String> {
        let url = Url::parse(api)?;
        let provider = url.domain().unwrap_or("gitea");

        Ok(provider.into())
    }

    pub async fn list_repositories_for_user(
        &self,
        user: &str,
        api: &str,
        access_token: Option<&GiteaAccessToken>,
    ) -> anyhow::Result<Vec<super::Repository>> {
        tracing::debug!(user = user, "fetching gitea repositories for user");

        let mut repositories = Vec::new();
        let mut page = 1;
        loop {
            let repos: Vec<GiteaRepo> = self
                .request(
                    &format!("{api}/users/{user}/repos"),
                    access_token,
                    page,
                )
                .await?;

            if repos.is_empty() {
                break;
            }

            repositories.extend(repos);
            page += 1;
        }

        let provider = &Self::get_domain(api)?;
        Ok(to_repositories(provider, repositories))
    }

    pub async fn list_repositories_for_organisation(
        &self,
        organisation: &str,
        api: &str,
        access_token: Option<&GiteaAccessToken>,
    ) -> anyhow::Result<Vec<super::Repository>> {
        tracing::debug!(
            organisation = organisation,
            "fetching gitea repositories for organisation"
        );

        let mut repositories = Vec::new();
        let mut page = 1;
        loop {
            let repos: Vec<GiteaRepo> = self
                .request(
                    &format!("{api}/orgs/{organisation}/repos"),
                    access_token,
                    page,
                )
                .await?;

            if repos.is_empty() {
                break;
            }

            repositories.extend(repos);
            page += 1;
        }

        let provider = &Self::get_domain(api)?;
        Ok(to_repositories(provider, repositories))
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        access_token: Option<&GiteaAccessToken>,
        page: usize,
    ) -> anyhow::Result<T> {
        let mut req = self.client.get(url).query(&[("page", page.to_string())]);

        match access_token {
            Some(GiteaAccessToken::Env { env }) => {
                let token =
                    std::env::var(env).context(format!("{env} didn't have a valid value"))?;
                req = req.basic_auth("", Some(token));
            }
            Some(GiteaAccessToken::Direct(var)) => {
                req = req.bearer_auth(var);
            }
            None => {}
        }

        req.send()
            .await
            .context("failed to send request")?
            .error_for_status()
            .context("request failed")?
            .json()
            .await
            .context("failed to parse response")
    }
}

fn to_repositories(provider: &str, repos: Vec<GiteaRepo>) -> Vec<super::Repository> {
    repos
        .into_iter()
        .map(|repo| super::Repository {
            provider: provider.into(),
            owner: repo
                .owner
                .map(|user| user.login.unwrap_or_default())
                .unwrap_or_default(),
            repo_name: repo.name.unwrap_or_default(),
            ssh_url: repo
                .ssh_url
                .expect("ssh url to be set for a gitea repository"),
            clone_url: repo
                .clone_url
                .expect("clone url to be set for a gitea repository"),
        })
        .collect()
}

pub trait GiteaProviderApp {
    fn gitea_provider(&self) -> GiteaProvider;
}

impl GiteaProviderApp for &'static App {
    fn gitea_provider(&self) -> GiteaProvider {
        GiteaProvider::new(self)
    }
}

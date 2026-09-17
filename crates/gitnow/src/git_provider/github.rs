use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;

use crate::{app::App, config::GitHubAccessToken};

#[derive(Deserialize)]
struct GitHubRepo {
    name: String,
    owner: Option<GitHubOwner>,
    ssh_url: Option<String>,
    clone_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubOwner {
    login: String,
}

pub struct GitHubProvider {
    #[allow(dead_code)]
    app: &'static App,
}

impl GitHubProvider {
    pub fn new(app: &'static App) -> GitHubProvider {
        GitHubProvider { app }
    }

    pub async fn list_repositories_for_current_user(
        &self,
        url: Option<&String>,
        access_token: &GitHubAccessToken,
    ) -> anyhow::Result<Vec<super::Repository>> {
        tracing::debug!("fetching github repositories for current user");

        let client = self.get_client(access_token)?;
        let base = self.api_base(url);

        let repos: Vec<GitHubRepo> = self
            .paginate(
                &client,
                &format!("{base}/user/repos?type=all&sort=full_name&per_page=100"),
            )
            .await?;

        Ok(self.to_repositories(url, repos))
    }

    pub async fn list_repositories_for_user(
        &self,
        user: &str,
        url: Option<&String>,
        access_token: &GitHubAccessToken,
    ) -> anyhow::Result<Vec<super::Repository>> {
        tracing::debug!(user = user, "fetching github repositories for user");

        let client = self.get_client(access_token)?;
        let base = self.api_base(url);

        let repos: Vec<GitHubRepo> = self
            .paginate(
                &client,
                &format!("{base}/users/{user}/repos?type=all&sort=full_name&per_page=100"),
            )
            .await?;

        Ok(self.to_repositories(url, repos))
    }

    pub async fn list_repositories_for_organisation(
        &self,
        organisation: &str,
        url: Option<&String>,
        access_token: &GitHubAccessToken,
    ) -> anyhow::Result<Vec<super::Repository>> {
        tracing::debug!(
            organisation = organisation,
            "fetching github repositories for organisation"
        );

        let client = self.get_client(access_token)?;
        let base = self.api_base(url);

        let repos: Vec<GitHubRepo> = self
            .paginate(
                &client,
                &format!("{base}/orgs/{organisation}/repos?type=all&sort=full_name&per_page=100"),
            )
            .await?;

        Ok(self.to_repositories(url, repos))
    }

    async fn paginate(
        &self,
        client: &reqwest::Client,
        initial_url: &str,
    ) -> anyhow::Result<Vec<GitHubRepo>> {
        let mut repos = Vec::new();
        let mut url = Some(initial_url.to_string());

        while let Some(current_url) = url {
            let resp = client
                .get(&current_url)
                .send()
                .await?
                .error_for_status()?;

            url = parse_next_link(resp.headers());

            let page: Vec<GitHubRepo> = resp.json().await?;
            repos.extend(page);
        }

        Ok(repos)
    }

    fn to_repositories(
        &self,
        url: Option<&String>,
        repos: Vec<GitHubRepo>,
    ) -> Vec<super::Repository> {
        repos
            .into_iter()
            .filter_map(|repo| {
                Some(super::Repository {
                    provider: self.get_url(url),
                    owner: repo.owner.map(|o| o.login)?,
                    repo_name: repo.name,
                    ssh_url: repo.ssh_url?,
                    clone_url: repo.clone_url?,
                })
            })
            .collect()
    }

    fn api_base(&self, url: Option<&String>) -> String {
        match url {
            Some(u) => format!("{u}/api/v3"),
            None => "https://api.github.com".to_string(),
        }
    }

    fn get_url(&self, url: Option<&String>) -> String {
        let default_domain = "github.com".to_string();

        if let Some(url) = url {
            let Some(url) = url::Url::parse(url).ok() else {
                return default_domain;
            };

            let Some(domain) = url.domain().map(|d| d.to_string()) else {
                return default_domain;
            };

            domain
        } else {
            default_domain
        }
    }

    fn get_client(&self, access_token: &GitHubAccessToken) -> anyhow::Result<reqwest::Client> {
        let token = match access_token {
            GitHubAccessToken::Direct(token) => token.to_owned(),
            GitHubAccessToken::Env { env } => std::env::var(env)?,
        };

        let client = reqwest::Client::builder()
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(AUTHORIZATION, format!("token {token}").parse()?);
                headers.insert(ACCEPT, "application/vnd.github+json".parse()?);
                headers.insert(USER_AGENT, "gitnow".parse()?);
                headers
            })
            .build()?;

        Ok(client)
    }
}

fn parse_next_link(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link = headers.get("link")?.to_str().ok()?;
    for part in link.split(',') {
        let part = part.trim();
        if part.ends_with("rel=\"next\"") {
            let url = part.split('>').next()?.trim_start_matches('<');
            return Some(url.to_string());
        }
    }
    None
}

pub trait GitHubProviderApp {
    fn github_provider(&self) -> GitHubProvider;
}

impl GitHubProviderApp for &'static App {
    fn github_provider(&self) -> GitHubProvider {
        GitHubProvider::new(self)
    }
}

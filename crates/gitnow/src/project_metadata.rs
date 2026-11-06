use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::git_provider::Repository;

pub const METADATA_FILENAME: &str = ".gitnow.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub version: u32,
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    pub repositories: Vec<RepoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoEntry {
    pub provider: String,
    pub owner: String,
    pub repo_name: String,
    pub ssh_url: String,
}

impl From<&Repository> for RepoEntry {
    fn from(repo: &Repository) -> Self {
        Self {
            provider: repo.provider.clone(),
            owner: repo.owner.clone(),
            repo_name: repo.repo_name.clone(),
            ssh_url: repo.ssh_url.clone(),
        }
    }
}

impl ProjectMetadata {
    pub fn new(name: String, template: Option<String>, repositories: Vec<RepoEntry>) -> Self {
        Self {
            version: 1,
            name,
            created_at: Utc::now(),
            template,
            repositories,
        }
    }

    pub fn load(project_dir: &Path) -> Option<Self> {
        let path = project_dir.join(METADATA_FILENAME);
        let content = std::fs::read_to_string(&path).ok()?;
        let metadata: Self = serde_json::from_str(&content).ok()?;
        Some(metadata)
    }

    pub fn save(&self, project_dir: &Path) -> anyhow::Result<()> {
        let path = project_dir.join(METADATA_FILENAME);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn add_repositories(&mut self, repos: Vec<RepoEntry>) {
        for repo in repos {
            if !self.repositories.iter().any(|r| r.ssh_url == repo.ssh_url) {
                self.repositories.push(repo);
            }
        }
    }

    pub fn remove_repositories(&mut self, repos: &[RepoEntry]) {
        self.repositories
            .retain(|r| !repos.iter().any(|x| x.ssh_url == r.ssh_url));
    }

    pub fn created_ago(&self) -> String {
        let duration = Utc::now().signed_duration_since(self.created_at);

        let days = duration.num_days();
        if days > 365 {
            let years = days / 365;
            return if years == 1 {
                "1 year ago".into()
            } else {
                format!("{years} years ago")
            };
        }
        if days > 30 {
            let months = days / 30;
            return if months == 1 {
                "1 month ago".into()
            } else {
                format!("{months} months ago")
            };
        }
        if days > 0 {
            return if days == 1 {
                "1 day ago".into()
            } else {
                format!("{days} days ago")
            };
        }

        let hours = duration.num_hours();
        if hours > 0 {
            return if hours == 1 {
                "1 hour ago".into()
            } else {
                format!("{hours} hours ago")
            };
        }

        let minutes = duration.num_minutes();
        if minutes > 0 {
            return if minutes == 1 {
                "1 minute ago".into()
            } else {
                format!("{minutes} minutes ago")
            };
        }

        "just now".into()
    }
}

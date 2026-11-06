use std::path::PathBuf;

use anyhow::Context;
use tokio::io::AsyncWriteExt;

use crate::{app::App, cache_codec::CacheCodecApp, config::Config, git_provider::Repository};

pub struct Cache {
    app: &'static App,
}

impl Cache {
    pub fn new(app: &'static App) -> Self {
        Self { app }
    }

    pub async fn update(&self, repositories: &[Repository]) -> anyhow::Result<()> {
        tracing::debug!(repository_len = repositories.len(), "storing repositories");

        let location = self.app.config.get_cache_file_location()?;
        tracing::trace!("found cache location: {}", location.display());

        if let Some(parent) = location.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let cache_content = self
            .app
            .cache_codec()
            .serialize_repositories(repositories)?;

        let mut cache_file = tokio::fs::File::create(location)
            .await
            .context("failed to create cache file")?;
        cache_file
            .write_all(&cache_content)
            .await
            .context("failed to write cache content to file")?;

        Ok(())
    }

    pub async fn get(&self) -> anyhow::Result<Option<Vec<Repository>>> {
        tracing::debug!("fetching repositories");

        let location = self.app.config.get_cache_file_location()?;
        if !location.exists() {
            tracing::debug!(
                location = location.display().to_string(),
                "cache doesn't exist"
            );
            return Ok(None);
        }

        if let Some(cache_duration) = self.app.config.settings.cache.duration.get_duration() {
            let metadata = tokio::fs::metadata(&location).await?;

            if let Ok(file_modified_last) = metadata
                .modified()
                .context("failed to get modified date")
                .inspect_err(|e| {
                    tracing::warn!(
                        "could not get valid metadata from file, cache will be reused: {}",
                        e
                    );
                })
                .and_then(|m| {
                    m.elapsed()
                        .context("failed to get elapsed from file")
                        .inspect_err(|e| tracing::warn!("failed to get elapsed from system: {}", e))
                })
            {
                tracing::trace!(
                    cache = file_modified_last.as_secs(),
                    expiry = cache_duration.as_secs(),
                    "checking if cache is valid"
                );
                if file_modified_last > cache_duration {
                    tracing::debug!("cache has expired");
                    return Ok(None);
                }

                tracing::debug!(
                    "cache is valid for: {} mins",
                    cache_duration.saturating_sub(file_modified_last).as_secs() / 60
                );
            }
        }

        let file = tokio::fs::read(&location).await?;
        if file.is_empty() {
            tracing::debug!("cache file appears to be empty");
            return Ok(None);
        }

        let repos = match self.app.cache_codec().deserialize_repositories(file) {
            Ok(repos) => repos,
            Err(e) => {
                tracing::warn!(error = e.to_string(), "failed to deserialize repositories");
                return Ok(None);
            }
        };

        Ok(Some(repos))
    }
}

/// Load repositories using the cache if available, otherwise fetch and update cache.
pub async fn load_repositories(
    app: &'static App,
    use_cache: bool,
) -> anyhow::Result<Vec<Repository>> {
    use crate::projects_list::ProjectsListApp;

    if use_cache && let Some(repos) = app.cache().get().await? {
        return Ok(repos);
    }

    tracing::info!("fetching repositories...");
    let repositories = app.projects_list().get_projects().await?;
    app.cache().update(&repositories).await?;
    Ok(repositories)
}

pub trait CacheApp {
    fn cache(&self) -> Cache;
}

impl CacheApp for &'static App {
    fn cache(&self) -> Cache {
        Cache::new(self)
    }
}

pub trait CacheConfig {
    fn get_cache_location(&self) -> anyhow::Result<PathBuf>;
    fn get_cache_file_location(&self) -> anyhow::Result<PathBuf>;
}

impl CacheConfig for Config {
    fn get_cache_location(&self) -> anyhow::Result<PathBuf> {
        Ok(self.settings.cache.location.clone().into())
    }

    fn get_cache_file_location(&self) -> anyhow::Result<PathBuf> {
        Ok(self.get_cache_location()?.join("cache.proto"))
    }
}

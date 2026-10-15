use std::io::Cursor;

use anyhow::Context;
use prost::Message;

use crate::{app::App, git_provider::Repository};

mod proto_codec {
    include!("gen/gitnow.v1.rs");
}

pub struct CacheCodec {}

impl CacheCodec {
    pub fn new() -> Self {
        Self {}
    }

    pub fn serialize_repositories(&self, repositories: &[Repository]) -> anyhow::Result<Vec<u8>> {
        let mut codec_repos = proto_codec::Repositories::default();

        for repo in repositories.iter().cloned() {
            codec_repos.repositories.push(proto_codec::Repository {
                provider: repo.provider,
                owner: repo.owner,
                repo_name: repo.repo_name,
                ssh_url: repo.ssh_url,
                clone_url: repo.clone_url,
            });
        }

        Ok(codec_repos.encode_to_vec())
    }

    pub fn deserialize_repositories(&self, content: Vec<u8>) -> anyhow::Result<Vec<Repository>> {
        let codex_repos = proto_codec::Repositories::decode(&mut Cursor::new(content))
            .context("failed to decode protobuf repositories")?;

        let mut repos = Vec::new();

        for codec_repo in codex_repos.repositories {
            let clone_url = if codec_repo.clone_url.is_empty() {
                format!(
                    "https://{}/{}/{}.git",
                    codec_repo.provider, codec_repo.owner, codec_repo.repo_name
                )
            } else {
                codec_repo.clone_url
            };

            repos.push(Repository {
                provider: codec_repo.provider,
                owner: codec_repo.owner,
                repo_name: codec_repo.repo_name,
                ssh_url: codec_repo.ssh_url,
                clone_url,
            });
        }

        Ok(repos)
    }
}

pub trait CacheCodecApp {
    fn cache_codec(&self) -> CacheCodec;
}

impl CacheCodecApp for &'static App {
    fn cache_codec(&self) -> CacheCodec {
        CacheCodec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> Repository {
        Repository {
            provider: "github.com".into(),
            owner: "owner".into(),
            repo_name: "repo".into(),
            ssh_url: "git@github.com:owner/repo.git".into(),
            clone_url: "https://github.com/owner/repo.git".into(),
        }
    }

    #[test]
    fn round_trip_preserves_clone_url() {
        let codec = CacheCodec::new();
        let expected = vec![repository()];

        let encoded = codec.serialize_repositories(&expected).unwrap();
        let decoded = codec.deserialize_repositories(encoded).unwrap();

        assert_eq!(decoded, expected);
    }

    #[test]
    fn legacy_cache_derives_clone_url_when_field_is_missing() {
        let codec = CacheCodec::new();
        let encoded = proto_codec::Repositories {
            repositories: vec![proto_codec::Repository {
                provider: "github.com".into(),
                owner: "owner".into(),
                repo_name: "repo".into(),
                ssh_url: "git@github.com:owner/repo.git".into(),
                clone_url: String::new(),
            }],
        }
        .encode_to_vec();

        let decoded = codec.deserialize_repositories(encoded).unwrap();

        assert_eq!(decoded[0].clone_url, "https://github.com/owner/repo.git");
    }
}

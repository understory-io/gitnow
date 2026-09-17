use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct Repository {
    pub provider: String,
    pub owner: String,
    pub repo_name: String,
    pub ssh_url: String,
    pub clone_url: String,
}

impl Repository {
    pub fn to_rel_path(&self) -> PathBuf {
        PathBuf::from(&self.provider)
            .join(&self.owner)
            .join(&self.repo_name)
    }
}

pub trait VecRepositoryExt {
    fn collect_unique(&mut self) -> &mut Self;
}

impl VecRepositoryExt for Vec<Repository> {
    fn collect_unique(&mut self) -> &mut Self {
        self.sort_by_key(|a| a.to_rel_path());
        self.dedup_by_key(|a| a.to_rel_path());

        self
    }
}

pub mod gitea;
pub mod github;

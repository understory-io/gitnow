use crate::{app::App, git_provider::Repository};

pub struct ProjectsList {}

impl ProjectsList {
    pub fn new(_app: &'static App) -> Self {
        Self {}
    }

    pub async fn get_projects(&self) -> anyhow::Result<Vec<Repository>> {
        Ok(self.from_strings([
            "github.com/kjuulh/gitnow",
            "github.com/kjuulh/gitnow-client",
            "github.com/kjuulh/crunch",
            "git.front.kjuulh.io/kjuulh/gitnow",
            "git.front.kjuulh.io/kjuulh/gitnow-client",
            "git.front.kjuulh.io/kjuulh/cuddle",
            "git.front.kjuulh.io/kjuulh/buckle",
            "git.front.kjuulh.io/kjuulh/books",
            "git.front.kjuulh.io/kjuulh/blog-deployment",
            "git.front.kjuulh.io/kjuulh/blog",
            "git.front.kjuulh.io/kjuulh/bitfield",
            "git.front.kjuulh.io/kjuulh/bitebuds-deployment",
            "git.front.kjuulh.io/kjuulh/bitebuds",
            "git.front.kjuulh.io/kjuulh/beerday",
            "git.front.kjuulh.io/kjuulh/bearing",
            "git.front.kjuulh.io/kjuulh/basic-webserver",
            "git.front.kjuulh.io/kjuulh/backup",
            "git.front.kjuulh.io/kjuulh/backstage",
            "git.front.kjuulh.io/kjuulh/autom8-calendar-integration",
            "git.front.kjuulh.io/kjuulh/astronvim",
            "git.front.kjuulh.io/kjuulh/artifacts",
            "git.front.kjuulh.io/kjuulh/articles",
            "git.front.kjuulh.io/kjuulh/acc-server",
            "git.front.kjuulh.io/kjuulh/_cargo-index",
            "git.front.kjuulh.io/keep-up/keep-up-example",
            "git.front.kjuulh.io/keep-up/keep-up",
            "git.front.kjuulh.io/experiments/wasm-bin",
            "git.front.kjuulh.io/dotfiles/doom",
            "git.front.kjuulh.io/danskebank/testssl.sh",
            "git.front.kjuulh.io/clank/kubernetes-state",
            "git.front.kjuulh.io/clank/kubernetes-init",
            "git.front.kjuulh.io/clank/blog",
            "git.front.kjuulh.io/cibus/deployments",
            "git.front.kjuulh.io/butikkaerlighilsen/client",
            "git.front.kjuulh.io/bevy/bevy",
            "git.front.kjuulh.io/OpenFood/openfood",
        ]))
    }

    fn from_strings(
        &self,
        repos_into: impl IntoIterator<Item = impl Into<Repository>>,
    ) -> Vec<Repository> {
        let repos = repos_into
            .into_iter()
            .map(|item| item.into())
            .collect::<Vec<Repository>>();

        repos
    }
}

impl From<&str> for Repository {
    fn from(value: &str) -> Self {
        let values = value.split("/").collect::<Vec<_>>();
        if values.len() != 3 {
            panic!("value: '{value}' isn't a valid provider/owner/repository")
        }

        let (provider, owner, name) = (
            values.get(0).unwrap(),
            values.get(1).unwrap(),
            values.get(2).unwrap(),
        );

        Self {
            provider: provider.to_string(),
            owner: owner.to_string(),
            repo_name: name.to_string(),
            ssh_url: format!("ssh://git@{provider}/{owner}/{name}.git"),
            clone_url: format!("https://{provider}/{owner}/{name}.git"),
        }
    }
}

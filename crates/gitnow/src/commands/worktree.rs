use std::io::IsTerminal;

use crate::{
    app::App,
    cache::load_repositories,
    chooser::Chooser,
    components::progress_command::ProgressCommand,
    fuzzy_matcher::FuzzyMatcherApp,
    interactive::{InteractiveApp, StringItem},
    shell::ShellApp,
    worktree::{WorktreeApp, sanitize_branch_name},
};

use super::root::RepositoryMatcher;

#[derive(clap::Parser)]
pub struct WorktreeCommand {
    /// Optional search string to pre-filter repositories
    #[arg()]
    search: Option<String>,

    /// Branch to check out (skips interactive branch selection)
    #[arg(long = "branch", short = 'b')]
    branch: Option<String>,

    /// Skip cache
    #[arg(long = "no-cache", default_value = "false")]
    no_cache: bool,

    /// Skip spawning a shell in the worktree
    #[arg(long = "no-shell", default_value = "false")]
    no_shell: bool,
}

impl WorktreeCommand {
    pub async fn execute(&mut self, app: &'static App, chooser: &Chooser) -> anyhow::Result<()> {
        // Step 1: Load repositories
        let repositories = load_repositories(app, !self.no_cache).await?;

        // Step 2: Select repository
        let repo = match &self.search {
            Some(needle) => {
                let matched_repos = app
                    .fuzzy_matcher()
                    .match_repositories(needle, &repositories);

                matched_repos
                    .first()
                    .ok_or(anyhow::anyhow!("failed to find repository"))?
                    .to_owned()
            }
            None => app
                .interactive()
                .interactive_search(&repositories, "")?
                .ok_or(anyhow::anyhow!("failed to find a repository"))?,
        };

        tracing::debug!("selected repo: {}", repo.to_rel_path().display());

        let wt = app.worktree();
        let (_project_path, bare_path) = wt.paths(&repo);

        // Step 3: Ensure bare clone exists
        if !bare_path.exists() {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() && !self.no_shell {
                let mut wrap_cmd =
                    ProgressCommand::new(format!("cloning: {}", repo.to_rel_path().display()));
                let wt = app.worktree();
                let repo_clone = repo.clone();
                let bare_path_clone = bare_path.clone();
                wrap_cmd
                    .execute(move || async move {
                        wt.ensure_bare_clone(&repo_clone, &bare_path_clone).await?;
                        Ok(())
                    })
                    .await?;
            } else {
                eprintln!("bare-cloning repository...");
                wt.ensure_bare_clone(&repo, &bare_path).await?;
            }
        }

        // Step 4: List branches
        let branches = app.worktree().list_branches(&bare_path).await?;

        if branches.is_empty() {
            anyhow::bail!("no branches found for {}", repo.to_rel_path().display());
        }

        // Step 5: Select branch
        let branch = match &self.branch {
            Some(b) => {
                if !branches.contains(b) {
                    anyhow::bail!(
                        "branch '{}' not found. Available branches: {}",
                        b,
                        branches.join(", ")
                    );
                }
                b.clone()
            }
            None => {
                let items: Vec<StringItem> = branches.into_iter().map(StringItem).collect();

                let selected = app
                    .interactive()
                    .interactive_search_items(&items, "")?
                    .ok_or(anyhow::anyhow!("no branch selected"))?;

                selected.0
            }
        };

        // Step 6: Create worktree at <project_path>/<sanitized_branch>/
        let sanitized = sanitize_branch_name(&branch);
        let (project_path, _) = app.worktree().paths(&repo);
        let worktree_path = project_path.join(&sanitized);

        if !worktree_path.exists() {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() && !self.no_shell {
                let mut wrap_cmd = ProgressCommand::new(format!("creating worktree: {}", branch));
                let wt = app.worktree();
                let bare_path = bare_path.clone();
                let worktree_path = worktree_path.clone();
                let branch = branch.clone();
                wrap_cmd
                    .execute(move || async move {
                        wt.add_worktree(&bare_path, &worktree_path, &branch).await?;
                        Ok(())
                    })
                    .await?;
            } else {
                eprintln!("creating worktree for branch '{}'...", branch);
                app.worktree()
                    .add_worktree(&bare_path, &worktree_path, &branch)
                    .await?;
            }
        } else {
            tracing::info!("worktree already exists at {}", worktree_path.display());
        }

        // Step 7: Enter shell or print path
        if !self.no_shell && !chooser.is_active() {
            app.shell().spawn_shell_at(&worktree_path).await?;
        } else {
            chooser.set(&worktree_path)?;
        }

        Ok(())
    }
}

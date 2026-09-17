use chrono::Utc;
use futures::{StreamExt, stream};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{
    app::App,
    cache::load_repositories,
    chooser::Chooser,
    config::expand_tilde,
    custom_command::CustomCommandApp,
    fuzzy_matcher::FuzzyMatcherApp,
    interactive::{InteractiveApp, Searchable},
    project_metadata::{ProjectMetadata, RepoEntry},
    shell::ShellApp,
    template_command,
};

use super::root::RepositoryMatcher;

#[derive(clap::Parser)]
pub struct ProjectCommand {
    #[command(subcommand)]
    command: Option<ProjectSubcommand>,

    /// Search string to filter existing projects
    #[arg()]
    search: Option<String>,

    /// Skip spawning a shell in the project directory
    #[arg(long = "no-shell", default_value = "false")]
    no_shell: bool,
}

#[derive(clap::Subcommand)]
enum ProjectSubcommand {
    /// Create a new project with selected repositories
    Create(ProjectCreateCommand),
    /// Add repositories to an existing project
    Add(ProjectAddCommand),
    /// Remove repositories from an existing project
    Remove(ProjectRemoveCommand),
    /// Delete an existing project
    Delete(ProjectDeleteCommand),
    /// List all projects and their repositories
    List(ProjectListCommand),
}

#[derive(clap::Parser)]
pub struct ProjectCreateCommand {
    /// Project name (will be used as directory name)
    #[arg()]
    name: Option<String>,

    /// Bootstrap from a template in the templates directory
    #[arg(long = "template", short = 't')]
    template: Option<String>,

    /// Skip template selection entirely (even if templates exist)
    #[arg(long = "no-template", default_value = "false")]
    no_template: bool,

    /// Repositories to include (fuzzy-matched against the cache). Can be
    /// specified multiple times: --repos foo --repos bar
    #[arg(long = "repos", short = 'r')]
    repos: Vec<String>,

    /// Skip cache when fetching repositories
    #[arg(long = "no-cache", default_value = "false")]
    no_cache: bool,

    /// Skip spawning a shell in the project directory
    #[arg(long = "no-shell", default_value = "false")]
    no_shell: bool,
}

#[derive(clap::Parser)]
pub struct ProjectAddCommand {
    /// Project name to add repositories to
    #[arg()]
    name: Option<String>,

    /// Repositories to add (fuzzy-matched against the cache). Can be
    /// specified multiple times: --repos foo --repos bar
    #[arg(long = "repos", short = 'r')]
    repos: Vec<String>,

    /// Skip cache when fetching repositories
    #[arg(long = "no-cache", default_value = "false")]
    no_cache: bool,
}

#[derive(clap::Parser)]
pub struct ProjectRemoveCommand {
    /// Project name to remove repositories from
    #[arg()]
    name: Option<String>,

    /// Repositories to remove (fuzzy-matched against the project manifest).
    /// Can be specified multiple times: --repos foo --repos bar
    #[arg(long = "repos", short = 'r')]
    repos: Vec<String>,

    /// Skip confirmation prompt
    #[arg(long = "force", short = 'f', default_value = "false")]
    force: bool,
}

#[derive(clap::Parser)]
pub struct ProjectListCommand {
    /// Show repository details for each project
    #[arg(long = "repos", default_value = "false")]
    repos: bool,

    /// Output as JSON
    #[arg(long = "json", default_value = "false")]
    json: bool,
}

#[derive(clap::Parser)]
pub struct ProjectDeleteCommand {
    /// Project name to delete
    #[arg()]
    name: Option<String>,

    /// Delete every project created more than this many days ago
    #[arg(long, value_name = "DAYS", conflicts_with = "name")]
    older_than: Option<u32>,

    /// Delete every project created before this date or RFC 3339 timestamp
    #[arg(
        long,
        value_name = "DATE",
        value_parser = parse_cutoff_date,
        conflicts_with_all = ["name", "older_than"]
    )]
    before: Option<chrono::DateTime<Utc>>,

    /// Skip confirmation prompt
    #[arg(long = "force", short = 'f', default_value = "false")]
    force: bool,

    /// Suppress previews and successful deletion output
    #[arg(long, short = 'q', default_value = "false")]
    quiet: bool,
}

// --- Shared helpers ---

/// A named directory entry usable in interactive search.
#[derive(Clone)]
struct DirEntry {
    name: String,
    path: PathBuf,
    metadata: Option<ProjectMetadata>,
}

impl Searchable for DirEntry {
    fn display_label(&self) -> String {
        match &self.metadata {
            Some(meta) => format!("{} ({})", self.name, meta.created_ago()),
            None => self.name.clone(),
        }
    }
}

fn parse_cutoff_date(value: &str) -> Result<chrono::DateTime<Utc>, String> {
    if let Ok(date) = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc());
    }

    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .map_err(|_| format!("invalid date '{value}'; expected YYYY-MM-DD or an RFC 3339 timestamp"))
}

/// Resolve a config directory path, expanding `~` to the home directory.
/// Falls back to `default` if the config value is `None`.
fn resolve_dir(configured: Option<&str>, default: &str) -> PathBuf {
    if let Some(dir) = configured {
        return expand_tilde(PathBuf::from(dir));
    }
    dirs::home_dir().unwrap_or_default().join(default)
}

fn get_projects_dir(app: &'static App) -> PathBuf {
    let configured = app
        .config
        .settings
        .project
        .as_ref()
        .and_then(|p| p.directory.as_deref());
    resolve_dir(configured, ".gitnow/projects")
}

fn get_templates_dir(app: &'static App) -> PathBuf {
    let configured = app
        .config
        .settings
        .project
        .as_ref()
        .and_then(|p| p.templates_directory.as_deref());
    resolve_dir(configured, ".gitnow/templates")
}

/// List subdirectories of `dir` as `DirEntry` items.
/// Projects with metadata are sorted by creation time (most recent first),
/// followed by projects without metadata sorted alphabetically.
fn list_subdirectories(dir: &Path) -> anyhow::Result<Vec<DirEntry>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = entry.path();
            let metadata = ProjectMetadata::load(&path);
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path,
                metadata,
            });
        }
    }

    entries.sort_by(|a, b| {
        match (&a.metadata, &b.metadata) {
            // Both have metadata: most recent first
            (Some(a_meta), Some(b_meta)) => b_meta.created_at.cmp(&a_meta.created_at),
            // Metadata projects come before non-metadata ones
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            // Both without metadata: alphabetical
            (None, None) => a.name.cmp(&b.name),
        }
    });

    Ok(entries)
}

fn projects_created_before(projects: &[DirEntry], cutoff: chrono::DateTime<Utc>) -> Vec<&DirEntry> {
    projects
        .iter()
        .filter(|project| {
            project
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.created_at < cutoff)
        })
        .collect()
}

const PROJECT_DELETE_CONCURRENCY: usize = 4;

async fn delete_project_directories<'a, I>(projects: I) -> Vec<(&'a DirEntry, std::io::Result<()>)>
where
    I: IntoIterator<Item = &'a DirEntry>,
{
    stream::iter(projects)
        .map(|project| async move {
            let result = tokio::fs::remove_dir_all(&project.path).await;
            (project, result)
        })
        .buffered(PROJECT_DELETE_CONCURRENCY)
        .collect()
        .await
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// Clone selected repositories concurrently into `target_dir`.
async fn clone_repos_into(
    app: &'static App,
    repos: &[crate::git_provider::Repository],
    target_dir: &Path,
) -> anyhow::Result<()> {
    let clone_template = app
        .config
        .settings
        .clone_command
        .as_deref()
        .unwrap_or(template_command::DEFAULT_CLONE_COMMAND);

    let concurrency_limit = Arc::new(tokio::sync::Semaphore::new(5));
    let mut handles = Vec::new();

    for repo in repos {
        let repo = repo.clone();
        let target_dir = target_dir.to_path_buf();
        let clone_template = clone_template.to_string();
        let concurrency = Arc::clone(&concurrency_limit);
        let custom_command = app.custom_command();

        let handle = tokio::spawn(async move {
            let _permit = concurrency.acquire().await?;

            let clone_path = target_dir.join(&repo.repo_name);

            if clone_path.exists() {
                eprintln!("  {} already exists, skipping", repo.repo_name);
                return Ok::<(), anyhow::Error>(());
            }

            eprintln!("  cloning {}...", repo.to_rel_path().display());

            let path_str = clone_path.display().to_string();
            let context = HashMap::from([
                ("ssh_url", repo.ssh_url.as_str()),
                ("clone_url", repo.clone_url.as_str()),
                ("path", path_str.as_str()),
            ]);

            let output = template_command::render_and_execute(&clone_template, context).await?;

            if !output.status.success() {
                let stderr = std::str::from_utf8(&output.stderr).unwrap_or_default();
                anyhow::bail!("failed to clone {}: {}", repo.repo_name, stderr);
            }

            custom_command
                .execute_post_clone_command(&clone_path)
                .await?;

            Ok(())
        });

        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;
    for res in results {
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!("clone error: {}", e);
                eprintln!("error: {}", e);
            }
            Err(e) => {
                tracing::error!("task error: {}", e);
                eprintln!("error: {}", e);
            }
        }
    }

    Ok(())
}

/// Helper to select an existing project, either by name or interactively.
fn select_project(
    app: &'static App,
    name: Option<String>,
    projects: &[DirEntry],
) -> anyhow::Result<DirEntry> {
    match name {
        Some(name) => projects
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| anyhow::anyhow!("project '{}' not found", name))
            .cloned(),
        None => app
            .interactive()
            .interactive_search_items(projects, "")?
            .ok_or_else(|| anyhow::anyhow!("no project selected")),
    }
}

async fn auto_delete_old_projects(app: &'static App) -> anyhow::Result<()> {
    let Some(days) = app
        .config
        .settings
        .project
        .as_ref()
        .and_then(|settings| settings.auto_delete_older_than_days)
    else {
        return Ok(());
    };

    let cutoff = Utc::now()
        .checked_sub_signed(chrono::Duration::days(i64::from(days)))
        .ok_or_else(|| anyhow::anyhow!("auto_delete_older_than_days value is too large"))?;
    let projects = list_subdirectories(&get_projects_dir(app))?;
    let matching = projects_created_before(&projects, cutoff);

    if matching.is_empty() {
        return Ok(());
    }

    eprintln!("Automatically deleting projects older than {days} days:");
    for project in &matching {
        eprintln!("  - {} ({})", project.name, project.path.display());
    }

    for (project, result) in delete_project_directories(matching).await {
        result.map_err(|error| {
            anyhow::anyhow!(
                "failed to automatically delete project '{}': {}",
                project.name,
                error
            )
        })?;
    }

    Ok(())
}

// --- Command implementations ---

impl ProjectCommand {
    pub async fn execute(&mut self, app: &'static App, chooser: &Chooser) -> anyhow::Result<()> {
        if !matches!(&self.command, Some(ProjectSubcommand::Delete(_))) {
            auto_delete_old_projects(app).await?;
        }

        match self.command.take() {
            Some(ProjectSubcommand::Create(mut create)) => create.execute(app, chooser).await,
            Some(ProjectSubcommand::Add(mut add)) => add.execute(app).await,
            Some(ProjectSubcommand::Remove(mut remove)) => remove.execute(app).await,
            Some(ProjectSubcommand::Delete(mut delete)) => delete.execute(app).await,
            Some(ProjectSubcommand::List(list)) => list.execute(app).await,
            None => self.open_existing(app, chooser).await,
        }
    }

    async fn open_existing(&self, app: &'static App, chooser: &Chooser) -> anyhow::Result<()> {
        let projects_dir = get_projects_dir(app);
        let projects = list_subdirectories(&projects_dir)?;

        if projects.is_empty() {
            anyhow::bail!(
                "no projects found in {}. Use 'gitnow project create' to create one.",
                projects_dir.display()
            );
        }

        let project = match &self.search {
            Some(needle) => {
                let matched = projects
                    .iter()
                    .find(|p| p.name.contains(needle.as_str()))
                    .or_else(|| {
                        projects.iter().find(|p| {
                            p.name
                                .to_lowercase()
                                .contains(&needle.to_lowercase())
                        })
                    })
                    .ok_or(anyhow::anyhow!(
                        "no project matching '{}' found",
                        needle
                    ))?
                    .clone();
                matched
            }
            None => app
                .interactive()
                .interactive_search_items(&projects, "")?
                .ok_or(anyhow::anyhow!("no project selected"))?,
        };

        if !self.no_shell && !chooser.is_active() {
            app.shell().spawn_shell_at(&project.path).await?;
        } else {
            chooser.set(&project.path)?;
        }

        Ok(())
    }
}

impl ProjectCreateCommand {
    async fn execute(&mut self, app: &'static App, chooser: &Chooser) -> anyhow::Result<()> {
        let name = match self.name.take() {
            Some(n) => n,
            None => {
                eprint!("Project name: ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    anyhow::bail!("project name cannot be empty");
                }
                trimmed
            }
        };

        let dir_name = name
            .replace(' ', "-")
            .replace('/', "-")
            .to_lowercase();

        let projects_dir = get_projects_dir(app);
        let project_path = projects_dir.join(&dir_name);

        if project_path.exists() {
            anyhow::bail!(
                "project '{}' already exists at {}",
                dir_name,
                project_path.display()
            );
        }

        let repositories = load_repositories(app, !self.no_cache).await?;

        let selected_repos = if !self.repos.is_empty() {
            let matcher = app.fuzzy_matcher();
            let mut matched = Vec::new();
            for needle in &self.repos {
                let results = matcher.match_repositories(needle, &repositories);
                let repo = results
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("no repository matching '{}' found", needle))?
                    .to_owned();
                if !matched.iter().any(|r: &crate::git_provider::Repository| r.ssh_url == repo.ssh_url) {
                    matched.push(repo);
                }
            }
            matched
        } else {
            eprintln!("Select repositories (Tab to toggle, Enter to confirm):");
            app.interactive()
                .interactive_multi_search(&repositories)?
        };

        if selected_repos.is_empty() {
            anyhow::bail!("no repositories selected");
        }

        tokio::fs::create_dir_all(&project_path).await?;

        clone_repos_into(app, &selected_repos, &project_path).await?;

        // Apply template if requested
        let templates_dir = get_templates_dir(app);
        let template = if self.no_template {
            None
        } else {
            match self.template.take() {
                Some(name) => {
                    let templates = list_subdirectories(&templates_dir)?;
                    Some(
                        templates
                            .into_iter()
                            .find(|t| t.name == name)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "template '{}' not found in {}",
                                    name,
                                    templates_dir.display()
                                )
                            })?,
                    )
                }
                None => {
                    let templates = list_subdirectories(&templates_dir)?;
                    if !templates.is_empty() {
                        eprintln!("Select a project template (Esc to skip):");
                        app.interactive().interactive_search_items(&templates, "")?
                    } else {
                        None
                    }
                }
            }
        };

        let template_name = if let Some(template) = template {
            eprintln!("  applying template '{}'...", template.name);
            copy_dir_recursive(&template.path, &project_path)?;
            Some(template.name.clone())
        } else {
            None
        };

        let repo_entries: Vec<RepoEntry> = selected_repos.iter().map(RepoEntry::from).collect();
        let metadata = ProjectMetadata::new(dir_name.clone(), template_name, repo_entries);
        metadata.save(&project_path)?;

        eprintln!(
            "project '{}' created at {} with {} repositories",
            dir_name,
            project_path.display(),
            selected_repos.len()
        );

        if !self.no_shell && !chooser.is_active() {
            app.shell().spawn_shell_at(&project_path).await?;
        } else {
            chooser.set(&project_path)?;
        }

        Ok(())
    }
}

impl ProjectAddCommand {
    async fn execute(&mut self, app: &'static App) -> anyhow::Result<()> {
        let projects_dir = get_projects_dir(app);
        let projects = list_subdirectories(&projects_dir)?;

        if projects.is_empty() {
            anyhow::bail!(
                "no projects found in {}. Use 'gitnow project create' to create one.",
                projects_dir.display()
            );
        }

        let project = select_project(app, self.name.take(), &projects)?;

        let repositories = load_repositories(app, !self.no_cache).await?;

        let selected_repos = if !self.repos.is_empty() {
            let matcher = app.fuzzy_matcher();
            let mut matched = Vec::new();
            for needle in &self.repos {
                let results = matcher.match_repositories(needle, &repositories);
                let repo = results
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("no repository matching '{}' found", needle))?
                    .to_owned();
                if !matched.iter().any(|r: &crate::git_provider::Repository| r.ssh_url == repo.ssh_url) {
                    matched.push(repo);
                }
            }
            matched
        } else {
            eprintln!("Select repositories to add (Tab to toggle, Enter to confirm):");
            app.interactive()
                .interactive_multi_search(&repositories)?
        };

        if selected_repos.is_empty() {
            anyhow::bail!("no repositories selected");
        }

        clone_repos_into(app, &selected_repos, &project.path).await?;

        if let Some(mut metadata) = ProjectMetadata::load(&project.path) {
            let new_entries: Vec<RepoEntry> = selected_repos.iter().map(RepoEntry::from).collect();
            metadata.add_repositories(new_entries);
            metadata.save(&project.path)?;
        }

        eprintln!(
            "added {} repositories to project '{}'",
            selected_repos.len(),
            project.name
        );

        Ok(())
    }
}

impl Searchable for RepoEntry {
    fn display_label(&self) -> String {
        format!("{}/{}/{}", self.provider, self.owner, self.repo_name)
    }
}

impl ProjectRemoveCommand {
    async fn execute(&mut self, app: &'static App) -> anyhow::Result<()> {
        let projects_dir = get_projects_dir(app);
        let projects = list_subdirectories(&projects_dir)?;

        if projects.is_empty() {
            anyhow::bail!(
                "no projects found in {}. Use 'gitnow project create' to create one.",
                projects_dir.display()
            );
        }

        let project = select_project(app, self.name.take(), &projects)?;
        let mut metadata = ProjectMetadata::load(&project.path);

        let candidates: Vec<RepoEntry> = match &metadata {
            Some(meta) => meta.repositories.clone(),
            None => list_subdirectories(&project.path)?
                .into_iter()
                .map(|d| RepoEntry {
                    provider: String::new(),
                    owner: String::new(),
                    repo_name: d.name,
                    ssh_url: String::new(),
                })
                .collect(),
        };

        if candidates.is_empty() {
            anyhow::bail!("no repositories found in project '{}'", project.name);
        }

        let selected: Vec<RepoEntry> = if !self.repos.is_empty() {
            let mut matched: Vec<RepoEntry> = Vec::new();
            for needle in &self.repos {
                let needle_lc = needle.to_lowercase();
                let hit = candidates
                    .iter()
                    .find(|r| r.repo_name.to_lowercase().contains(&needle_lc))
                    .ok_or_else(|| anyhow::anyhow!("no repository matching '{}' found", needle))?
                    .clone();
                if !matched.iter().any(|r| r.repo_name == hit.repo_name) {
                    matched.push(hit);
                }
            }
            matched
        } else {
            eprintln!("Select repositories to remove (Tab to toggle, Enter to confirm):");
            app.interactive().interactive_multi_search(&candidates)?
        };

        if selected.is_empty() {
            eprintln!("no repositories selected");
            return Ok(());
        }

        if !self.force {
            eprintln!("Repositories to remove from '{}':", project.name);
            for repo in &selected {
                eprintln!("  - {}", repo.display_label());
            }
            eprint!("Proceed? [y/N] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                eprintln!("aborted");
                return Ok(());
            }
        }

        let mut removed = 0;
        for repo in &selected {
            let repo_path = project.path.join(&repo.repo_name);
            if repo_path.exists() {
                tokio::fs::remove_dir_all(&repo_path).await?;
                eprintln!("  removed {}", repo.repo_name);
                removed += 1;
            } else {
                eprintln!("  {} not found on disk, dropping from manifest", repo.repo_name);
            }
        }

        if let Some(meta) = metadata.as_mut() {
            meta.remove_repositories(&selected);
            meta.save(&project.path)?;
        }

        eprintln!(
            "removed {} repositories from project '{}'",
            removed,
            project.name
        );

        Ok(())
    }
}

impl ProjectListCommand {
    async fn execute(&self, app: &'static App) -> anyhow::Result<()> {
        let projects_dir = get_projects_dir(app);
        let projects = list_subdirectories(&projects_dir)?;

        if projects.is_empty() {
            if self.json {
                println!("[]");
            } else {
                eprintln!(
                    "no projects found in {}. Use 'gitnow project create' to create one.",
                    projects_dir.display()
                );
            }
            return Ok(());
        }

        if self.json {
            let mut entries = Vec::new();
            for project in &projects {
                let mut entry = serde_json::Map::new();
                entry.insert(
                    "name".into(),
                    serde_json::Value::String(project.name.clone()),
                );
                entry.insert(
                    "path".into(),
                    serde_json::Value::String(project.path.display().to_string()),
                );
                if let Some(meta) = &project.metadata {
                    entry.insert(
                        "created_at".into(),
                        serde_json::Value::String(meta.created_at.to_rfc3339()),
                    );
                    if let Some(template) = &meta.template {
                        entry.insert(
                            "template".into(),
                            serde_json::Value::String(template.clone()),
                        );
                    }
                    if self.repos {
                        let repos: Vec<serde_json::Value> = meta
                            .repositories
                            .iter()
                            .map(|r| {
                                serde_json::json!({
                                    "provider": r.provider,
                                    "owner": r.owner,
                                    "repo_name": r.repo_name,
                                    "ssh_url": r.ssh_url,
                                })
                            })
                            .collect();
                        entry.insert("repositories".into(), serde_json::Value::Array(repos));
                    }
                }
                entries.push(serde_json::Value::Object(entry));
            }
            println!("{}", serde_json::to_string_pretty(&entries)?);
        } else {
            for project in &projects {
                if let Some(meta) = &project.metadata {
                    println!("{} ({})", project.name, meta.created_ago());
                } else {
                    println!("{}", project.name);
                }
                if self.repos {
                    if let Some(meta) = &project.metadata {
                        for repo in &meta.repositories {
                            println!("  {}/{}/{}", repo.provider, repo.owner, repo.repo_name);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl ProjectDeleteCommand {
    async fn execute(&mut self, app: &'static App) -> anyhow::Result<()> {
        let projects_dir = get_projects_dir(app);
        let projects = list_subdirectories(&projects_dir)?;

        if projects.is_empty() {
            anyhow::bail!("no projects found in {}", projects_dir.display());
        }

        let selected: Vec<DirEntry> = if let Some(days) = self.older_than {
            let cutoff = Utc::now()
                .checked_sub_signed(chrono::Duration::days(i64::from(days)))
                .ok_or_else(|| anyhow::anyhow!("--older-than value is too large"))?;
            let matching = projects_created_before(&projects, cutoff);

            if matching.is_empty() {
                if !self.quiet {
                    eprintln!("no projects older than {days} days found");
                }
                return Ok(());
            }

            matching.into_iter().cloned().collect()
        } else if let Some(cutoff) = self.before {
            let matching = projects_created_before(&projects, cutoff);

            if matching.is_empty() {
                if !self.quiet {
                    eprintln!("no projects created before {} found", cutoff.to_rfc3339());
                }
                return Ok(());
            }

            matching.into_iter().cloned().collect()
        } else {
            match self.name.take() {
                Some(name) => {
                    let project = projects
                        .iter()
                        .find(|p| p.name == name)
                        .ok_or_else(|| anyhow::anyhow!("project '{}' not found", name))?
                        .clone();
                    vec![project]
                }
                None => {
                    eprintln!("Select projects to delete (Tab to toggle, Enter to confirm):");
                    app.interactive().interactive_multi_search(&projects)?
                }
            }
        };

        if selected.is_empty() {
            eprintln!("no projects selected");
            return Ok(());
        }

        if !self.quiet {
            eprintln!("Projects to delete:");
            for project in &selected {
                eprintln!("  - {} ({})", project.name, project.path.display());
            }
        }

        if !self.force {
            eprint!("Proceed? [y/N] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                if !self.quiet {
                    eprintln!("aborted");
                }
                return Ok(());
            }
        }

        let mut deleted = 0;
        for (project, result) in delete_project_directories(&selected).await {
            match result {
                Ok(()) => {
                    if !self.quiet {
                        eprintln!("  deleted {}", project.name);
                    }
                    deleted += 1;
                }
                Err(e) => {
                    eprintln!("  failed to delete {}: {}", project.name, e);
                }
            }
        }

        if !self.quiet {
            eprintln!("deleted {} project(s)", deleted);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, created_at: Option<chrono::DateTime<Utc>>) -> DirEntry {
        DirEntry {
            name: name.into(),
            path: PathBuf::from(name),
            metadata: created_at.map(|created_at| ProjectMetadata {
                version: 1,
                name: name.into(),
                created_at,
                template: None,
                repositories: Vec::new(),
            }),
        }
    }

    #[test]
    fn age_filter_only_selects_projects_created_before_cutoff() {
        let cutoff = "2026-07-15T12:00:00Z".parse().unwrap();
        let projects = vec![
            project("older", Some(cutoff - chrono::Duration::seconds(1))),
            project("at-cutoff", Some(cutoff)),
            project("newer", Some(cutoff + chrono::Duration::seconds(1))),
            project("without-metadata", None),
        ];

        let matching = projects_created_before(&projects, cutoff);
        let names: Vec<&str> = matching
            .iter()
            .map(|project| project.name.as_str())
            .collect();

        assert_eq!(names, ["older"]);
    }

    #[test]
    fn cutoff_parser_accepts_dates_and_rfc3339_timestamps() {
        let date = parse_cutoff_date("2026-07-15").unwrap();
        let timestamp = parse_cutoff_date("2026-07-15T02:00:00+02:00").unwrap();
        let expected: chrono::DateTime<Utc> = "2026-07-15T00:00:00Z".parse().unwrap();

        assert_eq!(date, expected);
        assert_eq!(timestamp, expected);
    }

    #[test]
    fn cutoff_parser_rejects_invalid_dates() {
        let error = parse_cutoff_date("15/07/2026").unwrap_err();

        assert!(error.contains("expected YYYY-MM-DD or an RFC 3339 timestamp"));
    }
}

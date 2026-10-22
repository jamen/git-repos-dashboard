use derive_more::{Display, Error};
use git2::{BranchType, Repository, Revwalk, Sort};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};
use store::{QueryError, Store};

const REPO_PREFIX: &str = "repo:";
const ACTIVITY_KEY: &str = "activity";
const ACTIVITY_PER_REPO_LIMIT: usize = 3;
const BRANCH_COMMIT_LIMIT: usize = 5;
const GIT_DIRECTORY: &str = ".git";

/// A Git repository found in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    /// The repository's real Git directory: `<repo>/.git` for a working tree,
    /// or the directory itself for a bare repository.
    pub git_dir: PathBuf,
    pub bare: bool,
    pub head_commit: Option<Commit>,
    pub commits: u64,
    pub size_bytes: u64,
}

/// A compact description of a commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub hash: String,
    pub summary: String,
    pub time: i64,
}

/// A commit from one of the repositories in the activity feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityCommit {
    pub repo: String,
    pub commit: Commit,
}

/// A commit loaded for the commit view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetails {
    pub short: String,
    pub hash: String,
    pub summary: String,
    pub message: String,
    pub author: String,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub commits: Vec<Commit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

impl Repo {
    fn key(&self) -> String {
        format!("{REPO_PREFIX}{}", self.git_dir.display())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub found: usize,
    pub removed: usize,
}

/// Finds every repository in `root`, writes it to the store, and removes
/// records for repositories that are no longer there.
pub async fn scan(root: &Path, store: &Store) -> Result<Summary, ScanError> {
    let (repos, activity) = discover(root)?;
    let mut keys = HashSet::with_capacity(repos.len());

    for repo in &repos {
        let key = repo.key();

        store.set(&key, repo).await.map_err(ScanError::Store)?;
        keys.insert(key);
    }

    let existing = list(store).await.map_err(ScanError::Store)?;
    let mut removed = 0;

    store
        .set(ACTIVITY_KEY, &activity)
        .await
        .map_err(ScanError::Store)?;

    for repo in existing {
        let key = repo.key();

        if !keys.contains(&key) {
            store.remove(&key).await.map_err(ScanError::Store)?;
            removed += 1;
        }
    }

    Ok(Summary {
        found: repos.len(),
        removed,
    })
}

pub async fn list(store: &Store) -> Result<Vec<Repo>, QueryError> {
    let mut repos = store.list::<Repo>(REPO_PREFIX).await?;

    repos.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.git_dir.cmp(&b.git_dir)));

    Ok(repos)
}

pub async fn activity(store: &Store) -> Result<Vec<ActivityCommit>, QueryError> {
    Ok(store.get(ACTIVITY_KEY).await?.unwrap_or_default())
}

pub async fn find(store: &Store, name: &str) -> Result<Option<Repo>, QueryError> {
    Ok(list(store)
        .await?
        .into_iter()
        .find(|repo| repo.name == name))
}

pub fn branches(repo: &Repo) -> Vec<Branch> {
    let Ok(repository) = Repository::open_bare(&repo.git_dir) else {
        return Vec::new();
    };
    let Ok(branches) = repository.branches(Some(BranchType::Local)) else {
        return Vec::new();
    };

    let mut result = Vec::new();

    for branch in branches.flatten() {
        let (branch, _) = branch;
        let Some(name) = branch.name().ok().flatten() else {
            continue;
        };
        let Some(oid) = branch.get().target() else {
            continue;
        };
        let Ok(mut walk) = repository.revwalk() else {
            continue;
        };

        if walk.push(oid).is_err() {
            continue;
        }

        let _ = walk.set_sorting(Sort::TIME);

        let commits: Vec<Commit> = walk
            .filter_map(Result::ok)
            .take(BRANCH_COMMIT_LIMIT)
            .filter_map(|oid| repository.find_commit(oid).ok())
            .map(|commit| commit_row(&commit))
            .collect();

        if !commits.is_empty() {
            result.push(Branch {
                name: name.to_owned(),
                commits,
            });
        }
    }

    result.sort_by(|a, b| {
        let a_time = a.commits.first().map_or(0, |commit| commit.time);
        let b_time = b.commits.first().map_or(0, |commit| commit.time);

        b_time.cmp(&a_time).then_with(|| a.name.cmp(&b.name))
    });

    result
}

pub fn remotes(repo: &Repo) -> Vec<Remote> {
    let Ok(repository) = Repository::open_bare(&repo.git_dir) else {
        return Vec::new();
    };
    let Ok(remotes) = repository.remotes() else {
        return Vec::new();
    };

    let mut result = Vec::new();

    for name in remotes.iter().filter_map(|name| name.ok().flatten()) {
        let Ok(remote) = repository.find_remote(name) else {
            continue;
        };

        result.push(Remote {
            name: name.to_owned(),
            url: remote.url().unwrap_or_default().to_owned(),
        });
    }

    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn find_commit(repo: &Repo, revision: &str) -> Option<CommitDetails> {
    let repository = Repository::open_bare(&repo.git_dir).ok()?;
    let object = repository.revparse_single(revision).ok()?;
    let commit = object.peel_to_commit().ok()?;

    Some(CommitDetails {
        short: short_hash(&commit),
        hash: commit.id().to_string(),
        summary: commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_owned(),
        message: commit.message().ok().unwrap_or_default().trim().to_owned(),
        author: commit.author().name().unwrap_or_default().to_owned(),
        time: commit.time().seconds(),
    })
}

fn discover(root: &Path) -> Result<(Vec<Repo>, Vec<ActivityCommit>), ScanError> {
    let mut repos = Vec::new();
    let mut activity = Vec::new();

    for entry in fs::read_dir(root).map_err(ScanError::ReadDirectory)? {
        let entry = entry.map_err(ScanError::ReadEntry)?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let directory_name = entry.file_name().to_string_lossy().into_owned();
        let git_dir = path.join(GIT_DIRECTORY);

        let (repo, commits) = if git_dir.is_dir() {
            repo_with_metadata(directory_name, git_dir, false)
        } else if is_git_dir(&path) {
            let name = directory_name
                .strip_suffix(GIT_DIRECTORY)
                .unwrap_or(&directory_name)
                .to_owned();

            repo_with_metadata(name, path, true)
        } else {
            continue;
        };

        activity.extend(commits.into_iter().map(|commit| ActivityCommit {
            repo: repo.name.clone(),
            commit,
        }));

        repos.push(repo);
    }

    repos.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.git_dir.cmp(&b.git_dir)));

    activity.sort_by_key(|commit| std::cmp::Reverse(commit.commit.time));

    Ok((repos, activity))
}

fn repo_with_metadata(name: String, git_dir: PathBuf, bare: bool) -> (Repo, Vec<Commit>) {
    let repository = match Repository::open_bare(&git_dir) {
        Ok(repository) => Some(repository),
        Err(error) => {
            tracing::warn!("failed to open {name}: {error}");
            None
        }
    };

    let head = repository.as_ref().and_then(head_commit);
    let commits = repository.as_ref().map_or(0, commit_count);
    let activity = repository
        .as_ref()
        .map(|repository| recent_commits(repository, ACTIVITY_PER_REPO_LIMIT))
        .unwrap_or_default();

    let size_bytes = match directory_size(&git_dir) {
        Ok(size) => size,
        Err(error) => {
            tracing::warn!("failed to measure {name}: {error}");
            0
        }
    };

    (
        Repo {
            name,
            git_dir,
            bare,
            head_commit: head,
            commits,
            size_bytes,
        },
        activity,
    )
}

fn is_git_dir(path: &Path) -> bool {
    path.join("HEAD").is_file() && path.join("objects").is_dir() && path.join("refs").is_dir()
}

fn head_commit(repository: &Repository) -> Option<Commit> {
    let reference = repository
        .head()
        .or_else(|_| repository.find_reference("refs/heads/main"))
        .ok()?;
    let commit = reference.peel_to_commit().ok()?;

    Some(commit_row(&commit))
}

fn recent_commits(repository: &Repository, limit: usize) -> Vec<Commit> {
    let Ok(mut walk) = repository.revwalk() else {
        return Vec::new();
    };

    if walk.set_sorting(Sort::TIME).is_err() {
        return Vec::new();
    }

    if !push_branches(repository, &mut walk) {
        return Vec::new();
    }

    walk.filter_map(Result::ok)
        .take(limit)
        .filter_map(|oid| repository.find_commit(oid).ok())
        .map(|commit| commit_row(&commit))
        .collect()
}

fn commit_count(repository: &Repository) -> u64 {
    let Ok(mut walk) = repository.revwalk() else {
        return 0;
    };

    if !push_branches(repository, &mut walk) {
        return 0;
    }

    walk.filter_map(Result::ok).count() as u64
}

fn push_branches(repository: &Repository, walk: &mut Revwalk<'_>) -> bool {
    let mut pushed = false;

    for branch_type in [BranchType::Local, BranchType::Remote] {
        let Ok(branches) = repository.branches(Some(branch_type)) else {
            continue;
        };

        for branch in branches.flatten() {
            if let Some(oid) = branch.0.get().target()
                && walk.push(oid).is_ok()
            {
                pushed = true;
            }
        }
    }

    pushed
}

fn short_hash(commit: &git2::Commit<'_>) -> String {
    commit
        .as_object()
        .short_id()
        .ok()
        .and_then(|id| id.as_str().ok().map(str::to_owned))
        .unwrap_or_else(|| commit.id().to_string())
}

fn commit_row(commit: &git2::Commit<'_>) -> Commit {
    Commit {
        hash: short_hash(commit),
        summary: commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_owned(),
        time: commit.time().seconds(),
    }
}

fn directory_size(path: &Path) -> io::Result<u64> {
    let mut total = 0;
    let mut pending = vec![path.to_path_buf()];

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;

            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.file_type()?.is_file() {
                total += entry.metadata()?.len();
            }
        }
    }

    Ok(total)
}

#[derive(Debug, Display, Error)]
pub enum ScanError {
    #[display("failed to read the library directory: {_0}")]
    ReadDirectory(io::Error),
    #[display("failed to read a library entry: {_0}")]
    ReadEntry(io::Error),
    #[display("failed to update the store: {_0}")]
    Store(QueryError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    async fn setup(name: &str) -> (Store, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "git-repos-dashboard-library-{}-{name}",
            std::process::id()
        ));
        let database = std::env::temp_dir().join(format!(
            "git-repos-dashboard-library-{}-{name}.redb",
            std::process::id()
        ));

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&database);
        fs::create_dir_all(&root).unwrap();

        let store = Store::builder()
            .database(&database)
            .finish()
            .await
            .expect("failed to open database");

        (store, root, database)
    }

    fn normal_repo(root: &Path, name: &str) {
        fs::create_dir_all(root.join(name).join(".git")).unwrap();
    }

    fn bare_repo(root: &Path, name: &str) {
        let path = root.join(name);

        fs::create_dir_all(path.join("objects")).unwrap();
        fs::create_dir_all(path.join("refs")).unwrap();
        fs::write(path.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    }

    fn git_repo(path: &Path, main_branch: &str) {
        let repository = Repository::init_opts(
            path,
            git2::RepositoryInitOptions::new().initial_head(main_branch),
        )
        .unwrap();

        repository
            .remote("origin", "https://example.com/alpha.git")
            .unwrap();

        let initial_signature = git2::Signature::new(
            "Test",
            "test@example.com",
            &git2::Time::new(1_700_000_000, 0),
        )
        .unwrap();
        let feature_signature = git2::Signature::new(
            "Test",
            "test@example.com",
            &git2::Time::new(1_700_000_060, 0),
        )
        .unwrap();

        fs::write(path.join("file.txt"), "one\n").unwrap();

        let mut index = repository.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();

        let main_ref = format!("refs/heads/{main_branch}");
        let initial = repository
            .commit(
                Some(&main_ref),
                &initial_signature,
                &initial_signature,
                "Initial commit",
                &tree,
                &[],
            )
            .unwrap();

        fs::write(path.join("file.txt"), "two\n").unwrap();

        let mut index = repository.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let initial = repository.find_commit(initial).unwrap();

        repository
            .commit(
                Some("refs/heads/feature"),
                &feature_signature,
                &feature_signature,
                "Feature change",
                &tree,
                &[&initial],
            )
            .unwrap();
    }

    #[tokio::test]
    async fn scanning_finds_working_and_bare_repositories() {
        let (store, root, database) = setup("scan").await;

        normal_repo(&root, "alpha");
        bare_repo(&root, "beta.git");
        fs::create_dir_all(root.join("not-a-repo")).unwrap();

        let summary = scan(&root, &store).await.unwrap();
        assert_eq!(summary.found, 2);
        assert_eq!(summary.removed, 0);

        let repos = list(&store).await.unwrap();
        assert_eq!(repos.len(), 2);

        assert_eq!(repos[0].name, "alpha");
        assert_eq!(repos[0].git_dir, root.join("alpha/.git"));
        assert!(!repos[0].bare);

        assert_eq!(repos[1].name, "beta");
        assert_eq!(repos[1].git_dir, root.join("beta.git"));
        assert!(repos[1].bare);

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&database);
    }

    #[tokio::test]
    async fn rescanning_removes_repositories_that_are_gone() {
        let (store, root, database) = setup("prune").await;

        normal_repo(&root, "removed");
        normal_repo(&root, "kept");

        scan(&root, &store).await.unwrap();

        fs::remove_dir_all(root.join("removed")).unwrap();

        let summary = scan(&root, &store).await.unwrap();
        assert_eq!(summary.found, 1);
        assert_eq!(summary.removed, 1);

        let repos = list(&store).await.unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "kept");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&database);
    }

    #[tokio::test]
    async fn head_commit_uses_the_default_branch() {
        let (store, root, database) = setup("head").await;
        let path = root.join("legacy");

        fs::create_dir_all(&path).unwrap();
        git_repo(&path, "master");

        scan(&root, &store).await.unwrap();

        let repos = list(&store).await.unwrap();

        assert_eq!(
            repos[0].head_commit.as_ref().unwrap().summary,
            "Initial commit"
        );

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&database);
    }

    #[tokio::test]
    async fn scanning_reads_commit_metadata() {
        let (store, root, database) = setup("metadata").await;
        let path = root.join("alpha");

        fs::create_dir_all(&path).unwrap();
        git_repo(&path, "main");

        scan(&root, &store).await.unwrap();

        let repos = list(&store).await.unwrap();
        let repo = &repos[0];

        assert_eq!(repo.head_commit.as_ref().unwrap().summary, "Initial commit");
        assert_eq!(repo.commits, 2);
        assert!(repo.size_bytes > 0);

        let commit = find_commit(repo, &repo.head_commit.as_ref().unwrap().hash).unwrap();
        assert_eq!(commit.summary, "Initial commit");
        assert_eq!(commit.author, "Test");

        let remote_list = remotes(repo);
        assert_eq!(remote_list.len(), 1);
        assert_eq!(remote_list[0].name, "origin");
        assert_eq!(remote_list[0].url, "https://example.com/alpha.git");

        let feed = activity(&store).await.unwrap();
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0].repo, "alpha");
        assert_eq!(feed[0].commit.summary, "Feature change");
        assert_eq!(feed[1].commit.summary, "Initial commit");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_file(&database);
    }
}

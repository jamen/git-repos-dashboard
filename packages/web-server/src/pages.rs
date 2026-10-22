use crate::{Context, library};
use askama::Template;
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{Html, IntoResponse, Response},
};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Deserialize;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    convert::Infallible,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use templates::{
    AppShell, BranchPage, BranchRow, BranchesPage, CommitLinkRow, CommitPage, CommitsPage,
    IndexPage, InternalServerErrorPage, NotFoundPage, PageMeta, Partial, RemoteGroupRow, RemoteRow,
    RepoPage, RepoRow, SearchBox, SearchPage,
};

pub const PARTIAL_PARAM: &str = "partial";

const BRANCH_COMMIT_TOTAL: usize = 5;
const BRANCH_COMMIT_MAX: usize = 3;
const BRANCHES_PAGE_SIZE: usize = 50;
const COMMITS_PAGE_SIZE: usize = 50;

#[derive(Deserialize, Default)]
pub struct BranchListQuery {
    page: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct CommitsQuery {
    page: Option<usize>,
    branch: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct SearchQuery {
    q: Option<String>,
    repo: Option<String>,
    branch: Option<String>,
}

const PATH_SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'/')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'`')
    .add(b'{')
    .add(b'}');

const QUERY_VALUE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'&')
    .add(b'+')
    .add(b'=')
    .add(b'?');

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Document,
    Partial,
}

impl Mode {
    fn from_query(query: Option<&str>) -> Self {
        let asked = query.is_some_and(|query| {
            query.split('&').any(|pair| {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                key == PARTIAL_PARAM && value != "0"
            })
        });

        if asked { Mode::Partial } else { Mode::Document }
    }
}

pub struct Nav {
    pub mode: Mode,
    if_none_match: Option<String>,
}

impl Nav {
    fn holds(&self, etag: &HeaderValue) -> bool {
        holds(self.if_none_match.as_deref(), etag)
    }
}

fn holds(if_none_match: Option<&str>, etag: &HeaderValue) -> bool {
    let Some(candidates) = if_none_match else {
        return false;
    };

    let Ok(etag) = etag.to_str() else {
        return false;
    };

    candidates
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == etag || candidate == "*")
}

impl<S> FromRequestParts<S> for Nav
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let mode = Mode::from_query(parts.uri.query());

        let if_none_match = parts
            .headers
            .get(header::IF_NONE_MATCH)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        Ok(Nav {
            mode,
            if_none_match,
        })
    }
}

fn weak_etag(body: &str) -> HeaderValue {
    weak_etag_bytes(body.as_bytes())
}

fn weak_etag_bytes(body: &[u8]) -> HeaderValue {
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);

    HeaderValue::from_str(&format!("W/\"{:016x}\"", hasher.finish()))
        .expect("hex digits are a valid header value")
}

fn envelope_headers(etag: HeaderValue) -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    headers.insert(header::ETAG, etag);

    headers
}

fn send(nav: &Nav, status: StatusCode, html: String) -> Response {
    let etag = weak_etag(&html);

    if status == StatusCode::OK && nav.holds(&etag) {
        return (StatusCode::NOT_MODIFIED, envelope_headers(etag)).into_response();
    }

    (status, envelope_headers(etag), Html(html)).into_response()
}

pub struct Page {
    pub meta: PageMeta,
    pub content: String,
}

impl Page {
    fn render(&self, ctx: &Context, nav: &Nav) -> Result<String, askama::Error> {
        match nav.mode {
            Mode::Document => AppShell {
                meta: &self.meta,
                shell_id: ctx.shell_id,
                css: ctx.assets.css,
                js: ctx.assets.js,
                search: SearchBox::all(),
                version: env!("APP_VERSION"),
                content: &self.content,
            }
            .render(),
            Mode::Partial => Partial {
                meta: &self.meta,
                shell_id: ctx.shell_id,
                content: &self.content,
            }
            .render(),
        }
    }

    pub fn respond(self, ctx: &Context, nav: &Nav, status: StatusCode) -> Response {
        match self.render(ctx, nav) {
            Ok(html) => send(nav, status, html),
            Err(error) => {
                tracing::error!("failed to render envelope: {}", error);
                internal_server_error(ctx, nav)
            }
        }
    }
}

fn respond(
    ctx: &Context,
    nav: &Nav,
    status: StatusCode,
    meta: PageMeta,
    content: impl Template,
) -> Response {
    match content.render() {
        Ok(content) => Page { meta, content }.respond(ctx, nav, status),
        Err(error) => {
            tracing::error!("failed to render page content: {}", error);
            internal_server_error(ctx, nav)
        }
    }
}

fn internal_server_error(ctx: &Context, nav: &Nav) -> Response {
    let page = InternalServerErrorPage.render().ok().and_then(|content| {
        Page {
            meta: PageMeta::new("Internal Server Error", ""),
            content,
        }
        .render(ctx, nav)
        .ok()
    });

    let html = page.unwrap_or_else(|| {
        tracing::error!("failed to render the error page");
        templates::INTERNAL_SERVER_ERROR_FALLBACK.to_owned()
    });

    send(nav, StatusCode::INTERNAL_SERVER_ERROR, html)
}

fn repo_href(repo: &str) -> String {
    format!("/repos/{}", utf8_percent_encode(repo, PATH_SEGMENT))
}

fn search_href(repo: Option<&str>, branch: Option<&str>) -> String {
    let mut href = "/search".to_owned();
    let mut params = Vec::new();

    if let Some(repo) = repo {
        params.push(format!("repo={}", utf8_percent_encode(repo, QUERY_VALUE)));

        if let Some(branch) = branch {
            params.push(format!(
                "branch={}",
                utf8_percent_encode(branch, QUERY_VALUE)
            ));
        }
    }

    if !params.is_empty() {
        href.push('?');
        href.push_str(&params.join("&"));
    }

    href
}

fn branches_href(repo: &str, page: usize) -> String {
    let base = format!("/branches/{}", utf8_percent_encode(repo, PATH_SEGMENT));

    if page > 1 {
        format!("{base}?page={page}")
    } else {
        base
    }
}

fn branch_path(branch: &str) -> String {
    branch
        .split('/')
        .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn branch_href(repo: &str, branch: &str) -> String {
    format!(
        "/branches/{}/{}",
        utf8_percent_encode(repo, PATH_SEGMENT),
        branch_path(branch)
    )
}

fn commits_href(repo: &str, branch: Option<&str>, page: usize) -> String {
    let mut href = format!("/commits/{}", utf8_percent_encode(repo, PATH_SEGMENT));
    let mut params = Vec::new();

    if let Some(branch) = branch {
        params.push(format!(
            "branch={}",
            utf8_percent_encode(branch, QUERY_VALUE)
        ));
    }

    if page > 1 {
        params.push(format!("page={page}"));
    }

    if !params.is_empty() {
        href.push('?');
        href.push_str(&params.join("&"));
    }

    href
}

fn commit_href(repo: &str, hash: &str) -> String {
    format!(
        "/commits/{}/{}",
        utf8_percent_encode(repo, PATH_SEGMENT),
        utf8_percent_encode(hash, PATH_SEGMENT)
    )
}

fn mark_dividers(commits: &mut [CommitLinkRow]) {
    let mut seen_new = false;
    let mut marked = false;

    for commit in commits {
        if commit.is_new {
            seen_new = true;
        } else if seen_new && !marked {
            commit.divider_before = true;
            marked = true;
        }
    }
}

fn commit_link_row(
    repo_name: &str,
    now: i64,
    commit: library::Commit,
    is_new: bool,
) -> CommitLinkRow {
    CommitLinkRow {
        href: commit_href(repo_name, &commit.hash),
        hash: commit.hash,
        summary: commit.summary,
        when: relative_time(now, commit.time),
        is_new,
        divider_before: false,
    }
}

fn branch_rows(repo_name: &str, branches: Vec<library::Branch>, now: i64) -> Vec<BranchRow> {
    let single_branch = branches.len() == 1;
    let mut rows = Vec::new();
    let mut total = 0;

    for branch in branches {
        if total >= BRANCH_COMMIT_TOTAL {
            break;
        }

        let limit = if single_branch {
            BRANCH_COMMIT_TOTAL
        } else {
            BRANCH_COMMIT_MAX
        };
        let take = limit.min(BRANCH_COMMIT_TOTAL - total);

        let mut commits: Vec<CommitLinkRow> = branch
            .commits
            .into_iter()
            .take(take)
            .map(|commit| commit_link_row(repo_name, now, commit.commit, commit.is_new))
            .collect();
        mark_dividers(&mut commits);

        if commits.is_empty() {
            continue;
        }

        total += commits.len();
        let href = branch_href(repo_name, &branch.name);

        rows.push(BranchRow {
            name: branch.name,
            href,
            commits,
        });
    }

    rows
}

fn repo_rows(
    repos: Vec<library::Repo>,
    activity: Vec<library::ActivityCommit>,
    now: i64,
) -> Vec<RepoRow> {
    let mut recent_by_repo: HashMap<String, (i64, Vec<CommitLinkRow>)> = HashMap::new();

    for entry in activity {
        let commit = CommitLinkRow {
            href: commit_href(&entry.repo, &entry.commit.hash),
            hash: entry.commit.hash,
            summary: entry.commit.summary,
            when: relative_time(now, entry.commit.time),
            is_new: false,
            divider_before: false,
        };

        recent_by_repo
            .entry(entry.repo)
            .or_insert_with(|| (entry.commit.time, Vec::new()))
            .1
            .push(commit);
    }

    let mut rows: Vec<(i64, RepoRow)> = repos
        .into_iter()
        .map(|repo| {
            let (latest, recent) = recent_by_repo.remove(&repo.name).unwrap_or((0, Vec::new()));

            let row = RepoRow {
                href: repo_href(&repo.name),
                commits_href: commits_href(&repo.name, None, 1),
                name: repo.name,
                commits: repo.commits,
                size: human_size(repo.size_bytes),
                size_bytes: repo.size_bytes,
                recent,
            };

            (latest, row)
        })
        .collect();

    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    rows.into_iter().map(|(_, row)| row).collect()
}

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn delta_label(ahead: Option<usize>, behind: Option<usize>) -> String {
    match (ahead, behind) {
        (Some(0), Some(0)) => "up to date".to_owned(),
        (Some(ahead), Some(behind)) => {
            let mut parts = Vec::new();

            if ahead > 0 {
                parts.push(format!("↑{ahead}"));
            }

            if behind > 0 {
                parts.push(format!("↓{behind}"));
            }

            parts.join(" ")
        }
        _ => "—".to_owned(),
    }
}

fn relative_time(now: i64, then: i64) -> String {
    let seconds = (now - then).max(0);

    if seconds < 1 {
        return "now".to_owned();
    }

    if seconds < 60 {
        return format!("{seconds}s ago");
    }

    let minutes = seconds / 60;

    if minutes < 60 {
        return format!("{minutes}m ago");
    }

    let hours = minutes / 60;

    if hours < 24 {
        return format!("{hours}h ago");
    }

    let days = hours / 24;

    if days < 7 {
        return format!("{days}d ago");
    }

    let weeks = days / 7;

    if weeks < 8 {
        return format!("{weeks}w ago");
    }

    if days < 365 {
        return format!("{}mo ago", (days / 30).max(1));
    }

    format!("{}y ago", (days / 365).max(1))
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub async fn index_page_handler(State(ctx): State<Arc<Context>>, nav: Nav) -> Response {
    let repos = match library::list(&ctx.store).await {
        Ok(repos) => repos,
        Err(error) => {
            tracing::error!("failed to list repositories: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    let activity = match library::activity(&ctx.store).await {
        Ok(activity) => activity,
        Err(error) => {
            tracing::error!("failed to list activity: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    let now = now_seconds();

    let repos = repo_rows(repos, activity, now);

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new("Git Repos Dashboard", "A minimal dashboard"),
        IndexPage {
            repos,
            search_href: search_href(None, None),
        },
    )
}

fn not_found(ctx: &Context, nav: &Nav) -> Response {
    respond(
        ctx,
        nav,
        StatusCode::NOT_FOUND,
        PageMeta::new("Not Found — Git Repos Dashboard", ""),
        NotFoundPage,
    )
}

fn search_scope_label(repo: Option<&str>, branch: Option<&str>) -> String {
    match (repo, branch) {
        (Some(repo), Some(branch)) => format!("in {repo} / {branch}"),
        (Some(repo), None) => format!("in {repo}"),
        _ => "in all repositories".to_owned(),
    }
}

pub async fn search_page_handler(
    State(ctx): State<Arc<Context>>,
    nav: Nav,
    Query(query): Query<SearchQuery>,
) -> Response {
    let repo = query.repo.filter(|repo| !repo.is_empty());
    let branch = if repo.is_some() {
        query.branch.filter(|branch| !branch.is_empty())
    } else {
        None
    };
    let scope = search_scope_label(repo.as_deref(), branch.as_deref());
    let search_url = search_href(repo.as_deref(), branch.as_deref());
    let mut current_url = search_url.clone();

    if let Some(query) = query.q.as_deref().filter(|query| !query.is_empty()) {
        current_url.push(if current_url.contains('?') { '&' } else { '?' });
        current_url.push_str("q=");
        current_url.push_str(&utf8_percent_encode(query, QUERY_VALUE).to_string());
    }

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new("Search — Git Repos Dashboard", ""),
        SearchPage {
            search: SearchBox {
                query: query.q.unwrap_or_default(),
                repo: repo.unwrap_or_default(),
                branch: branch.unwrap_or_default(),
                scope,
            },
            search_href: search_url,
            current_href: current_url,
        },
    )
}

pub async fn not_found_page_handler(State(ctx): State<Arc<Context>>, nav: Nav) -> Response {
    not_found(&ctx, &nav)
}

pub async fn commits_page_handler(
    State(ctx): State<Arc<Context>>,
    nav: Nav,
    Path(repo_name): Path<String>,
    Query(query): Query<CommitsQuery>,
) -> Response {
    let repo = match library::find(&ctx.store, &repo_name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(&ctx, &nav),
        Err(error) => {
            tracing::error!("failed to find repository {repo_name}: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    let branch = query.branch.filter(|branch| !branch.is_empty());
    let total = library::commit_total(&repo, branch.as_deref()).unwrap_or(0);
    let pages = total.div_ceil(COMMITS_PAGE_SIZE).max(1);
    let page = query.page.unwrap_or(1).max(1).min(pages);
    let offset = (page - 1) * COMMITS_PAGE_SIZE;
    let now = now_seconds();

    let mut commits: Vec<CommitLinkRow> =
        library::commits(&repo, branch.as_deref(), offset, COMMITS_PAGE_SIZE)
            .into_iter()
            .map(|commit| commit_link_row(&repo.name, now, commit.commit, commit.is_new))
            .collect();
    mark_dividers(&mut commits);
    let first = commits_href(&repo.name, branch.as_deref(), 1);
    let prev = (page > 1).then(|| commits_href(&repo.name, branch.as_deref(), page - 1));
    let next = (page < pages).then(|| commits_href(&repo.name, branch.as_deref(), page + 1));
    let last = commits_href(&repo.name, branch.as_deref(), pages);
    let current_url = commits_href(&repo.name, branch.as_deref(), page);
    let all_commits_href = commits_href(&repo.name, None, 1);
    let repo_url = repo_href(&repo.name);
    let branch_url = branch
        .as_deref()
        .map(|branch| branch_href(&repo.name, branch))
        .unwrap_or_default();
    let search_url = search_href(Some(&repo.name), branch.as_deref());
    let title = match &branch {
        Some(branch) => format!("Commits — {branch} — {} — Git Repos Dashboard", repo.name),
        None => format!("Commits — {} — Git Repos Dashboard", repo.name),
    };
    let branch = branch.unwrap_or_default();

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new(title, ""),
        CommitsPage {
            repo: repo.name,
            repo_href: repo_url,
            search_href: search_url,
            branch,
            branch_href: branch_url,
            current_href: current_url,
            all_commits_href,
            total,
            page,
            pages,
            first,
            prev,
            next,
            last,
            commits,
        },
    )
}

pub async fn commit_page_handler(
    State(ctx): State<Arc<Context>>,
    nav: Nav,
    Path((repo_name, hash)): Path<(String, String)>,
) -> Response {
    let repo = match library::find(&ctx.store, &repo_name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(&ctx, &nav),
        Err(error) => {
            tracing::error!("failed to find repository {repo_name}: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    let Some(commit) = library::find_commit(&repo, &hash) else {
        return not_found(&ctx, &nav);
    };

    let message = commit
        .message
        .strip_prefix(&commit.summary)
        .unwrap_or(&commit.message)
        .trim()
        .to_owned();
    let repo_url = repo_href(&repo.name);
    let branch_url = commit
        .branch
        .as_deref()
        .map(|branch| branch_href(&repo.name, branch))
        .unwrap_or_default();
    let search_url = search_href(Some(&repo.name), None);
    let current_url = commit_href(&repo.name, &commit.short);

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new(
            format!("{} — {} — Git Repos Dashboard", commit.short, repo.name),
            "",
        ),
        CommitPage {
            repo: repo.name,
            repo_href: repo_url,
            search_href: search_url,
            short: commit.short,
            branch: commit.branch.unwrap_or_default(),
            branch_href: branch_url,
            current_href: current_url,
            summary: commit.summary,
            message,
            author: commit.author,
            when: relative_time(now_seconds(), commit.time),
        },
    )
}

pub async fn branches_page_handler(
    State(ctx): State<Arc<Context>>,
    nav: Nav,
    Path(repo_name): Path<String>,
    Query(query): Query<BranchListQuery>,
) -> Response {
    let repo = match library::find(&ctx.store, &repo_name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(&ctx, &nav),
        Err(error) => {
            tracing::error!("failed to find repository {repo_name}: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    let branches = library::branches(&repo);
    let total = branches.len();
    let pages = total.div_ceil(BRANCHES_PAGE_SIZE).max(1);
    let page = query.page.unwrap_or(1).max(1).min(pages);
    let offset = (page - 1) * BRANCHES_PAGE_SIZE;

    let now = now_seconds();

    let branches = branches
        .into_iter()
        .skip(offset)
        .take(BRANCHES_PAGE_SIZE)
        .map(|branch| {
            let href = branch_href(&repo.name, &branch.name);
            let mut commits: Vec<CommitLinkRow> = branch
                .commits
                .into_iter()
                .take(BRANCH_COMMIT_MAX)
                .map(|commit| commit_link_row(&repo.name, now, commit.commit, commit.is_new))
                .collect();
            mark_dividers(&mut commits);

            BranchRow {
                name: branch.name,
                href,
                commits,
            }
        })
        .collect();
    let first = branches_href(&repo.name, 1);
    let prev = (page > 1).then(|| branches_href(&repo.name, page - 1));
    let next = (page < pages).then(|| branches_href(&repo.name, page + 1));
    let last = branches_href(&repo.name, pages);
    let current_url = branches_href(&repo.name, page);
    let repo_url = repo_href(&repo.name);
    let search_url = search_href(Some(&repo.name), None);
    let title = format!("Branches — {} — Git Repos Dashboard", repo.name);

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new(title, ""),
        BranchesPage {
            repo: repo.name,
            repo_href: repo_url,
            search_href: search_url,
            current_href: current_url,
            total,
            page,
            pages,
            first,
            prev,
            next,
            last,
            branches,
        },
    )
}

pub async fn branch_page_handler(
    State(ctx): State<Arc<Context>>,
    nav: Nav,
    Path((repo_name, branch)): Path<(String, String)>,
) -> Response {
    let repo = match library::find(&ctx.store, &repo_name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(&ctx, &nav),
        Err(error) => {
            tracing::error!("failed to find repository {repo_name}: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    if !library::branch_exists(&repo, &branch) {
        return not_found(&ctx, &nav);
    }

    let total = library::commit_total(&repo, Some(&branch)).unwrap_or(0);
    let now = now_seconds();

    let mut commits: Vec<CommitLinkRow> =
        library::commits(&repo, Some(&branch), 0, BRANCH_COMMIT_MAX)
            .into_iter()
            .map(|commit| commit_link_row(&repo.name, now, commit.commit, commit.is_new))
            .collect();
    mark_dividers(&mut commits);

    let more_href = commits_href(&repo.name, Some(&branch), 1);
    let repo_url = repo_href(&repo.name);
    let current_url = branch_href(&repo.name, &branch);
    let search_url = search_href(Some(&repo.name), Some(&branch));
    let title = format!("{branch} — {} — Git Repos Dashboard", repo.name);

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new(title, ""),
        BranchPage {
            repo: repo.name,
            repo_href: repo_url,
            search_href: search_url,
            branch,
            current_href: current_url,
            more_href,
            total,
            commits,
        },
    )
}

pub async fn repo_page_handler(
    State(ctx): State<Arc<Context>>,
    nav: Nav,
    Path(repo_name): Path<String>,
) -> Response {
    let repo = match library::find(&ctx.store, &repo_name).await {
        Ok(Some(repo)) => repo,
        Ok(None) => return not_found(&ctx, &nav),
        Err(error) => {
            tracing::error!("failed to find repository {repo_name}: {}", error);
            return internal_server_error(&ctx, &nav);
        }
    };

    let now = now_seconds();
    let title = format!("{} — Git Repos Dashboard", repo.name);
    let branches = library::branches(&repo);
    let branch_count = branches.len();
    let branches = branch_rows(&repo.name, branches, now);
    let remotes = library::remotes(&repo)
        .into_iter()
        .map(|remote| RemoteRow {
            name: remote.name,
            url: remote.url,
            delta: delta_label(remote.ahead, remote.behind),
            activity: remote
                .activity
                .map(|time| relative_time(now, time))
                .unwrap_or_else(|| "—".to_owned()),
        })
        .collect();
    let remote_groups = library::remote_groups(&repo)
        .into_iter()
        .map(|group| RemoteGroupRow {
            name: group.name,
            remotes: group.remotes,
        })
        .collect();

    let search_url = search_href(Some(&repo.name), None);
    let current_url = repo_href(&repo.name);

    respond(
        &ctx,
        &nav,
        StatusCode::OK,
        PageMeta::new(title, ""),
        RepoPage {
            search_href: search_url,
            current_href: current_url,
            branch_count,
            all_commits_href: commits_href(&repo.name, None, 1),
            branches_href: branches_href(&repo.name, 1),
            name: repo.name,
            branches,
            remotes,
            remote_groups,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo(name: &str, commits: u64) -> library::Repo {
        library::Repo {
            name: name.to_owned(),
            git_dir: PathBuf::from(format!("/repos/{name}/.git")),
            bare: false,
            head_commit: None,
            commits,
            size_bytes: 0,
        }
    }

    fn branch(name: &str, count: usize) -> library::Branch {
        library::Branch {
            name: name.to_owned(),
            commits: (0..count)
                .map(|index| library::BranchCommit {
                    commit: library::Commit {
                        hash: format!("{name}{index}"),
                        summary: format!("{name} commit {index}"),
                        time: 1_000 - index as i64,
                    },
                    is_new: false,
                })
                .collect(),
        }
    }

    #[test]
    fn branch_page_fills_to_five_commits_across_branches() {
        let rows = branch_rows("repo", vec![branch("main", 5), branch("feature", 5)], 1_000);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "main");
        assert_eq!(rows[0].commits.len(), 3);
        assert_eq!(rows[1].name, "feature");
        assert_eq!(rows[1].commits.len(), 2);
    }

    #[test]
    fn a_single_branch_shows_five_commits() {
        let rows = branch_rows("repo", vec![branch("main", 7)], 1_000);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].commits.len(), 5);
    }

    #[test]
    fn repository_links_escape_repository_names() {
        assert_eq!(repo_href("my repo"), "/repos/my%20repo");
        assert_eq!(commits_href("my repo", None, 1), "/commits/my%20repo");
        assert_eq!(branches_href("my repo", 1), "/branches/my%20repo");
        assert_eq!(
            branch_href("my repo", "wip/auth"),
            "/branches/my%20repo/wip/auth"
        );
        assert_eq!(branches_href("my repo", 2), "/branches/my%20repo?page=2");
        assert_eq!(commits_href("my repo", None, 1), "/commits/my%20repo");
        assert_eq!(
            commits_href("my repo", Some("feature branch"), 1),
            "/commits/my%20repo?branch=feature%20branch"
        );
        assert_eq!(
            commit_href("my repo", "abc1234"),
            "/commits/my%20repo/abc1234"
        );
    }

    #[test]
    fn repos_are_ordered_by_latest_activity() {
        let now = 1_000_000_000i64;
        let repos = vec![repo("alpha", 10), repo("beta", 20), repo("gamma", 5)];
        let activity = vec![
            library::ActivityCommit {
                repo: "alpha".to_owned(),
                commit: library::Commit {
                    hash: "1111111".to_owned(),
                    summary: "Older".to_owned(),
                    time: now - 100,
                },
            },
            library::ActivityCommit {
                repo: "beta".to_owned(),
                commit: library::Commit {
                    hash: "2222222".to_owned(),
                    summary: "Newer".to_owned(),
                    time: now - 10,
                },
            },
        ];

        let rows = repo_rows(repos, activity, now);

        assert_eq!(rows[0].name, "beta");
        assert_eq!(rows[1].name, "alpha");
        assert_eq!(rows[2].name, "gamma");
        assert_eq!(rows[0].recent.len(), 1);
        assert_eq!(rows[2].recent.len(), 0);
    }

    fn commit_link(is_new: bool) -> CommitLinkRow {
        CommitLinkRow {
            href: "/commits/repo/abc1234".to_owned(),
            hash: "abc1234".to_owned(),
            summary: "commit".to_owned(),
            when: "now".to_owned(),
            is_new,
            divider_before: false,
        }
    }

    #[test]
    fn dividers_mark_the_first_shared_commit() {
        let mut commits = vec![
            commit_link(true),
            commit_link(true),
            commit_link(false),
            commit_link(false),
        ];

        mark_dividers(&mut commits);

        assert!(!commits[0].divider_before);
        assert!(!commits[1].divider_before);
        assert!(commits[2].divider_before);
        assert!(!commits[3].divider_before);

        let mut all_new = vec![commit_link(true), commit_link(true)];
        mark_dividers(&mut all_new);
        assert!(all_new.iter().all(|commit| !commit.divider_before));
    }

    #[test]
    fn remote_deltas_are_compact() {
        assert_eq!(delta_label(Some(2), Some(1)), "↑2 ↓1");
        assert_eq!(delta_label(Some(0), Some(0)), "up to date");
        assert_eq!(delta_label(Some(3), Some(0)), "↑3");
        assert_eq!(delta_label(Some(0), Some(4)), "↓4");
        assert_eq!(delta_label(None, None), "—");
    }

    #[test]
    fn times_are_compact() {
        let now = 1_000_000_000i64;
        let day = 86_400;

        assert_eq!(relative_time(now, now), "now");
        assert_eq!(relative_time(now, now - 1), "1s ago");
        assert_eq!(relative_time(now, now - 59), "59s ago");
        assert_eq!(relative_time(now, now - 60), "1m ago");
        assert_eq!(relative_time(now, now - 3_600), "1h ago");
        assert_eq!(relative_time(now, now - day), "1d ago");
        assert_eq!(relative_time(now, now - 7 * day), "1w ago");
        assert_eq!(relative_time(now, now - 49 * day), "7w ago");
        assert_eq!(relative_time(now, now - 60 * day), "2mo ago");
        assert_eq!(relative_time(now, now - 365 * day), "1y ago");
    }

    fn nav(if_none_match: Option<&str>) -> Nav {
        Nav {
            mode: Mode::Document,
            if_none_match: if_none_match.map(str::to_owned),
        }
    }

    #[test]
    fn the_parameter_selects_the_envelope() {
        assert_eq!(Mode::from_query(Some("partial=1")), Mode::Partial);
        assert_eq!(Mode::from_query(Some("partial")), Mode::Partial);
        assert_eq!(Mode::from_query(Some("q=hello&partial=1")), Mode::Partial);

        assert_eq!(Mode::from_query(None), Mode::Document);
        assert_eq!(Mode::from_query(Some("q=hello")), Mode::Document);
        assert_eq!(Mode::from_query(Some("partial=0")), Mode::Document);

        assert_eq!(Mode::from_query(Some("q=partial=1")), Mode::Document);
        assert_eq!(Mode::from_query(Some("partially=1")), Mode::Document);
    }

    #[test]
    fn etags_identify_the_envelope() {
        let document = weak_etag("<!doctype html><main id=\"main\">x</main>");
        let partial = weak_etag("<main id=\"main\">x</main>");

        assert_ne!(document, partial);

        assert_eq!(partial, weak_etag("<main id=\"main\">x</main>"));
        assert!(partial.to_str().unwrap().starts_with("W/\""));
    }

    #[test]
    fn revalidation_matches_a_held_envelope() {
        let etag = weak_etag("body");

        assert!(nav(Some(etag.to_str().unwrap())).holds(&etag));
        assert!(nav(Some("*")).holds(&etag));

        assert!(nav(Some(&format!("W/\"0000\", {}", etag.to_str().unwrap()))).holds(&etag));

        assert!(!nav(Some("W/\"0000\"")).holds(&etag));
        assert!(!nav(None).holds(&etag));
    }

    #[test]
    fn a_held_envelope_gets_no_body() {
        let html = "<main id=\"main\">x</main>".to_owned();
        let etag = weak_etag(&html);
        let held = nav(Some(etag.to_str().unwrap()));

        let response = send(&held, StatusCode::OK, html.clone());
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(response.headers().get(header::ETAG), Some(&etag));

        let response = send(&held, StatusCode::NOT_FOUND, html);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

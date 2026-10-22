use askama::Template;

pub struct PageMeta {
    pub title: String,
    pub description: String,
}

impl PageMeta {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
        }
    }
}

pub struct SearchBox {
    pub query: String,
    pub repo: String,
    pub branch: String,
    pub scope: String,
}

impl SearchBox {
    pub fn all() -> Self {
        Self {
            query: String::new(),
            repo: String::new(),
            branch: String::new(),
            scope: "in all repositories".to_owned(),
        }
    }
}

#[derive(Template)]
#[template(path = "shells/app.html")]
pub struct AppShell<'a> {
    pub meta: &'a PageMeta,
    pub shell_id: &'a str,
    pub css: &'a str,
    pub js: &'a str,
    pub search: SearchBox,
    pub version: &'a str,
    pub content: &'a str,
}

#[derive(Template)]
#[template(path = "partial.html")]
pub struct Partial<'a> {
    pub meta: &'a PageMeta,
    pub shell_id: &'a str,
    pub content: &'a str,
}

pub struct RepoRow {
    pub name: String,
    pub href: String,
    pub commits_href: String,
    pub commits: u64,
    pub size: String,
    pub size_bytes: u64,
    pub recent: Vec<CommitLinkRow>,
}

pub struct CommitLinkRow {
    pub hash: String,
    pub summary: String,
    pub when: String,
    pub href: String,
    pub is_new: bool,
    pub divider_before: bool,
}

#[derive(Template)]
#[template(path = "pages/index.html")]
pub struct IndexPage {
    pub repos: Vec<RepoRow>,
    pub search_href: String,
}

#[derive(Template)]
#[template(path = "pages/commits.html")]
pub struct CommitsPage {
    pub repo: String,
    pub repo_href: String,
    pub search_href: String,
    pub branch: String,
    pub branch_href: String,
    pub current_href: String,
    pub all_commits_href: String,
    pub total: usize,
    pub page: usize,
    pub pages: usize,
    pub first: String,
    pub prev: Option<String>,
    pub next: Option<String>,
    pub last: String,
    pub commits: Vec<CommitLinkRow>,
}

#[derive(Template)]
#[template(path = "pages/commit.html")]
pub struct CommitPage {
    pub repo: String,
    pub repo_href: String,
    pub search_href: String,
    pub short: String,
    pub branch: String,
    pub branch_href: String,
    pub current_href: String,
    pub summary: String,
    pub message: String,
    pub author: String,
    pub when: String,
}

pub struct BranchRow {
    pub name: String,
    pub href: String,
    pub commits: Vec<CommitLinkRow>,
}

pub struct RemoteRow {
    pub name: String,
    pub url: String,
    pub delta: String,
    pub activity: String,
}

pub struct RemoteGroupRow {
    pub name: String,
    pub remotes: Vec<String>,
}

#[derive(Template)]
#[template(path = "pages/repo.html")]
pub struct RepoPage {
    pub name: String,
    pub search_href: String,
    pub current_href: String,
    pub all_commits_href: String,
    pub branches_href: String,
    pub branches: Vec<BranchRow>,
    pub remotes: Vec<RemoteRow>,
    pub remote_groups: Vec<RemoteGroupRow>,
}

#[derive(Template)]
#[template(path = "pages/branches.html")]
pub struct BranchesPage {
    pub repo: String,
    pub repo_href: String,
    pub search_href: String,
    pub current_href: String,
    pub total: usize,
    pub page: usize,
    pub pages: usize,
    pub first: String,
    pub prev: Option<String>,
    pub next: Option<String>,
    pub last: String,
    pub branches: Vec<BranchRow>,
}

#[derive(Template)]
#[template(path = "pages/search.html")]
pub struct SearchPage {
    pub search: SearchBox,
    pub search_href: String,
    pub current_href: String,
}

#[derive(Template)]
#[template(path = "pages/branch.html")]
pub struct BranchPage {
    pub repo: String,
    pub repo_href: String,
    pub search_href: String,
    pub branch: String,
    pub current_href: String,
    pub more_href: String,
    pub total: usize,
    pub commits: Vec<CommitLinkRow>,
}

#[derive(Template)]
#[template(path = "pages/not_found.html")]
pub struct NotFoundPage;

#[derive(Template)]
#[template(path = "pages/internal_server_error.html")]
pub struct InternalServerErrorPage;

pub const INTERNAL_SERVER_ERROR_FALLBACK: &str = r#"<!doctype html>
<html lang="en">
    <head>
        <meta charset="utf-8" />
        <title>Internal Server Error</title>
    </head>
    <body></body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> PageMeta {
        PageMeta::new("Home", "A minimal dashboard")
    }

    fn content() -> String {
        IndexPage {
            search_href: "/search".to_owned(),
            repos: vec![
                RepoRow {
                    name: "alpha".to_owned(),
                    href: "/repos/alpha".to_owned(),
                    commits_href: "/commits/alpha".to_owned(),
                    commits: 12,
                    size: "1.5 MiB".to_owned(),
                    size_bytes: 1_572_864,
                    recent: vec![
                        CommitLinkRow {
                            hash: "def5678".to_owned(),
                            summary: "Latest change".to_owned(),
                            when: "7w ago".to_owned(),
                            href: "/commits/alpha/def5678".to_owned(),
                            is_new: false,
                            divider_before: false,
                        },
                        CommitLinkRow {
                            hash: "abc1234".to_owned(),
                            summary: "Initial change".to_owned(),
                            when: "8w ago".to_owned(),
                            href: "/commits/alpha/abc1234".to_owned(),
                            is_new: false,
                            divider_before: false,
                        },
                    ],
                },
                RepoRow {
                    name: "beta".to_owned(),
                    href: "/repos/beta".to_owned(),
                    commits_href: "/commits/beta".to_owned(),
                    commits: 0,
                    size: "0 B".to_owned(),
                    size_bytes: 0,
                    recent: vec![],
                },
            ],
        }
        .render()
        .unwrap()
    }

    #[test]
    fn content_uses_the_dashboard_layout() {
        let html = content();

        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a></h1>"
        ));
        assert!(html.contains("<p class=\"page-meta\">2 repositories</p>"));
        assert!(!html.contains("Featured"));
        assert!(!html.contains("<h2>Stats</h2>"));
        assert!(!html.contains("<dt>repositories</dt>"));
        assert!(!html.contains("<h2>Repos</h2>"));
        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("alpha"));
        assert!(html.contains(
            "<p class=\"repo-meta\"><a class=\"repo-link\" href=\"/commits/alpha\">12 commits</a>, 1.5 MiB</p>"
        ));
        assert!(html.contains("class=\"commit-list\""));
        assert!(html.contains("class=\"commit-hash\""));
        assert!(html.contains("class=\"commit-message\""));
        assert!(html.contains("class=\"commit-time\""));
        assert!(html.contains("href=\"/commits/alpha/def5678\""));
        assert!(html.contains("def5678"));
        assert!(html.contains("Latest change"));
        assert!(html.contains("7w ago"));
        assert!(!html.contains("<h2>Activity</h2>"));
        assert!(!html.contains("<dt>main</dt>"));
        assert!(!html.contains("<dt>latest</dt>"));
        assert!(!html.contains("<dt>commits</dt>"));
        assert!(!html.contains("<dt>size</dt>"));
        assert!(html.contains("beta"));

        let count = html
            .find("<p class=\"page-meta\">2 repositories</p>")
            .unwrap();
        let repos = html.find("class=\"repo-grid\"").unwrap();

        assert!(count < repos);
        assert!(!html.contains("class=\"columns\""));
        assert!(html.contains("class=\"repo-grid\""));
        assert!(!html.contains("/.git"));
    }

    #[test]
    fn branches_page_renders_paginated_branches() {
        let html = BranchesPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha".to_owned(),
            current_href: "/branches/alpha?page=2".to_owned(),
            total: 60,
            page: 2,
            pages: 2,
            first: "/branches/alpha".to_owned(),
            prev: Some("/branches/alpha".to_owned()),
            next: None,
            last: "/branches/alpha?page=2".to_owned(),
            branches: vec![BranchRow {
                name: "main".to_owned(),
                href: "/branches/alpha/main".to_owned(),
                commits: vec![CommitLinkRow {
                    hash: "abc1234".to_owned(),
                    summary: "A commit".to_owned(),
                    when: "2d ago".to_owned(),
                    href: "/commits/alpha/abc1234".to_owned(),
                    is_new: false,
                    divider_before: false,
                }],
            }],
        }
        .render()
        .unwrap();

        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a> › <a class=\"repo-link\" href=\"/repos/alpha\">alpha</a> › <a class=\"repo-link\" href=\"/branches/alpha?page=2\">branches</a></h1>"
        ));
        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("href=\"/search?repo=alpha\""));
        assert!(html.contains("60 branches"));
        assert!(html.contains("href=\"/branches/alpha/main\""));
        assert!(html.contains("A commit"));
        assert!(html.contains("Page 2 of 2"));
        assert!(html.contains("<span class=\"pagination-disabled\">Next</span>"));
    }

    #[test]
    fn branch_page_renders_commits() {
        let html = BranchPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha&branch=main".to_owned(),
            branch: "main".to_owned(),
            current_href: "/branches/alpha/main".to_owned(),
            more_href: "/commits/alpha?branch=main".to_owned(),
            total: 4,
            commits: vec![CommitLinkRow {
                hash: "abc1234".to_owned(),
                summary: "A commit".to_owned(),
                when: "2d ago".to_owned(),
                href: "/commits/alpha/abc1234".to_owned(),
                is_new: false,
                divider_before: false,
            }],
        }
        .render()
        .unwrap();

        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a> › <a class=\"repo-link\" href=\"/repos/alpha\">alpha</a> › <a class=\"repo-link\" href=\"/branches/alpha/main\">branch main</a></h1>"
        ));
        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("href=\"/search?repo=alpha&#38;branch=main\""));
        assert!(html.contains("main"));
        assert!(html.contains("href=\"/commits/alpha/abc1234\""));
        assert!(html.contains("href=\"/commits/alpha?branch=main\">More commits</a>"));
        assert!(html.contains("A commit"));
    }

    #[test]
    fn commits_page_renders_pagination() {
        let html = CommitsPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha".to_owned(),
            branch: String::new(),
            branch_href: String::new(),
            current_href: "/commits/alpha?page=2".to_owned(),
            all_commits_href: "/commits/alpha".to_owned(),
            total: 120,
            page: 2,
            pages: 3,
            first: "/commits/alpha".to_owned(),
            prev: Some("/commits/alpha".to_owned()),
            next: Some("/commits/alpha?page=3".to_owned()),
            last: "/commits/alpha?page=3".to_owned(),
            commits: vec![CommitLinkRow {
                hash: "abc1234".to_owned(),
                summary: "A commit".to_owned(),
                when: "2d ago".to_owned(),
                href: "/commits/alpha/abc1234".to_owned(),
                is_new: false,
                divider_before: false,
            }],
        }
        .render()
        .unwrap();

        assert!(html.contains("alpha"));
        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("120 commits"));
        assert!(html.contains("href=\"/search?repo=alpha\""));
        assert!(html.contains("Page 2 of 3"));
        assert!(html.contains(">First</a>"));
        assert!(html.contains(">Previous</a>"));
        assert!(html.contains(">Next</a>"));
        assert!(html.contains(">Last</a>"));
        assert!(html.contains("href=\"/commits/alpha\""));
        assert!(html.contains("href=\"/commits/alpha?page=3\""));
        assert!(html.contains("A commit"));
    }

    #[test]
    fn commits_page_shows_the_branch_filter() {
        let html = CommitsPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha&branch=main".to_owned(),
            branch: "main".to_owned(),
            branch_href: "/branches/alpha/main".to_owned(),
            current_href: "/commits/alpha?branch=main".to_owned(),
            all_commits_href: "/commits/alpha".to_owned(),
            total: 1,
            page: 1,
            pages: 1,
            first: "/commits/alpha".to_owned(),
            prev: None,
            next: None,
            last: "/commits/alpha".to_owned(),
            commits: vec![],
        }
        .render()
        .unwrap();

        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("href=\"/search?repo=alpha&#38;branch=main\""));
        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a> › <a class=\"repo-link\" href=\"/repos/alpha\">alpha</a> › <a class=\"repo-link\" href=\"/branches/alpha/main\">branch main</a> › <a class=\"repo-link\" href=\"/commits/alpha?branch=main\">commits</a></h1>"
        ));
        assert!(html.contains("<p class=\"page-meta\">1 commit</p>"));
        assert!(html.contains("<a href=\"/commits/alpha\">All commits</a>"));

        let count = html.find("1 commit").unwrap();
        let all = html.find("All commits").unwrap();
        assert!(count < all);
    }

    #[test]
    fn pagination_disables_unavailable_buttons() {
        let first_page = CommitsPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha".to_owned(),
            branch: String::new(),
            branch_href: String::new(),
            current_href: "/commits/alpha".to_owned(),
            all_commits_href: "/commits/alpha".to_owned(),
            total: 120,
            page: 1,
            pages: 3,
            first: "/commits/alpha".to_owned(),
            prev: None,
            next: Some("/commits/alpha?page=2".to_owned()),
            last: "/commits/alpha?page=3".to_owned(),
            commits: vec![],
        }
        .render()
        .unwrap();

        assert!(first_page.contains("<span class=\"pagination-disabled\">First</span>"));
        assert!(first_page.contains("<span class=\"pagination-disabled\">Previous</span>"));
        assert!(first_page.contains(">Next</a>"));
        assert!(first_page.contains(">Last</a>"));

        let last_page = CommitsPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha".to_owned(),
            branch: String::new(),
            branch_href: String::new(),
            current_href: "/commits/alpha?page=3".to_owned(),
            all_commits_href: "/commits/alpha".to_owned(),
            total: 120,
            page: 3,
            pages: 3,
            first: "/commits/alpha".to_owned(),
            prev: Some("/commits/alpha?page=2".to_owned()),
            next: None,
            last: "/commits/alpha?page=3".to_owned(),
            commits: vec![],
        }
        .render()
        .unwrap();

        assert!(last_page.contains(">First</a>"));
        assert!(last_page.contains(">Previous</a>"));
        assert!(last_page.contains("<span class=\"pagination-disabled\">Next</span>"));
        assert!(last_page.contains("<span class=\"pagination-disabled\">Last</span>"));
    }

    #[test]
    fn search_page_renders() {
        let html = SearchPage {
            search: SearchBox {
                query: "needle".to_owned(),
                repo: "alpha".to_owned(),
                branch: "main".to_owned(),
                scope: "in alpha / main".to_owned(),
            },
            search_href: "/search?repo=alpha&branch=main".to_owned(),
            current_href: "/search?repo=alpha&branch=main&q=needle".to_owned(),
        }
        .render()
        .unwrap();

        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a> › <a class=\"repo-link\" href=\"/search?repo=alpha&#38;branch=main&#38;q=needle\">search</a></h1>"
        ));
        assert!(html.contains("href=\"/search?repo=alpha&#38;branch=main\""));
        assert!(html.contains("value=\"needle\""));
        assert!(html.contains("in alpha / main"));
    }

    #[test]
    fn commit_page_renders_commit_details() {
        let html = CommitPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            search_href: "/search?repo=alpha".to_owned(),
            short: "abc1234".to_owned(),
            branch: "main".to_owned(),
            branch_href: "/branches/alpha/main".to_owned(),
            current_href: "/commits/alpha/abc1234".to_owned(),
            summary: "Initial commit".to_owned(),
            message: "Initial commit\n\nWith a body.".to_owned(),
            author: "Test".to_owned(),
            when: "2d ago".to_owned(),
        }
        .render()
        .unwrap();

        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a> › <a class=\"repo-link\" href=\"/repos/alpha\">alpha</a> › <a class=\"repo-link\" href=\"/branches/alpha/main\">branch main</a> › <a class=\"repo-link\" href=\"/commits/alpha/abc1234\">commit abc1234</a></h1>"
        ));
        assert!(!html.contains("<h3>"));
        assert!(!html.contains("<dt>commit</dt>"));
        assert!(!html.contains("abc1234def5678"));
        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("href=\"/search?repo=alpha\""));
        assert!(html.contains("alpha"));
        assert!(html.contains("abc1234"));
        assert!(html.contains("Initial commit"));
        assert!(html.contains("With a body."));
        assert!(html.contains("Test"));
        assert!(html.contains("2d ago"));
    }

    #[test]
    fn repo_page_renders_branches_and_commits() {
        let html = RepoPage {
            name: "alpha".to_owned(),
            search_href: "/search?repo=alpha".to_owned(),
            current_href: "/repos/alpha".to_owned(),
            all_commits_href: "/commits/alpha".to_owned(),
            branches_href: "/branches/alpha".to_owned(),
            branches: vec![
                BranchRow {
                    name: "main".to_owned(),
                    href: "/branches/alpha/main".to_owned(),
                    commits: vec![CommitLinkRow {
                        hash: "abc1234".to_owned(),
                        summary: "Initial commit".to_owned(),
                        when: "2d ago".to_owned(),
                        href: "/commits/alpha/abc1234".to_owned(),
                        is_new: false,
                        divider_before: false,
                    }],
                },
                BranchRow {
                    name: "feature".to_owned(),
                    href: "/branches/alpha/feature".to_owned(),
                    commits: vec![
                        CommitLinkRow {
                            hash: "def5678".to_owned(),
                            summary: "Feature work".to_owned(),
                            when: "1d ago".to_owned(),
                            href: "/commits/alpha/def5678".to_owned(),
                            is_new: true,
                            divider_before: false,
                        },
                        CommitLinkRow {
                            hash: "abc1234".to_owned(),
                            summary: "Initial commit".to_owned(),
                            when: "2d ago".to_owned(),
                            href: "/commits/alpha/abc1234".to_owned(),
                            is_new: false,
                            divider_before: true,
                        },
                    ],
                },
            ],
            remotes: vec![RemoteRow {
                name: "origin".to_owned(),
                url: "git@github.com:jamen/alpha.git".to_owned(),
                delta: "↑2 ↓1".to_owned(),
                activity: "3h ago".to_owned(),
            }],
            remote_groups: vec![RemoteGroupRow {
                name: "all".to_owned(),
                remotes: vec!["origin".to_owned(), "mirror".to_owned()],
            }],
        }
        .render()
        .unwrap();

        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a> › <a class=\"repo-link\" href=\"/repos/alpha\">alpha</a></h1>"
        ));
        assert!(html.contains("<a href=\"/commits/alpha\">All commits</a>"));
        assert!(html.contains("class=\"repo-columns\""));
        assert!(html.contains("<a class=\"repo-link\" href=\"/branches/alpha\">Branches</a>"));
        assert!(html.contains("<h3>Remotes</h3>"));
        assert!(html.contains("<a class=\"show-more\" href=\"/branches/alpha\">All branches</a>"));
        assert!(html.contains("alpha"));
        assert!(html.contains("main"));
        assert!(html.contains("feature"));
        assert!(html.contains("href=\"/branches/alpha/main\""));
        assert!(html.contains("href=\"/commits/alpha/abc1234\""));
        assert!(html.contains("Feature work"));
        assert!(html.contains("class=\"commit-divider\""));
        assert!(html.contains("aria-hidden=\"true\""));
        assert!(html.contains("origin"));
        assert!(html.contains("git@github.com:jamen/alpha.git"));
        assert!(html.contains("↑2 ↓1"));
        assert!(html.contains("3h ago"));
        assert!(html.contains("all"));
        assert!(html.contains("origin mirror"));
        assert!(html.contains("All branches"));
        assert!(!html.contains("working tree"));
    }

    #[test]
    fn app_shell_composes_a_document() {
        let html = AppShell {
            meta: &meta(),
            shell_id: "app.test",
            css: "CSS_MARKER",
            js: "JS_MARKER",
            search: SearchBox::all(),
            version: "test version",
            content: &content(),
        }
        .render()
        .unwrap();

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<meta charset=\"utf-8\""));
        assert!(html.contains("name=\"viewport\""));
        assert!(html.contains("rel=\"icon\""));
        assert!(html.contains("CSS_MARKER") && html.contains("JS_MARKER"));
        assert!(html.contains("<title id=\"title\">Home</title>"));
        assert!(html.contains("<main id=\"main\" class=\"container\">"));
        assert!(html.contains(
            "<h1 class=\"title\"><a class=\"repo-link\" href=\"/\">Repositories</a></h1>"
        ));
        assert!(html.contains("data-search-overlay"));
        assert!(html.contains("href=\"/search\""));
        assert!(html.contains("<footer class=\"site-footer\">"));
        assert!(html.contains("Back to top"));
        assert!(html.contains("test version"));
    }

    #[test]
    fn partial_envelope_is_named_parts_only() {
        let html = Partial {
            meta: &meta(),
            shell_id: "app.test",
            content: &content(),
        }
        .render()
        .unwrap();

        assert!(!html.contains("<!doctype"));
        assert!(!html.contains("CSS_MARKER") && !html.contains("<style"));
        assert!(!html.contains("<nav"));

        assert!(html.contains("<title id=\"title\">Home</title>"));
        assert!(html.contains("<meta id=\"description\""));
        assert!(html.contains("<meta id=\"shell\" name=\"shell\" content=\"app.test\""));
        assert!(html.contains("<main id=\"main\""));
    }

    #[test]
    fn both_envelopes_carry_identical_content() {
        let page = content();

        let document = AppShell {
            meta: &meta(),
            shell_id: "app.test",
            css: "",
            js: "",
            search: SearchBox::all(),
            version: "test version",
            content: &page,
        }
        .render()
        .unwrap();

        let partial = Partial {
            meta: &meta(),
            shell_id: "app.test",
            content: &page,
        }
        .render()
        .unwrap();

        assert!(document.contains(page.trim()));
        assert!(partial.contains(page.trim()));
    }
}

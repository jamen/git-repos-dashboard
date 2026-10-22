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

#[derive(Template)]
#[template(path = "shells/app.html")]
pub struct AppShell<'a> {
    pub meta: &'a PageMeta,
    pub shell_id: &'a str,
    pub css: &'a str,
    pub js: &'a str,
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
}

#[derive(Template)]
#[template(path = "pages/index.html")]
pub struct IndexPage {
    pub repos: Vec<RepoRow>,
}

#[derive(Template)]
#[template(path = "pages/commit.html")]
pub struct CommitPage {
    pub repo: String,
    pub repo_href: String,
    pub short: String,
    pub hash: String,
    pub summary: String,
    pub message: String,
    pub author: String,
    pub when: String,
}

pub struct BranchRow {
    pub name: String,
    pub commits: Vec<CommitLinkRow>,
}

pub struct RemoteRow {
    pub name: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "pages/repo.html")]
pub struct RepoPage {
    pub name: String,
    pub branches: Vec<BranchRow>,
    pub remotes: Vec<RemoteRow>,
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
            repos: vec![
                RepoRow {
                    name: "alpha".to_owned(),
                    href: "/repos/alpha".to_owned(),
                    commits: 12,
                    size: "1.5 MiB".to_owned(),
                    size_bytes: 1_572_864,
                    recent: vec![
                        CommitLinkRow {
                            hash: "def5678".to_owned(),
                            summary: "Latest change".to_owned(),
                            when: "7w ago".to_owned(),
                            href: "/commits/alpha/def5678".to_owned(),
                        },
                        CommitLinkRow {
                            hash: "abc1234".to_owned(),
                            summary: "Initial change".to_owned(),
                            when: "8w ago".to_owned(),
                            href: "/commits/alpha/abc1234".to_owned(),
                        },
                    ],
                },
                RepoRow {
                    name: "beta".to_owned(),
                    href: "/repos/beta".to_owned(),
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

        assert!(html.contains("<h1 class=\"title\"><a href=\"/\">git-repos-dashboard</a></h1>"));
        assert!(html.contains("<p class=\"repo-count\">2 repositories</p>"));
        assert!(!html.contains("Featured"));
        assert!(!html.contains("<h2>Stats</h2>"));
        assert!(!html.contains("<dt>repositories</dt>"));
        assert!(!html.contains("<h2>Repos</h2>"));
        assert!(html.contains("href=\"/repos/alpha\""));
        assert!(html.contains("alpha"));
        assert!(html.contains("<p class=\"repo-meta\">12 commits, 1.5 MiB</p>"));
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
            .find("<p class=\"repo-count\">2 repositories</p>")
            .unwrap();
        let repos = html.find("class=\"repo-grid\"").unwrap();

        assert!(count < repos);
        assert!(!html.contains("class=\"columns\""));
        assert!(html.contains("class=\"repo-grid\""));
        assert!(html.contains("<footer class=\"site-footer\">"));
        assert!(!html.contains("/.git"));
    }

    #[test]
    fn commit_page_renders_commit_details() {
        let html = CommitPage {
            repo: "alpha".to_owned(),
            repo_href: "/repos/alpha".to_owned(),
            short: "abc1234".to_owned(),
            hash: "abc1234def5678".to_owned(),
            summary: "Initial commit".to_owned(),
            message: "Initial commit\n\nWith a body.".to_owned(),
            author: "Test".to_owned(),
            when: "2d ago".to_owned(),
        }
        .render()
        .unwrap();

        assert!(html.contains("<h2>Commit</h2>"));
        assert!(html.contains("href=\"/repos/alpha\""));
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
            branches: vec![
                BranchRow {
                    name: "main".to_owned(),
                    commits: vec![CommitLinkRow {
                        hash: "abc1234".to_owned(),
                        summary: "Initial commit".to_owned(),
                        when: "2d ago".to_owned(),
                        href: "/commits/alpha/abc1234".to_owned(),
                    }],
                },
                BranchRow {
                    name: "feature".to_owned(),
                    commits: vec![CommitLinkRow {
                        hash: "def5678".to_owned(),
                        summary: "Feature work".to_owned(),
                        when: "1d ago".to_owned(),
                        href: "/commits/alpha/def5678".to_owned(),
                    }],
                },
            ],
            remotes: vec![RemoteRow {
                name: "origin".to_owned(),
                url: "git@github.com:jamen/alpha.git".to_owned(),
            }],
        }
        .render()
        .unwrap();

        assert!(html.contains("class=\"repo-columns\""));
        assert!(html.contains("<h3>Branches</h3>"));
        assert!(html.contains("<h3>Remotes</h3>"));
        assert!(html.contains("alpha"));
        assert!(html.contains("main"));
        assert!(html.contains("feature"));
        assert!(html.contains("href=\"/commits/alpha/abc1234\""));
        assert!(html.contains("Feature work"));
        assert!(html.contains("origin"));
        assert!(html.contains("git@github.com:jamen/alpha.git"));
        assert!(html.contains("Show more"));
        assert!(!html.contains("working tree"));
    }

    #[test]
    fn app_shell_composes_a_document() {
        let html = AppShell {
            meta: &meta(),
            shell_id: "app.test",
            css: "CSS_MARKER",
            js: "JS_MARKER",
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
        assert!(html.contains("git-repos-dashboard"));
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

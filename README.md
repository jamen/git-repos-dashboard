# git-repos-dashboard

A minimal dashboard for managing a collection of Git repos.

## Running

```sh
cargo run -p web-server
```

The server listens on `localhost:8080`. Its database defaults to the
platform's local data directory:

- Linux: `$XDG_DATA_HOME/git-repos-dashboard/store.redb` (or
  `~/.local/share/git-repos-dashboard/store.redb`)
- macOS: `~/Library/Application Support/git-repos-dashboard/store.redb`
- Windows: `%LOCALAPPDATA%\git-repos-dashboard\store.redb`

Pass `--database` to override it. Static assets are embedded in the binary at
build time.

```sh
cargo run -p web-server -- --library ~/git --host 127.0.0.1:3000 --database data.redb
cargo test
```

`--library` points at a directory containing the Git repositories; it
defaults to `./library`. Working-tree repos (`<repo>/.git`) and bare repos
(`<repo>.git`) are scanned at startup and listed on the index page, ordered
by their most recent activity. Each repo shows its commit count, size, and up
to 3 recent commits with relative times. Repository pages list recent
branches with their commits: up to 3 per branch, totaling at least 5 (or 5
from the only branch), followed by a Remotes section showing local ahead/behind
status and latest reflog activity, plus configured remote groups with their
members space-separated. The Branches subheading
and “Show more” link go to the paginated `/branches/<repo>` list, which shows
each branch with up to 3 recent commits. Branch names link to a dedicated
`/branches/<repo>/<branch>` page with a compact commit list and a “More
commits” link. Commit counts
link to a paginated `/commits/<repo>` view, which also accepts
`?branch=<name>` to filter by branch. Branch-specific commit lists place
commits not reachable from the repository’s
default branch above an “existing history” divider. Bare repos use `HEAD`;
working trees resolve a remote’s `HEAD` branch or fall back through
`init.defaultBranch`, `main`, `master`, and `HEAD`. Individual commits link to
`/commits/<repo>/<hash>`, and repository names link to `/repos/<repo>`.
Metadata is read with `git2` (libgit2).

## Layout

| package | owns |
| --- | --- |
| `web-server` | Axum server: routing, configuration, wiring between packages |
| `templates` | Askama templates and page structs; renders a full document or a page's partials |
| `store` | Embedded `redb` key-value store; async API, synchronous storage |
| `web-client` | TypeScript partial router + minimal CSS reset, built with Vite; not a Cargo workspace member |

`web-server` builds the client assets in `build.rs` and embeds them, along
with the files under `packages/web-server/assets`, via `include_str!` and
`include_bytes!` in `assets.rs`.

Pages render in one of two envelopes:

```sh
curl -s --compressed http://localhost:8080              # full document
curl -s --compressed 'http://localhost:8080?partial=1'  # named partials only
```

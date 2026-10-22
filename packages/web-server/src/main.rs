mod assets;
mod library;
mod pages;

use argh::FromArgs;
use axum::{
    Router, extract::Request, http::HeaderValue, middleware::Next, response::Response, routing::get,
};
use derive_more::{Display, Error};
use std::{borrow::Cow, io, path::PathBuf, sync::Arc};
use store::{Store, StoreInitError};
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// git-repos-dashboard web server
#[derive(FromArgs)]
struct Cli {
    /// log filter directive
    #[argh(option)]
    log: Option<String>,

    /// address and port to listen on (default: localhost:8080)
    #[argh(option)]
    host: Option<String>,

    /// path to the database file (default: platform data directory)
    #[argh(option)]
    database: Option<PathBuf>,

    /// path to the directory containing the git repositories (default: ./library)
    #[argh(option)]
    library: Option<PathBuf>,
}

pub struct Context {
    pub store: Store,
    pub library: PathBuf,
    pub assets: &'static assets::Assets,
    pub shell_id: &'static str,
}

#[tokio::main]
async fn main() {
    let cli = argh::from_env::<Cli>();

    let log_directive = cli.log.clone().map(Cow::Owned).unwrap_or(Cow::Borrowed(""));

    let log_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .parse_lossy(log_directive.as_ref());

    tracing_subscriber::registry()
        .with(log_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    if let Err(error) = try_serve(cli).await {
        tracing::error!("{}", error);
    }
}

async fn try_serve(cli: Cli) -> Result<(), TryServeError> {
    use TryServeError as E;

    let mut builder = Store::builder();

    if let Some(database) = cli.database {
        builder = builder.database(database);
    }

    let store = builder.finish().await.map_err(E::Store)?;

    tracing::info!("using database {}", store.path().display());

    let library_path = cli.library.unwrap_or_else(|| PathBuf::from("library"));

    tracing::info!("using library {}", library_path.display());

    match library::scan(&library_path, &store).await {
        Ok(summary) => tracing::info!(
            "indexed {} repositories ({} removed)",
            summary.found,
            summary.removed
        ),
        Err(error) => tracing::error!("failed to scan the library: {error}"),
    }

    let ctx = Arc::new(Context {
        store,
        library: library_path,
        assets: &assets::ASSETS,
        shell_id: env!("SHELL_ID"),
    });

    tracing::info!("shell {}", ctx.shell_id);

    let host = cli
        .host
        .map(Cow::Owned)
        .unwrap_or(Cow::Borrowed("localhost:8080"));

    let listener = TcpListener::bind(host.as_ref()).await.map_err(E::Bind)?;

    match listener.local_addr() {
        Ok(addr) => tracing::info!("listening on {:?}", addr),
        Err(err) => tracing::warn!("failed to get listening address: {err}"),
    }

    let router = Router::new()
        .route("/", get(pages::index_page_handler))
        .route("/search", get(pages::search_page_handler))
        .route("/commits/{repo}", get(pages::commits_page_handler))
        .route("/commits/{repo}/{hash}", get(pages::commit_page_handler))
        .route("/branches/{repo}", get(pages::branches_page_handler))
        .route(
            "/branches/{repo}/{*branch}",
            get(pages::branch_page_handler),
        )
        .route("/repos/{repo}", get(pages::repo_page_handler))
        .nest("/assets", assets::Assets::router())
        .fallback(pages::not_found_page_handler)
        .layer(CompressionLayer::new().br(true).gzip(true))
        .layer(axum::middleware::from_fn(default_accept_encoding))
        .with_state(ctx);

    axum::serve(listener, router).await.map_err(E::Serve)?;

    Ok(())
}

#[derive(Debug, Display, Error)]
enum TryServeError {
    #[display("failed to open the store: {_0}")]
    Store(StoreInitError),
    #[display("failed to bind: {_0}")]
    Bind(io::Error),
    #[display("failed to serve: {_0}")]
    Serve(io::Error),
}

async fn default_accept_encoding(mut request: Request, next: Next) -> Response {
    if !request.headers().contains_key("accept-encoding") {
        request
            .headers_mut()
            .insert("accept-encoding", HeaderValue::from_static("gzip"));
    }

    next.run(request).await
}

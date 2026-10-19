use delilah_daemon::{api, config::Config, scheduler, state::AppState, store::Store};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    if let Err(e) = run().await {
        tracing::error!("{e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    match tokio::task::spawn_blocking(delilah_agents::env::init_login_path).await? {
        Some(path) => tracing::info!(%path, "agents get the login shell's PATH"),
        None => tracing::warn!("could not read the login shell's PATH; agents get the daemon's"),
    }
    std::fs::create_dir_all(&config.home)?;
    let store = Store::open(&config.database_path()).await?;

    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, home = %config.home.display(), "delilahd listening");
    let app = AppState::new(store, config);
    scheduler::spawn(app.clone());
    axum::serve(listener, api::router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

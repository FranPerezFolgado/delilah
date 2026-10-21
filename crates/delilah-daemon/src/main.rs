use delilah_daemon::{
    api, config::Config, pr_loop, scheduler, specs, state::AppState, store::Store,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // To the terminal (or launchd's log file), and to memory for the app's viewer.
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(delilah_daemon::logs::buffer()),
        )
        .init();

    // Read the binary's details now: after an update the file on disk is a new one.
    api::health_init();
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
    // Only the user: it holds every hook payload (tool inputs and outputs) and logs.
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config.home, std::fs::Permissions::from_mode(0o700))?;
    }
    let store = Store::open(&config.database_path()).await?;

    let listener = tokio::net::TcpListener::bind(config.addr).await?;
    tracing::info!(addr = %config.addr, home = %config.home.display(), "delilahd listening");
    let app = AppState::new(store, config);
    app.load_language().await;
    app.load_remote_control().await;
    scheduler::spawn(app.clone());
    pr_loop::spawn(app.clone());
    specs::spawn(app.clone());
    axum::serve(listener, api::router(app))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Ctrl+C in a terminal, or SIGTERM from launchd when the app stops the service.
async fn shutdown_signal() {
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                term.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate => {}
    }
    tracing::info!("shutting down");
}

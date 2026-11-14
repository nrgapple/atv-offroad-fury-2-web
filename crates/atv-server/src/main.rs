//! `fury-server`: settings from the environment (research/GAME_SERVER.md),
//! JSON logs to stdout, rooms saved across restarts.

use tokio::sync::watch;

fn main() -> std::process::ExitCode {
    // `--version` answers without starting anything or touching the state
    // directory (the deploy script and CI call it).
    match std::env::args().nth(1).as_deref() {
        None => serve(),
        Some("--version" | "-V") => {
            println!(
                "{}",
                atv_server::config::version_line(std::env::var("FURY_COMMIT").ok())
            );
            std::process::ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!(
                "fury-server: unknown argument {other:?}; settings come from FURY_* variables (research/GAME_SERVER.md), --version prints the version"
            );
            std::process::ExitCode::from(2)
        }
    }
}

#[tokio::main]
async fn serve() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_current_span(false)
        .init();
    let config = match atv_server::config::Config::from_vars(|name| std::env::var(name).ok()) {
        Ok(config) => config,
        Err(e) => {
            tracing::error!(event = "invalid_settings", error = %e);
            return std::process::ExitCode::from(2);
        }
    };
    let listener = match tokio::net::TcpListener::bind(config.listen).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!(event = "listen_failed", address = %config.listen, error = %e);
            return std::process::ExitCode::from(1);
        }
    };
    let shared = atv_server::Shared::new(config);
    if let Err(e) = std::fs::create_dir_all(&shared.config.state_dir) {
        tracing::error!(event = "state_dir_failed", error = %e);
        return std::process::ExitCode::from(1);
    }
    atv_server::restore(&shared);
    let (stop, stopped) = watch::channel(false);
    tokio::spawn(async move {
        atv_server::terminated().await;
        let _ = stop.send(true);
    });
    match atv_server::serve(shared, listener, stopped).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(event = "serve_failed", error = %e);
            std::process::ExitCode::from(1)
        }
    }
}

//! # emulator-server
//!
//! Master server orchestrator running the Blaze Redirector (:42127),
//! Core Blaze Server (:42128), and Mock Telemetry Sink (:443 / :8080).

pub mod blaze_server;
pub mod listener;
pub mod redirector_server;
pub mod telemetry_server;
pub mod tls;

pub use blaze_server::run_blaze_server;
pub use listener::{accept_dual_stream, DualStream};
pub use redirector_server::run_redirector_server;
pub use telemetry_server::run_telemetry_server;
pub use tls::create_self_signed_tls_acceptor;

use tokio::net::TcpListener;
use services_redirector::RedirectorConfig;

/// Starts all emulator server services concurrently on the specified ports.
pub async fn start_services(
    redirector_port: u16,
    blaze_port: u16,
    telemetry_port: u16,
    secure_redirect: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tls_acceptor = create_self_signed_tls_acceptor()?;

    let redir_listener = TcpListener::bind(format!("127.0.0.1:{}", redirector_port)).await?;
    let blaze_listener = TcpListener::bind(format!("127.0.0.1:{}", blaze_port)).await?;
    let telemetry_listener = TcpListener::bind(format!("127.0.0.1:{}", telemetry_port)).await?;

    tracing::info!("============================================================");
    tracing::info!("  FIFA 22 Frostbite 3 Clean-Room Multiplayer Emulator       ");
    tracing::info!("============================================================");
    tracing::info!("  * Redirector (gosredirector.ea.com) : Port {}", redirector_port);
    tracing::info!("  * Core Blaze Server                 : Port {}", blaze_port);
    tracing::info!("  * Telemetry Sink (river.data.ea.com): Port {}", telemetry_port);
    tracing::info!("  * Downgrade to Plain TCP            : {}", !secure_redirect);
    tracing::info!("============================================================");

    let redir_cfg = RedirectorConfig {
        host: "127.0.0.1".to_string(),
        port: blaze_port,
        secure: secure_redirect,
    };

    let redir_task = tokio::spawn(run_redirector_server(
        redir_listener,
        tls_acceptor.clone(),
        redir_cfg,
    ));

    let session_store = std::sync::Arc::new(services_blaze::SessionStore::new());

    let blaze_task = tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor.clone(),
        "127.0.0.1".to_string(),
        17498,
        session_store,
    ));

    let telemetry_task = tokio::spawn(run_telemetry_server(
        telemetry_listener,
        tls_acceptor,
    ));

    tokio::select! {
        _ = redir_task => {},
        _ = blaze_task => {},
        _ = telemetry_task => {},
    }

    Ok(())
}


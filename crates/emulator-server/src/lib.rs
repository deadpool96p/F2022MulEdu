//! # emulator-server
//!
//! Master server orchestrator running the Blaze Redirector (:42127),
//! Core Blaze Server (:42128), Mock Telemetry Sink (:443 / :8080),
//! QoS/STUN UDP Responder (:17498), and ProtoTunnel Relay Engine (:3659).

pub mod blaze_server;
pub mod control_server;
pub mod listener;
pub mod redirector_server;
pub mod telemetry_server;
pub mod tls;

pub use blaze_server::run_blaze_server;
pub use control_server::run_control_server;
pub use listener::{accept_dual_stream, DualStream};
pub use redirector_server::run_redirector_server;
pub use telemetry_server::run_telemetry_server;
pub use tls::create_self_signed_tls_acceptor;

pub use services_blaze::{GameRegistry, GameSession, SessionStore};
pub use services_qos_relay::{run_qos_server, run_relay_server, PeerTable};

use std::sync::Arc;
use tokio::net::{TcpListener, UdpSocket};
use services_redirector::RedirectorConfig;

/// Starts all emulator server services concurrently on the specified ports.
pub async fn start_services(
    redirector_port: u16,
    blaze_port: u16,
    telemetry_port: u16,
    secure_redirect: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    start_services_full(
        redirector_port,
        blaze_port,
        telemetry_port,
        17498, // Default DirtySDK QoS UDP port
        3659,  // Default DirtySDK NetGameDist / ProtoTunnel UDP port
        secure_redirect,
    )
    .await
}

/// Starts all emulator server services including custom QoS and Relay UDP ports.
pub async fn start_services_full(
    redirector_port: u16,
    blaze_port: u16,
    telemetry_port: u16,
    qos_port: u16,
    relay_port: u16,
    secure_redirect: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tls_acceptor = create_self_signed_tls_acceptor()?;

    let redir_listener = TcpListener::bind(format!("0.0.0.0:{}", redirector_port)).await?;
    let blaze_listener = TcpListener::bind(format!("0.0.0.0:{}", blaze_port)).await?;
    let telemetry_listener = match TcpListener::bind(format!("0.0.0.0:{}", telemetry_port)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("Could not bind telemetry to port {}: {:?}. Falling back to 8080", telemetry_port, e);
            TcpListener::bind("0.0.0.0:8080").await?
        }
    };
    let qos_socket = UdpSocket::bind(format!("0.0.0.0:{}", qos_port)).await?;
    let relay_socket = UdpSocket::bind(format!("0.0.0.0:{}", relay_port)).await?;
    let control_listener = TcpListener::bind("0.0.0.0:42129").await?;

    tracing::info!("============================================================");
    tracing::info!("  FIFA 22 Frostbite 3 Clean-Room Multiplayer Emulator       ");
    tracing::info!("============================================================");
    tracing::info!("  * Redirector (gosredirector.ea.com) : Port {}", redirector_port);
    tracing::info!("  * Core Blaze Server                 : Port {}", blaze_port);
    tracing::info!("  * Telemetry Sink (river.data.ea.com): Port {}", telemetry_port);
    tracing::info!("  * QoS / STUN Server (ProtoPing UDP) : Port {}", qos_port);
    tracing::info!("  * ProtoTunnel UDP Relay             : Port {}", relay_port);
    tracing::info!("  * Control Channel REST API          : Port 42129");
    tracing::info!("  * Secure Redirect (TLS on Blaze)    : {}", secure_redirect);
    tracing::info!("============================================================");

    let redir_cfg = RedirectorConfig {
        host: "127.0.0.1".to_string(),
        port: blaze_port,
        secure: secure_redirect,
    };

    let redir_task = tokio::spawn(run_redirector_server(
        redir_listener,
        tls_acceptor.clone(),
        redir_cfg.clone(),
    ));

    // Secondary Redirector listener on port 42230 (FIFA 22 standard redirector port)
    if redirector_port != 42230 {
        if let Ok(l) = TcpListener::bind("0.0.0.0:42230").await {
            tracing::info!("  * Secondary Redirector Listener     : Port 42230");
            tokio::spawn(run_redirector_server(
                l,
                tls_acceptor.clone(),
                redir_cfg.clone(),
            ));
        }
    }


    let session_store = Arc::new(SessionStore::new());
    let game_registry = Arc::new(GameRegistry::new());

    let blaze_task = tokio::spawn(run_blaze_server(
        blaze_listener,
        tls_acceptor.clone(),
        "127.0.0.1".to_string(),
        qos_port,
        session_store.clone(),
        game_registry.clone(),
    ));

    let telemetry_task = tokio::spawn(run_telemetry_server(
        telemetry_listener,
        tls_acceptor,
    ));

    let qos_task = tokio::spawn(async move {
        if let Err(e) = run_qos_server(qos_socket).await {
            tracing::error!("[QoS:17498] Server error: {}", e);
        }
    });

    let peer_table = Arc::new(PeerTable::new());
    let relay_task = tokio::spawn(async move {
        if let Err(e) = run_relay_server(relay_socket, peer_table).await {
            tracing::error!("[ProtoTunnel:3659] Relay error: {}", e);
        }
    });

    let control_task = tokio::spawn(run_control_server(
        control_listener,
        session_store.clone(),
        game_registry.clone(),
    ));

    tokio::select! {
        _ = redir_task => {},
        _ = blaze_task => {},
        _ = telemetry_task => {},
        _ = qos_task => {},
        _ = relay_task => {},
        _ = control_task => {},
    }

    Ok(())
}

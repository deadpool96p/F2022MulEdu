//! # emulator-server
//!
//! Master server orchestrator running the Blaze Redirector (:42127),
//! Core Blaze Server (:42128), Mock Telemetry Sink (:443 / :8080),
//! QoS/STUN UDP Responder (:17498), and ProtoTunnel Relay Engine (:3659).

pub mod blaze_server;
pub mod blaze_tracer;
pub mod control_server;
pub mod crl_server;
pub mod http2_server;
pub mod listener;
pub mod redirector_server;
pub mod telemetry_server;
pub mod tls;

pub use blaze_server::run_blaze_server;
pub use control_server::run_control_server;
pub use crl_server::run_crl_server;
pub use listener::{accept_dual_stream, DualStream};
pub use redirector_server::run_redirector_server;
pub use telemetry_server::run_telemetry_server;
pub use tls::{
    create_self_signed_tls_acceptor, format_fingerprint, regenerate_certificates, sha256_digest,
};

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
    start_services_full_with_qos(
        redirector_port,
        blaze_port,
        telemetry_port,
        qos_port,
        relay_port,
        secure_redirect,
        services_redirector::DEFAULT_QOS_HOSTNAME.to_string(),
    )
    .await
}

/// Starts all emulator server services, advertising the given host name for
/// the QoS (`qosapi`) service in the redirector reply.
///
/// The host name is the one the client turns into the TLS SNI for the QoS
/// connection, so it has to be a name the emulator's leaf certificate
/// covers -- see `services_redirector::DEFAULT_QOS_HOSTNAME`.
pub async fn start_services_full_with_qos(
    redirector_port: u16,
    blaze_port: u16,
    telemetry_port: u16,
    qos_port: u16,
    relay_port: u16,
    secure_redirect: bool,
    qos_hostname: String,
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

    // The revocation list the leaf certificate points at. A strict TLS
    // client (WinHTTP, which is what FIFA 22's HTTP/2 stack is) fetches it
    // while validating the chain, and refuses the handshake with
    // `CRYPT_E_NO_REVOCATION_CHECK` when it cannot. It is served over
    // plain HTTP on loopback: an `https://` distribution point would make
    // the CRL fetch itself depend on a revocation check.
    let crl_listener = match TcpListener::bind(format!("127.0.0.1:{}", tls::CRL_PORT)).await {
        Ok(l) => Some(l),
        Err(e) => {
            tracing::warn!(
                "[CRL] Could not bind port {}: {:?}. Windows TLS clients will reject the chain.",
                tls::CRL_PORT,
                e
            );
            None
        }
    };
    let crl = Arc::new(tls::load_crl().unwrap_or_default());

    tracing::info!("============================================================");
    tracing::info!("  FIFA 22 Frostbite 3 Clean-Room Multiplayer Emulator       ");
    tracing::info!("============================================================");
    tracing::info!("  * Redirector (gosredirector.ea.com) : Port {}", redirector_port);
    tracing::info!("  * Core Blaze Server                 : Port {}", blaze_port);
    tracing::info!("  * Telemetry Sink (river.data.ea.com): Port {}", telemetry_port);
    tracing::info!("  * QoS / STUN Server (ProtoPing UDP) : Port {}", qos_port);
    tracing::info!("  * ProtoTunnel UDP Relay             : Port {}", relay_port);
    tracing::info!("  * Certificate Revocation List (HTTP) : Port {}", tls::CRL_PORT);
    tracing::info!("  * Control Channel REST API          : Port 42129");
    tracing::info!("  * Secure Redirect (TLS on Blaze)    : {}", secure_redirect);
    tracing::info!("============================================================");

    let redir_cfg = RedirectorConfig {
        host: "127.0.0.1".to_string(),
        port: blaze_port,
        secure: secure_redirect,
        // The QoS (ProtoPing) HTTP endpoints are served by the telemetry
        // sink, so that is the port the redirector advertises for the
        // `qosapi` service.
        qos_port: telemetry_port,
        qos_hostname,
        ..Default::default()
    };

    let redir_task = tokio::spawn(run_redirector_server(
        redir_listener,
        tls_acceptor.clone(),
        redir_cfg.clone(),
    ));

    // IPv6 Redirector Listener
    if let Ok(l) = TcpListener::bind(format!("[::]:{}", redirector_port)).await {
        tracing::info!("  * IPv6 Redirector Listener          : Port {}", redirector_port);
        tokio::spawn(run_redirector_server(
            l,
            tls_acceptor.clone(),
            redir_cfg.clone(),
        ));
    }

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
        if let Ok(l) = TcpListener::bind("[::]:42230").await {
            tracing::info!("  * Secondary IPv6 Redirector Listener: Port 42230");
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

    // IPv6 Blaze Listener
    if let Ok(l) = TcpListener::bind(format!("[::]:{}", blaze_port)).await {
        tracing::info!("  * IPv6 Blaze Listener               : Port {}", blaze_port);
        tokio::spawn(run_blaze_server(
            l,
            tls_acceptor.clone(),
            "127.0.0.1".to_string(),
            qos_port,
            session_store.clone(),
            game_registry.clone(),
        ));
    }

    // Secondary Blaze listener on port 44325 (EA standard production Blaze cluster port)
    if blaze_port != 44325 {
        if let Ok(l) = TcpListener::bind("0.0.0.0:44325").await {
            tracing::info!("  * Secondary Blaze Listener          : Port 44325");
            tokio::spawn(run_blaze_server(
                l,
                tls_acceptor.clone(),
                "127.0.0.1".to_string(),
                qos_port,
                session_store.clone(),
                game_registry.clone(),
            ));
        }
        if let Ok(l) = TcpListener::bind("[::]:44325").await {
            tracing::info!("  * Secondary IPv6 Blaze Listener     : Port 44325");
            tokio::spawn(run_blaze_server(
                l,
                tls_acceptor.clone(),
                "127.0.0.1".to_string(),
                qos_port,
                session_store.clone(),
                game_registry.clone(),
            ));
        }
    }

    // IPv6 Control Listener
    if let Ok(l) = TcpListener::bind("[::]:42129").await {
        tokio::spawn(run_control_server(l, session_store.clone(), game_registry.clone()));
    }

    let telemetry_task = tokio::spawn(run_telemetry_server(
        telemetry_listener,
        tls_acceptor.clone(),
    ));

    // IPv6 Telemetry Listener (:443)
    if let Ok(l) = TcpListener::bind(format!("[::]:{}", telemetry_port)).await {
        tracing::info!("  * IPv6 Telemetry Listener           : Port {}", telemetry_port);
        tokio::spawn(run_telemetry_server(l, tls_acceptor.clone()));
    }

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

    let crl_task = tokio::spawn(async move {
        match crl_listener {
            Some(listener) => run_crl_server(listener, crl).await,
            None => std::future::pending::<()>().await,
        }
    });

    tokio::select! {
        _ = redir_task => {},
        _ = blaze_task => {},
        _ = telemetry_task => {},
        _ = qos_task => {},
        _ = relay_task => {},
        _ = control_task => {},
        _ = crl_task => {},
    }

    Ok(())
}

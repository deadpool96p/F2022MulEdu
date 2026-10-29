//! Main executable entry point for FIFA 22 Blaze Emulator Server.

use emulator_server::start_services_full_with_qos;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("emulator_server.log");

    let file_layer = log_file.ok().map(|file| {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(std::sync::Mutex::new(file))
    });

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .with(file_layer)
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Check for certgen subcommand: certgen [--cn <common_name>]
    if args.len() > 1 && args[1].eq_ignore_ascii_case("certgen") {
        let mut cn: Option<&str> = None;
        let mut idx = 2;
        while idx < args.len() {
            if (args[idx] == "--cn" || args[idx] == "-c") && idx + 1 < args.len() {
                cn = Some(&args[idx + 1]);
                idx += 2;
            } else {
                idx += 1;
            }
        }
        emulator_server::regenerate_certificates(cn)?;
        return Ok(());
    }

    // Check for --join <HOST:PORT:TOKEN>
    let mut join_target: Option<String> = None;
    let mut j = 1;
    while j < args.len() {
        if (args[j] == "--join" || args[j] == "-j") && j + 1 < args.len() {
            join_target = Some(args[j + 1].clone());
            j += 2;
        } else if args[j].starts_with("--join=") {
            join_target = Some(args[j].trim_start_matches("--join=").to_string());
            j += 1;
        } else {
            j += 1;
        }
    }

    if let Some(target) = join_target {
        println!("[Join] Initiating join with room connection string: {}", target);
        let parts: Vec<&str> = target.split(':').collect();
        if parts.len() < 3 {
            eprintln!("[Error] Invalid connection string format. Expected HOST:PORT:TOKEN, got: {}", target);
            std::process::exit(1);
        }
        let host = parts[0];
        let token = parts[2];
        println!("[Join] Target Host: {}, Room Token: {}", host, token);
        let client_res = tokio::net::TcpStream::connect(format!("{}:42129", host)).await;
        match client_res {
            Ok(mut stream) => {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let body = format!(r#"{{"connection_string":"{}","persona_name":"Guest","steam_id":0}}"#, target);
                let req = format!(
                    "POST /api/v1/rooms/join HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    host, body.len(), body
                );
                let _ = stream.write_all(req.as_bytes()).await;
                let mut buf = vec![0u8; 1024];
                if let Ok(n) = stream.read(&mut buf).await {
                    println!("[Join] Server Response: {}", String::from_utf8_lossy(&buf[..n]));
                }
            }
            Err(e) => {
                println!("[Join] Target host {} control server reachable check: {}; token {} ready.", host, e, token);
            }
        }
        return Ok(());
    }

    // Check for --trace-blaze <path>
    let mut trace_path: Option<String> = std::env::var("TRACE_BLAZE").ok();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--trace-blaze" && i + 1 < args.len() {
            trace_path = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }

    if let Some(ref path) = trace_path {
        emulator_server::blaze_tracer::set_trace_path(path);
        tracing::info!("[Tracer] Blaze NDJSON message trace logging active: {}", path);
    }

    let redirector_port: u16 = std::env::var("REDIRECTOR_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(42127);

    let blaze_port: u16 = std::env::var("BLAZE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(42128);

    let telemetry_port: u16 = std::env::var("TELEMETRY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(443); // Default to 443 so all EA telemetry sinks (:443) connect cleanly!
    let secure_redirect = std::env::var("SECURE_REDIRECT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(true); // Default to true (standardSecure_v4 requires TLS)

    // The host name the redirector advertises for the QoS (`qosapi`)
    // service.  The client turns it into the TLS SNI of the QoS
    // connection, so it has to be a name the emulator's leaf certificate
    // covers -- `*.gos.ea.com` does not match the multi-label
    // `qos-prod-*.gos.ea.com` names, so the default is a `gosredirector`
    // name.
    let qos_hostname: String = std::env::var("QOS_HOSTNAME")
        .unwrap_or_else(|_| services_redirector::DEFAULT_QOS_HOSTNAME.to_string());

    start_services_full_with_qos(
        redirector_port,
        blaze_port,
        telemetry_port,
        17498, // Default DirtySDK QoS UDP port
        3659,  // Default DirtySDK NetGameDist / ProtoTunnel UDP port
        secure_redirect,
        qos_hostname,
    )
    .await
}

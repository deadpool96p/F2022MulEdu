//! Main executable entry point for FIFA 22 Blaze Emulator Server.

use emulator_server::start_services;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
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

    start_services(redirector_port, blaze_port, telemetry_port, secure_redirect).await
}

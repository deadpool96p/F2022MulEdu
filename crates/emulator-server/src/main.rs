//! Main executable entry point for FIFA 22 Blaze Emulator Server.

use emulator_server::start_services;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

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

    start_services(redirector_port, blaze_port, telemetry_port, false).await
}

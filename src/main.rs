//! Aura Swarm entry point.

use aura_swarm::{Swarm, SwarmConfig};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("aura=info".parse()?))
        .init();

    // Load config from environment
    let config = SwarmConfig::from_env();

    // Run the swarm
    Swarm::new(config).run().await
}

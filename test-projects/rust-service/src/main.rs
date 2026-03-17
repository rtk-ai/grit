mod auth;
mod config;
mod handler;
mod storage;

use config::Config;
use handler::handle_request;
use storage::init_db;
use tracing::{info, error};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("rust_service=debug,info")
        .init();

    let config = match Config::load_config(None) {
        Ok(cfg) => {
            info!("Configuration loaded successfully");
            cfg
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = config.validate_config() {
        error!("Invalid configuration: {}", e);
        std::process::exit(1);
    }

    let db = match init_db(&config.database_url).await {
        Ok(pool) => {
            info!("Database initialized successfully");
            pool
        }
        Err(e) => {
            error!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    let bind_addr = format!("{}:{}", config.host, config.port);
    info!("Starting server on {}", bind_addr);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to {}: {}", bind_addr, e);
            std::process::exit(1);
        }
    };

    info!("Server listening on {}", bind_addr);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let db = db.clone();
                let secret = config.jwt_secret.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_request(stream, &db, &secret).await {
                        error!("Error handling request from {}: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

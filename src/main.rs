// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;

mod controller_web;
mod controller_zig;
mod service_config;
mod service_storage;
mod service_upstream;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = "\
Usage: zorian --config=<path>

Options:
  --help             Show this help message
  --version          Show version
";

#[tokio::main]
async fn main() {
    let mut config_path = None;

    for arg in std::env::args().skip(1) {
        if arg == "--help" || arg == "-h" {
            print!("{HELP}");
            return;
        }
        if arg == "--version" || arg == "-V" {
            println!("zorian {VERSION}");
            return;
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            config_path = Some(PathBuf::from(path));
        }
    }

    let config_path = config_path.expect("missing --config argument");

    let config = match service_config::ConfigService::from_file(&config_path).await {
        Ok(config) => Arc::new(config),
        Err(service_config::ConfigError::Io(e)) => {
            eprintln!(
                "error: failed to read config file '{}': {e}",
                config_path.display()
            );
            std::process::exit(1);
        }
        Err(service_config::ConfigError::Parse(e)) => {
            eprintln!(
                "error: failed to parse config file '{}': {e}",
                config_path.display()
            );
            std::process::exit(1);
        }
    };
    let storage = Arc::new(service_storage::StorageService::new(config.clone()));
    let upstream = Arc::new(service_upstream::UpstreamService::new());

    let web_controller = Arc::new(controller_web::WebController::new());
    let zig_controller = Arc::new(controller_zig::ZigController::new(storage, upstream));

    let app = Router::new()
        .merge(web_controller.router())
        .merge(zig_controller.router());

    let listener = tokio::net::TcpListener::bind(config.addr()).await.unwrap();

    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

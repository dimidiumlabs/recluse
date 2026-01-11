// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::Router;
use std::sync::Arc;

mod controller_web;
mod controller_zig;
mod service_config;
mod service_storage;
mod service_upstream;

#[tokio::main]
async fn main() {
    let config = Arc::new(service_config::ConfigService::new());
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

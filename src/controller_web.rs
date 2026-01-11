// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use axum::{Router, extract, http, response, routing};
use std::sync::Arc;

/// Handles html pages rendering and static files
pub struct WebController {
    jinja: minijinja::Environment<'static>,
}

impl WebController {
    pub fn new() -> Self {
        let mut jinja = minijinja::Environment::new();
        jinja
            .add_template("index", include_str!("./index.html"))
            .unwrap();

        Self { jinja }
    }

    pub fn router(self: Arc<Self>) -> Router {
        Router::new()
            .route("/", routing::get(Self::index))
            .with_state(self)
    }

    async fn index(
        extract::State(controller): extract::State<Arc<Self>>,
    ) -> Result<response::Html<String>, http::StatusCode> {
        let template = controller.jinja.get_template("index").unwrap();
        let rendered = template.render(minijinja::context! {}).unwrap();

        Ok(response::Html(rendered))
    }
}

// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::{Arc, LazyLock};

use axum::extract;
use dimidiumlabs_server::{AssetCatalog, UiLayer};
use dimidiumlabs_ui::{
    APP_STYLESHEET_PATH, APPLE_TOUCH_ICON_PATH, Asset, CachePolicy, Document, FAVICON_ICO_PATH,
    FAVICON_SVG_PATH, MANIFEST_PATH, ROBOTS_PATH, css, image,
};
use maud::{Markup, Render};
use serde::Deserialize;
use tracing::error;

use repos::{GoBackend, ZigBackend};

use crate::ui::{
    STYLESHEET,
    pages::{
        GoRelease, GoReleaseFile, IndexPage, LicenseEntry as PageLicenseEntry,
        LicenseOverview as PageLicenseOverview, LicenseUse, LicensesPage, ZigRelease,
        ZigReleaseFile,
    },
};

fn application_assets() -> Vec<Asset> {
    vec![
        css(APP_STYLESHEET_PATH, STYLESHEET),
        image(
            FAVICON_SVG_PATH,
            "image/svg+xml",
            include_bytes!("assets/favicon.svg"),
        ),
        image(
            FAVICON_ICO_PATH,
            "image/x-icon",
            include_bytes!("assets/favicon.ico"),
        ),
        image(
            APPLE_TOUCH_ICON_PATH,
            "image/png",
            include_bytes!("assets/favicon-192.png"),
        ),
        image(
            "/-/assets/icon-192.png",
            "image/png",
            include_bytes!("assets/favicon-192.png"),
        ),
        image(
            "/-/assets/icon-512.png",
            "image/png",
            include_bytes!("assets/favicon-512.png"),
        ),
        Asset::embedded(
            MANIFEST_PATH,
            "application/manifest+json",
            include_bytes!("assets/manifest.webmanifest"),
            CachePolicy::Revalidate,
        ),
        Asset::embedded(
            ROBOTS_PATH,
            "text/plain; charset=utf-8",
            include_bytes!("assets/robots.txt"),
            CachePolicy::Revalidate,
        ),
    ]
}

#[derive(Deserialize)]
struct LicenseList {
    overview: Vec<LicenseOverview>,
    licenses: Vec<LicenseEntry>,
}

#[derive(Deserialize)]
struct LicenseOverview {
    id: String,
    name: String,
    count: usize,
}

#[derive(Deserialize)]
struct LicenseEntry {
    id: String,
    name: String,
    text: String,
    first_of_kind: bool,
    used_by: Vec<LicenseUsedBy>,
}

#[derive(Deserialize)]
struct LicenseUsedBy {
    #[serde(rename = "crate")]
    crate_: LicenseCrate,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct LicenseCrate {
    name: String,
    version: String,
    repository: Option<String>,
}

static LICENSES: LazyLock<LicenseList> = LazyLock::new(|| {
    let json = include_str!(concat!(env!("OUT_DIR"), "/licenses.json"));
    serde_json::from_str(json).expect("failed to parse licenses.json")
});

/// Handles html pages rendering and static files
pub struct WebController {
    zig: Option<Arc<ZigBackend>>,
    go: Option<Arc<GoBackend>>,
}

impl WebController {
    pub fn new(zig: Option<Arc<ZigBackend>>, go: Option<Arc<GoBackend>>) -> Self {
        Self { zig, go }
    }
}

impl WebController {
    pub fn router(self: Arc<Self>) -> axum::Router {
        let assets = AssetCatalog::new(application_assets())
            .expect("Tesor UI assets use unique canonical paths")
            .router::<Arc<Self>>();

        axum::Router::new()
            .route("/", axum::routing::get(Self::index))
            .route("/about/licenses", axum::routing::get(Self::licenses))
            .merge(assets)
            .layer(UiLayer::default())
            .with_state(self)
    }

    async fn index(extract::State(ctrl): extract::State<Arc<Self>>) -> Markup {
        let zig = if let Some(ref backend) = ctrl.zig {
            match backend.get_releases().await {
                Ok(releases) => releases
                    .into_iter()
                    .rev()
                    .map(|release| ZigRelease {
                        version: release.version,
                        date: release.meta.date,
                        docs: release.meta.docs,
                        std_docs: release.meta.std_docs,
                        notes: release.meta.notes,
                        files: release
                            .files
                            .into_iter()
                            .map(|file| ZigReleaseFile {
                                filename: file.filename,
                                target: file.meta.target,
                            })
                            .collect(),
                    })
                    .collect(),
                Err(e) => {
                    error!("failed to get zig versions: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let go = if let Some(ref backend) = ctrl.go {
            match backend.get_releases().await {
                Ok(releases) => releases
                    .into_iter()
                    .rev()
                    .map(|release| GoRelease {
                        version: release.version,
                        stable: release.meta.stable,
                        files: release
                            .files
                            .into_iter()
                            .map(|file| GoReleaseFile {
                                filename: file.filename,
                            })
                            .collect(),
                    })
                    .collect(),
                Err(e) => {
                    error!("failed to get go versions: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        Document::new(
            "Tesor — tiny & opinionated packages mirror",
            IndexPage { zig, go }.render(),
        )
        .with_manifest()
        .with_svg_icon()
        .with_apple_touch_icon()
        .render()
    }

    async fn licenses() -> Markup {
        let data = &*LICENSES;
        let overview = data
            .overview
            .iter()
            .map(|entry| PageLicenseOverview {
                id: entry.id.clone(),
                name: entry.name.clone(),
                count: entry.count,
            })
            .collect();
        let licenses = data
            .licenses
            .iter()
            .map(|entry| PageLicenseEntry {
                id: entry.id.clone(),
                name: entry.name.clone(),
                text: entry.text.clone(),
                first_of_kind: entry.first_of_kind,
                used_by: entry
                    .used_by
                    .iter()
                    .map(|usage| LicenseUse {
                        name: usage.crate_.name.clone(),
                        version: usage.crate_.version.clone(),
                        repository: usage.crate_.repository.clone(),
                    })
                    .collect(),
            })
            .collect();
        Document::new(
            "Third Party Licenses",
            LicensesPage {
                project_license: include_str!("../../../LICENSE"),
                overview,
                licenses,
            }
            .render(),
        )
        .with_manifest()
        .with_svg_icon()
        .with_apple_touch_icon()
        .render()
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use dimidiumlabs_server::DEFAULT_CSP;
    use dimidiumlabs_ui::GLOBAL_STYLESHEET_PATH;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn application_catalog_uses_canonical_asset_paths() {
        let catalog = AssetCatalog::new(application_assets()).expect("valid catalog");
        let stylesheet = catalog
            .get(APP_STYLESHEET_PATH)
            .expect("application stylesheet");
        let stylesheet = std::str::from_utf8(stylesheet.bytes()).expect("UTF-8 stylesheet");
        assert!(stylesheet.contains(":root{"), "missing application theme");
        assert!(catalog.get("/-/assets/icon-192.png").is_some());
        assert!(catalog.get("/-/assets/icon-512.png").is_some());
        assert!(catalog.get(ROBOTS_PATH).is_some());
    }

    #[test]
    fn generated_license_bundle_includes_embedded_plex_fonts() {
        let license = LICENSES
            .licenses
            .iter()
            .find(|license| license.id == "OFL-1.1")
            .expect("OFL license");
        assert!(
            license
                .used_by
                .iter()
                .any(|usage| usage.crate_.name == "dimidiumlabs-ui")
        );
        assert!(license.text.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }

    #[test]
    fn manifest_uses_registered_icon_urls() {
        let manifest = std::str::from_utf8(include_bytes!("assets/manifest.webmanifest"))
            .expect("UTF-8 manifest");
        assert!(manifest.contains("/-/assets/icon-192.png"));
        assert!(manifest.contains("/-/assets/icon-512.png"));
        assert!(!manifest.contains("\"/icon-"));
    }

    #[tokio::test]
    async fn shared_assets_keep_csp_etag_cache_and_head_behavior() {
        let router = Arc::new(WebController::new(None, None)).router();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(GLOBAL_STYLESHEET_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_SECURITY_POLICY],
            DEFAULT_CSP
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-cache");
        let etag = response.headers()[header::ETAG].clone();

        let head = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri(GLOBAL_STYLESHEET_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert_eq!(head.headers()[header::ETAG], etag);
        assert!(
            to_bytes(head.into_body(), usize::MAX)
                .await
                .unwrap()
                .is_empty()
        );

        let cached = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(GLOBAL_STYLESHEET_PATH)
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(cached.headers()[header::ETAG], etag);

        let font = router
            .oneshot(
                Request::builder()
                    .uri("/-/assets/fonts/ibm-plex-mono-variable-1.0.0-roman.woff2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(font.status(), StatusCode::OK);
        assert_eq!(font.headers()[header::CONTENT_TYPE], "font/woff2");
        assert_eq!(
            font.headers()[header::CACHE_CONTROL],
            "public, max-age=31536000, immutable"
        );
    }
}

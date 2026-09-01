// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::ui::classes;
use maud::{Markup, Render, html};

pub struct Document<'a> {
    pub title: &'a str,
    pub content: Markup,
}

impl Render for Document<'_> {
    fn render(&self) -> Markup {
        html! {
            (maud::DOCTYPE)
            html lang="en" {
                head {
                    meta charset="UTF-8";
                    meta http-equiv="X-UA-Compatible" content="ie=edge";
                    meta name="viewport" content="width=device-width,initial-scale=1";

                    title { (self.title) } link rel="stylesheet" href="/base.css";

                    link rel="manifest" href="/manifest.webmanifest";

                    link rel="icon" href="/favicon.ico" sizes="32x32";
                    link rel="icon" href="/favicon.svg" type="image/svg+xml";
                    link rel="apple-touch-icon" href="/apple-touch-icon.png";
                }

                body {
                    article class=(classes::document::DOCUMENT) { (self.content) }
                }
            }
        }
    }
}

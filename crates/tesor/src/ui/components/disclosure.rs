// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::ui::classes;
use maud::{Markup, Render, html};

pub struct Disclosure {
    pub summary: Markup,
    pub body: Markup,
}

impl Render for Disclosure {
    fn render(&self) -> Markup {
        html! {
            details class=(classes::disclosure::DISCLOSURE) {
                summary {
                    (self.summary.clone())
                }

                (self.body.clone())
            }
        }
    }
}

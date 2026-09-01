// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::ui::classes;
use maud::{Markup, Render, html};

pub struct Link<'a> {
    pub href: &'a str,
    pub label: Markup,
}

impl Render for Link<'_> {
    fn render(&self) -> Markup {
        html! {
            a class=(classes::link::LINK) href=(self.href) {
                (self.label.clone())
            }
        }
    }
}

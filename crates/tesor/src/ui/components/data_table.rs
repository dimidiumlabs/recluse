// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::ui::classes;
use maud::{Markup, Render, html};

pub struct DataTable<'a> {
    pub headers: &'a [&'a str],
    pub rows: Markup,
}

impl Render for DataTable<'_> {
    fn render(&self) -> Markup {
        html! {
            table class=(classes::data_table::TABLE) {
                thead {
                    tr {
                        @for header in self.headers {
                            th { (header) }
                        }
                    }
                }
                tbody {
                    (self.rows.clone())
                }
            }
        }
    }
}

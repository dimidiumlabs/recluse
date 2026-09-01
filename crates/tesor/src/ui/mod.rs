// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod components;
pub mod pages;

pub(crate) mod classes {
    include!(concat!(env!("OUT_DIR"), "/css_modules.rs"));
}

pub const STYLESHEET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stylesheet.css"));

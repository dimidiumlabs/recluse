// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod components;
pub mod pages;

pub(crate) mod classes {
    include!(concat!(env!("OUT_DIR"), "/css_modules.rs"));
}

mod generated_assets {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

pub const APPLICATION: &[dimidiumlabs_ui::Asset] = generated_assets::APPLICATION;

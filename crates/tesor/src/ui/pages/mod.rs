// SPDX-FileCopyrightText: 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

mod index;
mod licenses;

pub use index::{GoRelease, GoReleaseFile, IndexPage, ZigRelease, ZigReleaseFile};
pub use licenses::{LicenseEntry, LicenseOverview, LicenseUse, LicensesPage};

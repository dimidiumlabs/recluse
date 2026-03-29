// Copyright (c) 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

fn main() {
    println!("cargo::rerun-if-changed=../../Cargo.lock");

    let json =
        xtask::licenses::generate_json("Cargo.toml").expect("failed to generate licenses JSON");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out_dir}/licenses.json"), json)
        .expect("failed to write licenses.json");
}

// Copyright (c) 2026 Nikolay Govorov
// SPDX-License-Identifier: AGPL-3.0-or-later

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        Some("licenses") => {
            let json = xtask::licenses::generate_json("crates/recluse/Cargo.toml")?;
            println!("{json}");
            Ok(())
        }
        Some(cmd) => Err(format!("unknown command: {cmd}").into()),
        None => Err(
            "usage: cargo xtask <command>\n\ncommands:\n  licenses  Generate dependency license JSON"
                .into(),
        ),
    }
}

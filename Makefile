# SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
# SPDX-License-Identifier: AGPL-3.0-or-later

all: fmt clippy test deps licenses

.PHONY: licenses
licenses:
	reuse lint

.PHONY: fmt
fmt:
	cargo fmt --all --check

.PHONY: deps
deps:
	cargo deny check

.PHONY: clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: test
test:
	cargo build --release
	cargo test --all-features --release --locked

.PHONY: coverage
coverage:
	mkdir -p target/coverage/
	cargo llvm-cov --all-features --workspace --lcov --output-path target/coverage/lcov.info
	cargo llvm-cov --all-features --workspace --html --output-dir target/coverage/html

# SPDX-FileCopyrightText: 2026 Nikolay Govorov
# SPDX-License-Identifier: AGPL-3.0-or-later

CARGO_LLVM_COV_VERSION = ^0.8

all: legal fmt clippy test

.PHONY: legal
legal: signoff licenses

.PHONY: signoff
signoff:
	mise run signoff

.PHONY: setup
setup:
	cargo install cargo-llvm-cov@$(CARGO_LLVM_COV_VERSION)

.PHONY: licenses
licenses:
	mise run licenses

.PHONY: fmt
fmt:
	cargo fmt --all --check

.PHONY: deps
deps: licenses

.PHONY: clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: test
test:
	cargo build --release
	cargo test --all-features --release --locked

.PHONY: smoke
smoke:
	cargo run -p smoke

.PHONY: smoke-local
smoke-local:
	./tests/smoke/run-local.sh

.PHONY: coverage
coverage:
	mkdir -p target/coverage/
	cargo llvm-cov --all-features --workspace --lcov --output-path target/coverage/lcov.info
	cargo llvm-cov --all-features --workspace --html --output-dir target/coverage/html

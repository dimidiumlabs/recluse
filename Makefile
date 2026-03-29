# Copyright (c) 2026 Nikolay Govorov
# SPDX-License-Identifier: AGPL-3.0-or-later

CARGO_DENY_VERSION = ^0.19
CARGO_LLVM_COV_VERSION = ^0.8

all: fmt clippy test deps licenses

.PHONY: setup
setup:
	cargo install \
		cargo-deny@$(CARGO_DENY_VERSION) \
		cargo-llvm-cov@$(CARGO_LLVM_COV_VERSION)

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

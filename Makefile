# Homebrew's cargo ignores rust-toolchain.toml; prefer rustup's shim, which
# reads the pin, so `make check` lints with the clippy CI uses.
CARGO := $(shell command -v rustup >/dev/null 2>&1 && test -x "$(HOME)/.cargo/bin/cargo" && echo "$(HOME)/.cargo/bin/cargo" || echo cargo)

.PHONY: all check lint test build fmt toolchain

all: check

## Everything CI runs, in the order it runs it.
check: toolchain lint test

toolchain:
	@echo "using: $$($(CARGO) --version)  (pinned: $$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml))"

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --all-targets -- -D warnings

test:
	$(CARGO) test

build:
	$(CARGO) build --release

fmt:
	$(CARGO) fmt --all

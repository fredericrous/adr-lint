# Homebrew's cargo ignores rust-toolchain.toml; prefer rustup's shim, which
# reads the pin, so `make check` lints with the clippy CI uses.
CARGO := $(shell command -v rustup >/dev/null 2>&1 && test -x "$(HOME)/.cargo/bin/cargo" && echo "$(HOME)/.cargo/bin/cargo" || echo cargo)

.PHONY: all check lint test rules bump-tap npm-test build fmt toolchain msrv

all: check

## What CI's `rust` job runs, in the order it runs it; its `msrv` job runs
## `make msrv`.
check: toolchain lint test rules bump-tap npm-test

toolchain:
	@echo "using: $$($(CARGO) --version)  (pinned: $$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml))"

lint:
	$(CARGO) fmt --all --check
	$(CARGO) clippy --all-targets -- -D warnings

test:
	$(CARGO) test

## The embedded ast-grep rules against their fixtures. The version is the one
## `adr-lint check` demands, written once in AST_GREP_VERSION. Locally a
## missing or different ast-grep says SKIPPED; under CI it fails.
rules:
	@v=$$(cat AST_GREP_VERSION); \
	if [ "$$(ast-grep --version 2>/dev/null)" = "ast-grep $$v" ]; then \
		ast-grep test --skip-snapshot-tests; \
	else \
		echo "rules: ast-grep $$v is not installed — run 'brew install ast-grep'"; \
		echo "rules: SKIPPED, so this run does not prove the rule fixtures"; \
		test -z "$$CI"; \
	fi

## The release workflow's tap rewrite, against the seed formula.
bump-tap:
	python3 -I -m unittest discover -s tests -p 'test_bump_tap.py'

## The eslint preset's tests, from its lockfile.
npm-test:
	cd npm/eslint-config && npm ci && npm test

build:
	$(CARGO) build --release

fmt:
	$(CARGO) fmt --all

## The floor `rust-version` declares, compiled against. Locally it says
## SKIPPED when that toolchain is not installed, so a pass is never claimed.
msrv:
	@v=$$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml); \
	for t in $$v.0 $$v; do \
		if rustup run $$t cargo --version >/dev/null 2>&1; then \
			echo "msrv: checking with $$t"; exec rustup run $$t cargo check --locked; \
		fi; \
	done; \
	echo "msrv: rust $$v is not installed — run 'rustup toolchain install $$v.0'"; \
	echo "msrv: SKIPPED, so this run does not prove the floor"; \
	test -z "$$CI"

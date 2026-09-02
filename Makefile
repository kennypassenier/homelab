# ============================================================================
# Homelab v2 — build, test, and release management.
#
# Releasing is tag-driven: `make release VERSION=3.0.1` runs the full local
# gate, stamps the workspace version, commits, tags and pushes. GitHub CI
# (.github/workflows/release.yml) then re-runs the gate and publishes the
# binaries + SHA256SUMS as a GitHub Release. Rolling out to the host stays a
# separate, deliberate step: `homelab release-update` (B6) or press U in the
# TUI when the update badge appears.
# ============================================================================

.PHONY: help build test gate fmt clippy release host-binary hooks install

help:
	@echo "make build            debug build of the whole workspace"
	@echo "make test             run all tests"
	@echo "make gate             full local gate: fmt + clippy -D warnings + tests"
	@echo "make hooks            wire the git-native commit gates (once per clone)"
	@echo "make host-binary      release build of homelab-host for Debian 12 (via docker)"
	@echo "make release VERSION=x.y.z"
	@echo "                      gate, stamp workspace version, commit, tag vx.y.z, push."
	@echo "                      CI publishes the GitHub Release; roll out afterwards"
	@echo "                      with 'homelab release-update' (or U in the TUI)."

# One-time per clone: core.hooksPath is local config, never committed, so a
# fresh clone has no enforcement until this runs.
hooks:
	git config core.hooksPath .githooks
	@echo "git-native hooks active: $$(git config core.hooksPath)"

build:
	cargo build --workspace

test:
	cargo test --workspace

fmt:
	cargo fmt --all --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

gate: fmt clippy test

# Cross-build the host binary against Debian 12 glibc, same as CI does.
host-binary:
	docker run --rm -v $(PWD):/src -w /src rust:1-bookworm \
		cargo build --release -p homelab-host --target-dir target-debian
	@echo "→ target-debian/release/homelab-host"

release:
ifndef VERSION
	$(error usage: make release VERSION=x.y.z)
endif
	@case "$(VERSION)" in \
		[0-9]*.[0-9]*.[0-9]*) ;; \
		*) echo "VERSION must be plain x.y.z (no leading v)"; exit 1 ;; \
	esac
	@test -z "$$(git status --porcelain)" || { echo "working tree not clean — commit first"; exit 1; }
	@git rev-parse "v$(VERSION)" >/dev/null 2>&1 && { echo "tag v$(VERSION) already exists"; exit 1; } || true
	$(MAKE) gate
	@sed -i 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml
	@cargo update --workspace --quiet 2>/dev/null || cargo check --workspace --quiet
	@if ! git diff --quiet; then \
		git add Cargo.toml Cargo.lock && \
		git commit -m "release: v$(VERSION) [meta]"; \
	fi
	git tag -a "v$(VERSION)" -m "homelab v$(VERSION)"
	git push origin HEAD --follow-tags
	@echo ""
	@echo "✓ v$(VERSION) tagged and pushed — CI is building the release."
	@echo "  watch:    gh run watch"
	@echo "  roll out: homelab release-update   (after the release appears)"

# Kenny, 2026-09-02: `homelab` was never installed anywhere. Every document in
# this repository writes commands as `homelab <verb>`, and none of them worked
# from a shell — they only ever ran as `cargo run -q -p homelab-client --`,
# with the environment sourced first. That gap sat there for the whole project.
install: ## build the client and put it on PATH (~/.local/bin)
	@cargo build --release -p homelab-client
	@mkdir -p $(HOME)/.local/bin $(HOME)/.config/homelab
	@install -m 755 target/release/homelab $(HOME)/.local/bin/homelab
	@if [ ! -f $(HOME)/.config/homelab/env ] && [ -f .env ]; then \
		install -m 600 .env $(HOME)/.config/homelab/env; \
		echo "  · copied .env to ~/.config/homelab/env (0600)"; \
	fi
	@echo "✓ homelab installed to ~/.local/bin — try: homelab status"

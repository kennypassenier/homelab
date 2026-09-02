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
	# Release only from a base CI has actually passed.
	#
	# Branch protection on main requires the `check` and `msrv` jobs, but
	# `enforce_admins` is off so `make release` can push directly — which
	# means those required checks do not apply to a direct push at all. The
	# almanac project hit the same thing on 2026-09-02 and flagged it. On this
	# repository it has never been exercised (every pushed tip on main that
	# day was green), and the local gate below runs the full suite on every
	# commit — but "protected" overstated what was true, and a red tip could
	# have been tagged and shipped to the host with nothing objecting.
	#
	# This is the narrow fix that does not require weakening `make release`:
	# refuse to release from a HEAD whose CI is red. Unknown is allowed and
	# says so — a commit that was never pushed has no runs, and refusing that
	# would make the rule unusable offline.
	@st=$$(gh api "repos/{owner}/{repo}/commits/$$(git rev-parse HEAD)/check-runs" \
		--jq '[.check_runs[] | select(.name=="check" or .name=="msrv") | .conclusion] | join(",")' \
		2>/dev/null | tr -cd 'a-z_,'); \
	case ",$$st," in \
		*,failure,*|*,cancelled,*|*,timed_out,*) \
			echo "refusing: CI on HEAD says $$st — this would tag and ship a red base"; \
			exit 1 ;; \
		*,success,*) echo "  · CI on HEAD: $$st" ;; \
		*) echo "  · no CI verdict on HEAD yet (unpushed, or the API did not answer) — continuing" ;; \
	esac
	# DRY=1 stops here: every check has run, nothing has a side effect yet.
	#
	# There was no way to try this target without publishing, and on
	# 2026-09-02 I used it twice to test the CI guard above. Three fake tags
	# reached GitHub and two release workflows started before I noticed;
	# tags deleted, runs cancelled, no release published, but the version in
	# Cargo.toml had been bumped to 9.9.9 on main and had to be reverted.
	# A target whose only mode is "do it for real" gets rehearsed in
	# production, which is what happened.
	@if [ -n "$(DRY)" ]; then \
		echo "✓ dry run for v$(VERSION): checks passed, nothing tagged or pushed"; \
		exit 0; \
	fi
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
#
# `cargo install` rather than a copy into ~/.local/bin, and the reason is
# measured: ~/.local/bin reaches Kenny's PATH through /etc/profile, which only
# LOGIN shells read. The terminal inside Claude Desktop is not one, so the
# first version of this target installed a binary he still could not run.
# ~/.cargo/bin is exported by his own ~/.bashrc and is on fish's PATH too, so
# it holds in every shell he actually types in.
install: ## build the client and put it on PATH (~/.cargo/bin)
	@cargo install --path client --quiet
	@mkdir -p $(HOME)/.config/homelab
	@if [ ! -f $(HOME)/.config/homelab/env ] && [ -f .env ]; then \
		install -m 600 .env $(HOME)/.config/homelab/env; \
		echo "  · copied .env to ~/.config/homelab/env (0600)"; \
	fi
	@echo "✓ homelab installed to ~/.cargo/bin — try: homelab status"

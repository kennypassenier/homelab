# ============================================================================
# Homelab v2 — build, test, and release management.
#
# Releasing is tag-driven: `make release VERSION=3.0.1` runs the full local
# gate, stamps the workspace version, commits, tags and pushes. GitHub CI
# (.github/workflows/release.yml) then re-runs the gate and publishes the
# binaries + SHA256SUMS as a GitHub Release. Rolling out to the host stays a
# separate, deliberate step: `homelab release-update` (B6) or press u in the
# TUI when the update badge appears.
# ============================================================================

.PHONY: help build test gate admin-full release-binaries fmt clippy release host-binary hooks install diagrams

help:
	@echo "make build            debug build of the whole workspace"
	@echo "make test             run all tests"
	@echo "make gate             full local gate: fmt + clippy -D warnings + tests"
	@echo "make hooks            wire the git-native commit gates (once per clone)"
	@echo "make host-binary      release build of homelab-host for Debian 12 (via docker)"
	@echo "make release VERSION=x.y.z"
	@echo "                      gate, stamp workspace version, commit, tag vx.y.z, push."
	@echo "                      CI publishes the GitHub Release; roll out afterwards"
	@echo "                      with 'homelab release-update' (or u in the TUI)."

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

# Render every Mermaid diagram in the documentation; fails on a broken one.
# Needs node and a headless Chrome (see the script's header). Not part of the
# gate, which must run without either.
diagrams:
	scripts/check-diagrams.sh

# One gate, one script (2026-09-29): `make gate` is the commit hook's
# .claude/hooks/gates.sh with the whole test suite and no cache skips, so a
# green run on a clean checkout stamps .git/gate-pass for `make release`.
gate:
	GATE_FULL=1 GATE_SUITE=full .claude/hooks/gates.sh

# The dashboard's browser checks (tsc, prettier, node tests); Playwright joins
# with the `read` milestone.
admin-full:
	cd admin/web && { [ -d node_modules ] || npm ci --no-audit --no-fund; } && npm run --silent check

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
	@# A release is cut from main only: `git push origin HEAD` pushed whatever
	@# branch was checked out (expert panel 2026-09-27, make-release-guard-no-wait).
	@test "$$(git rev-parse --abbrev-ref HEAD)" = main || { echo "refusing: releases are cut from main, not $$(git rev-parse --abbrev-ref HEAD)"; exit 1; }
	@git rev-parse "v$(VERSION)" >/dev/null 2>&1 && { echo "tag v$(VERSION) already exists"; exit 1; } || true
	# Release only from a base CI has actually passed.
	#
	# Branch protection on main requires the `check` job (msrv was removed
	# 2026-09-10, d318ada), but
	# `enforce_admins` is off so `make release` can push directly — which
	# means those required checks do not apply to a direct push at all. The
	# almanac project hit the same thing on 2026-09-02 and flagged it. On this
	# repository it has never been exercised (every pushed tip on main that
	# day was green), and the local gate below runs the full suite on every
	# commit — but "protected" overstated what was true, and a red tip could
	# have been tagged and shipped to the host with nothing objecting.
	#
	# Reads EVERY check on the commit, not `check` by name. The
	# almanac project sat on four days of red CI across seven releases
	# because its `gates` job was green and a second job nobody was reading
	# was not — and a guard that knows two job names by heart would have
	# missed a third exactly the same way.
	#
	# This is the narrow fix that does not require weakening `make release`:
	# refuse to release from a HEAD whose CI is red. Unknown is allowed and
	# says so — a commit that was never pushed has no runs, and refusing that
	# would make the rule unusable offline.
	@# Wait while CI on HEAD is still running: measured on 2026-09-27, the
	@# day's releases were tagged while the push's own CI was mid-run, so the
	@# guard below read "no verdict" and let every one through
	@# (make-release-guard-no-wait). 20 minutes at most. An unpushed HEAD
	@# makes the API answer an error body, not a number: that is no runs, not
	@# a reason to wait (it waited the full 20 minutes on 2026-09-27).
	@for i in $$(seq 1 40); do \
		p=$$(gh api "repos/{owner}/{repo}/commits/$$(git rev-parse HEAD)/check-runs" \
			--jq '[.check_runs[] | select(.status != "completed")] | length' 2>/dev/null); \
		case "$$p" in ''|*[!0-9]*) p=0 ;; esac; \
		[ "$$p" -eq 0 ] && break; \
		[ $$i -eq 1 ] && echo "  · CI on HEAD still running ($$p check(s)) — waiting"; \
		sleep 30; \
	done
	@st=$$(gh api "repos/{owner}/{repo}/commits/$$(git rev-parse HEAD)/check-runs" \
		--jq '[.check_runs[] | select(.app.slug != "dependabot") | .conclusion] | join(",")' \
		2>/dev/null | tr -cd 'a-z_,'); \
	case ",$$st," in \
		*,failure,*|*,cancelled,*|*,timed_out,*) \
			echo "refusing: CI on HEAD says $$st — this would tag and ship a red base"; \
			exit 1 ;; \
		*,success,*) echo "  · CI on HEAD: $$st" ;; \
		*) echo "  · no CI verdict on HEAD yet (unpushed, or the API did not answer) — continuing" ;; \
	esac
	# gap-30 (Kenny, Phase 9 form 2026-09-27: "In make release"): the MSRV
	# Cargo.toml promises is checked once per release, with that compiler.
	# The CI job that did this was removed on 2026-09-10 (d318ada) and nothing
	# took its place. `rustup toolchain install <msrv> --profile minimal`
	# once per machine.
	@msrv=$$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml); \
	echo "  · MSRV $$msrv: cargo +$$msrv check"; \
	rustup toolchain list | grep -q "^$$msrv" || { \
		echo "refusing: Rust $$msrv is not installed here, so the MSRV check cannot run"; \
		echo "  rustup toolchain install $$msrv --profile minimal"; exit 1; }; \
	cargo +$$msrv check --workspace --locked --quiet || { \
		echo "refusing: the code no longer builds with Rust $$msrv, which Cargo.toml promises (rust-version)"; \
		echo "  either fix the code or raise rust-version deliberately"; exit 1; }
	# DRY=1 stops here: every check has run, nothing has a side effect yet.
	#
	# `ifndef`, not a shell `exit 0`. The first version used the latter and
	# did nothing at all, because every recipe line is its own shell — the
	# exit ended that line successfully and make carried on to the tag and
	# the push. That is how a third fake tag reached GitHub while I was
	# testing the guard that was supposed to prevent exactly this.
	#
	# There was no rehearsal mode at all before 2026-09-02, and a target
	# whose only mode is "do it for real" gets rehearsed in production.
ifdef DRY
	@echo "✓ dry run for v$(VERSION): version, tag, CI and MSRV checks passed; the gate (fmt, clippy, tests) was NOT run; nothing tagged or pushed"
else
	@# Kenny, 2026-09-29: "drie keer dezelfde testrun is dom, dat moet naar
	@# één": skip the gate when `make gate` already passed on this clean tree
	@# with this toolchain (.githooks/gate-stamp fresh; no helper = run it).
	@if .githooks/gate-stamp fresh; then echo "gate already passed for this tree (stamp $$(git rev-parse --short 'HEAD^{tree}')), not running it again"; else $(MAKE) gate; fi
	@sed -i 's/^version = ".*"/version = "$(VERSION)"/' Cargo.toml
	@cargo update --workspace --quiet 2>/dev/null || cargo check --workspace --quiet
	@if ! git diff --quiet; then \
		git add Cargo.toml Cargo.lock && \
		git commit -m "release: v$(VERSION) [meta]"; \
	fi
	git tag -a "v$(VERSION)" -m "homelab v$(VERSION)"
	$(MAKE) release-binaries
	git push origin HEAD --follow-tags
	gh release create "v$(VERSION)" --verify-tag --title "homelab v$(VERSION)" --generate-notes \
		dist/homelab-host dist/homelab dist/homelab-admin dist/SHA256SUMS
	@echo ""
	@echo "✓ v$(VERSION) built here, tagged, pushed and published."
	@echo "  sign:     ~/Projects/workstation/bin/sign-releases homelab:v$(VERSION)"
	@echo "  roll out: homelab release-update   (after it is signed)"
endif

# release-build (Kenny, 2026-09-28: "Lokaal bouwen en uploaden"): the release
# binaries are built on this machine, in the same Debian image the GitHub
# release job used, from the tagged tree (`make release` refuses a dirty
# one). homelab-admin is built on its own: with chassis-rs in the same build,
# rustls would carry two crypto providers into the host and the client. The
# registry and git caches are this machine's, mounted, so a build does not
# download the world; the target directory is separate from the debug one.
# The image carries the Rust that rust-toolchain.toml pins, and the build
# uses the image's own install: with only the toolchain file, rustup in a
# fresh container downloaded 1.97 again on every build (measured 2026-09-29:
# 1 min 10 s per build, two builds per release).
DEBIAN_IMAGE ?= rust:1.97-bookworm
IMAGE_TOOLCHAIN = $(shell docker run --rm $(DEBIAN_IMAGE) rustup default 2>/dev/null | cut -d' ' -f1)
DOCKER_CARGO = docker run --rm --user $$(id -u):$$(id -g) \
	-v $(CURDIR):/src -w /src \
	-v $(HOME)/.cargo/registry:/usr/local/cargo/registry \
	-v $(HOME)/.cargo/git:/usr/local/cargo/git \
	-e CARGO_HOME=/usr/local/cargo -e HOME=/tmp \
	-e RUSTUP_TOOLCHAIN=$(IMAGE_TOOLCHAIN) \
	$(DEBIAN_IMAGE) cargo
release-binaries:
	$(DOCKER_CARGO) build --release --locked -p homelab-host -p homelab-client --target-dir target-debian
	$(DOCKER_CARGO) build --release --locked -p homelab-admin --target-dir target-debian-admin
	rm -rf dist && mkdir -p dist
	cp target-debian/release/homelab-host dist/homelab-host
	cp target-debian/release/homelab dist/homelab
	cp target-debian-admin/release/homelab-admin dist/homelab-admin
	cd dist && sha256sum homelab-host homelab homelab-admin > SHA256SUMS

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

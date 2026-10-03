# ============================================================================
# Homelab v2 — build, test, and release management.
#
# Everything runs on this machine (Kenny, 2026-09-29: builds and checks are
# local, GitHub only receives the result). `make release VERSION=3.0.1` runs
# the scanners, the MSRV check and the full local gate, stamps the workspace
# version, commits, tags, builds the binaries + SHA256SUMS, pushes and
# publishes them as a GitHub Release. Rolling out to the host stays a
# separate, deliberate step: `homelab release-update` (B6) or press u in the
# TUI when the update badge appears.
# ============================================================================

.PHONY: help build test gate gate-full check advisories secrets secrets-staged msrv scanners admin-full invariants release-binaries fmt clippy release host-binary hooks install diagrams host-drift fail-first

help:
	@echo "make build            debug build of the whole workspace"
	@echo "make test             run all tests"
	@echo "make gate             local gate: fmt + clippy -D warnings + the tests that moved"
	@echo "                      since the last recorded run (fix-187) — full on the first"
	@echo "                      run, after Cargo.lock/toolchain/gate changes, or when a"
	@echo "                      foundational crate (core, proto) changed"
	@echo "make gate-full        make gate, but always the whole suite (no carry)"
	@echo "make check            gate + advisories + secrets + msrv (what CI ran until 2026-09-29)"
	@echo "make hooks            wire the git-native commit gates (once per clone)"
	@echo "make host-binary      release build of homelab-host for Debian 12 (via docker)"
	@echo "make release VERSION=x.y.z"
	@echo "                      check, stamp version, commit, tag vx.y.z, build, push"
	@echo "                      and publish the GitHub Release; roll out afterwards"
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
# .claude/hooks/gates.sh with no cache skips, so a green run on a clean
# checkout stamps .git/gate-pass for `make release`. Since fix-187 (Kenny,
# 2026-10-02: "tests run once per release, then only the failures are
# rerun" — four full runs for one release on 2026-10-01 is what this
# closes) the test step itself is .githooks/gate-carry.sh: it reruns
# everything only the first time, after Cargo.lock/the toolchain/the gate's
# own definition changes, or when a crate most of the workspace depends on
# (core, proto) changed; otherwise it reruns exactly the tests that were
# failing plus the crates that actually changed, and says which on its own
# stdout. `make gate-full` (or GATE_CARRY_MODE=full) skips that decision.
gate:
	GATE_FULL=1 GATE_SUITE=full .claude/hooks/gates.sh

gate-full:
	GATE_CARRY_MODE=full GATE_FULL=1 GATE_SUITE=full .claude/hooks/gates.sh

# What .github/workflows/ci.yml ran on every push until 2026-09-29, when
# Kenny moved every build and check to his own machine ("alle builds lokaal").
# `make release` runs all of it; run it by hand for the verdict CI used to give.
check: gate advisories secrets msrv

# The two scanners are pinned release binaries with a pinned checksum, exactly
# as the CI jobs fetched them (fix-140), cached once per machine outside the
# repository so WSL and Garuda need no package for them.
SCANNERS := $(HOME)/.cache/homelab-scanners
DENY_VERSION := 0.20.2
DENY_SHA256 := 9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f
GITLEAKS_VERSION := 8.30.1
GITLEAKS_SHA256 := 551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb
DENY := $(SCANNERS)/cargo-deny-$(DENY_VERSION)
GITLEAKS := $(SCANNERS)/gitleaks-$(GITLEAKS_VERSION)

scanners: $(DENY) $(GITLEAKS)

$(DENY):
	@mkdir -p $(SCANNERS)
	@name=cargo-deny-$(DENY_VERSION)-x86_64-unknown-linux-musl; tmp=$$(mktemp -d); \
	curl -fsSL -o $$tmp/deny.tgz "https://github.com/EmbarkStudios/cargo-deny/releases/download/$(DENY_VERSION)/$$name.tar.gz" && \
	echo "$(DENY_SHA256)  $$tmp/deny.tgz" | sha256sum -c --quiet - && \
	tar xzf $$tmp/deny.tgz -C $$tmp "$$name/cargo-deny" && mv $$tmp/$$name/cargo-deny $@; \
	rc=$$?; rm -rf $$tmp; exit $$rc

$(GITLEAKS):
	@mkdir -p $(SCANNERS)
	@tmp=$$(mktemp -d); \
	curl -fsSL -o $$tmp/gitleaks.tgz "https://github.com/gitleaks/gitleaks/releases/download/v$(GITLEAKS_VERSION)/gitleaks_$(GITLEAKS_VERSION)_linux_x64.tar.gz" && \
	echo "$(GITLEAKS_SHA256)  $$tmp/gitleaks.tgz" | sha256sum -c --quiet - && \
	tar xzf $$tmp/gitleaks.tgz -C $$tmp gitleaks && mv $$tmp/gitleaks $@; \
	rc=$$?; rm -rf $$tmp; exit $$rc

# Known vulnerabilities in what Cargo.lock pins (F48: Dependabot alerts went
# unread for weeks; a refused release does not).
advisories: $(DENY)
	$(DENY) --locked check advisories

# Secrets anywhere in the history. check-secrets.sh at commit matches one
# shape and `--no-verify` skips it; this scans every commit. --redact keeps
# a found secret out of the terminal. False positives: .gitleaks.toml.
secrets: $(GITLEAKS)
	$(GITLEAKS) git --no-banner --redact --config .gitleaks.toml .

# The commit-time halves of the two scans (.githooks/pre-commit): only what
# is staged, so a commit pays seconds, not the full history. Security checks
# belong to the commit gate (standing rule 7), not only to the release.
secrets-staged: $(GITLEAKS)
	@$(GITLEAKS) git --pre-commit --staged --no-banner --redact --config .gitleaks.toml --log-level warn .

# gap-30 (Kenny, Phase 9 form 2026-09-27: "In make release"): the MSRV
# Cargo.toml promises, checked with that compiler; `+$msrv` overrides
# rust-toolchain.toml. `rustup toolchain install <msrv> --profile minimal`
# once per machine.
msrv:
	@msrv=$$(sed -n 's/^rust-version = "\(.*\)"/\1/p' Cargo.toml); \
	echo "  · MSRV $$msrv: cargo +$$msrv check"; \
	rustup toolchain list | grep -q "^$$msrv" || { \
		echo "refusing: Rust $$msrv is not installed here, so the MSRV check cannot run"; \
		echo "  rustup toolchain install $$msrv --profile minimal"; exit 1; }; \
	cargo +$$msrv check --workspace --locked --quiet || { \
		echo "refusing: the code no longer builds with Rust $$msrv, which Cargo.toml promises (rust-version)"; \
		echo "  either fix the code or raise rust-version deliberately"; exit 1; }

# The dashboard's browser checks (tsc, prettier, node tests); Playwright joins
# with the `read` milestone.
admin-full:
	cd admin/web && { [ -d node_modules ] || npm ci --no-audit --no-fund; } && npm run --silent check

# docs/INVARIANTS.md: the Playwright smoke against the admin dashboard's
# demo-host build, pinning the UI invariants Kenny has stated as "must
# always be so" (the step counter, Pause/Stop, the nav bar, the backup
# calendar skeleton, the job dialog). Builds its own binary and starts and
# stops its own throwaway server; touches no real host. Also wired into
# `make gate`'s full run via `.githooks/gate-carry.sh invariants`.
invariants:
	./scripts/invariants-run.sh

# Cross-build the host binary against Debian 12 glibc, same as the release does.
host-binary:
	docker run --rm -v $(PWD):/src -w /src rust:1-bookworm \
		cargo build --release -p homelab-host --target-dir target-debian
	@echo "→ target-debian/release/homelab-host"

# fix-guards-7: config/host.toml against the host's own host.toml, through
# this tree's client (`homelab host diff`: reads only, never writes). A
# refusal names every key; `homelab host apply` reconciles them.
host-drift:
	@if [ -n "$$HOST_DRIFT_OK" ]; then \
		echo "host drift check skipped on purpose: $$HOST_DRIFT_OK"; \
	elif cargo run -q -p homelab-client --bin homelab -- host diff </dev/null; then \
		true; \
	else \
		echo "RELEASE BLOCKED — the repository's config/host.toml and the host disagree (above), or the host did not answer." >&2; \
		echo "Reconcile with 'homelab host apply', or go ahead deliberately: HOST_DRIFT_OK=\"<why>\" make release …" >&2; \
		exit 1; \
	fi

# fix-guards-5: run the tests a branch added against the code before it.
# A test that passes there proves nothing about the fix (BASE defaults to
# the branch's merge-base with main).
fail-first:
	@scripts/fail-first.py $(BASE)

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
	# fix-guards-2: no release on top of an earlier release's unmeasured
	# rows (3.70.1 to 3.70.7 each went out on the last one's; 110 rows by
	# 2026-10-03). fix-guards-7: no release while config/host.toml and the
	# host's own host.toml disagree (fix-240: 600 in the repository, 120 on
	# the host, the row closed as done). Both read only; both have a named,
	# visible override: UNMEASURED_OK="<why>", HOST_DRIFT_OK="<why>".
	@python3 .githooks/check-register.py --release $(VERSION)
	$(MAKE) host-drift
	# The scanners and the MSRV check have no side effect, so they run before
	# DRY=1 stops. Until 2026-09-29 this spot asked GitHub for the CI verdict on HEAD
	# and refused a red base; there is no CI any more, and these are the checks
	# it ran (the gate follows below).
	$(MAKE) advisories secrets msrv
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
	@echo "✓ dry run for v$(VERSION): version, tag, advisories, secrets and MSRV checks passed; the gate (fmt, clippy, tests) was NOT run; nothing tagged or pushed"
else
	@# Kenny, 2026-09-29: "drie keer dezelfde testrun is dom, dat moet naar
	@# één": skip the gate when `make gate` already passed on this clean tree
	@# with this toolchain (.githooks/gate-stamp fresh; no helper = run it).
	@if .githooks/gate-stamp fresh; then echo "gate already passed for this tree (stamp $$(git rev-parse --short 'HEAD^{tree}')), not running it again"; else $(MAKE) gate; fi
	@# fix-187: name what the gate that just stamped (or was already fresh)
	@# actually ran — a full run, or a carried one and its base — so a
	@# release never looks like it ran the whole suite when it carried.
	@for f in rust node; do \
		if [ -f .git/gate-carry/$$f/last-run.txt ]; then echo "gate ($$f): $$(head -1 .git/gate-carry/$$f/last-run.txt)"; fi; \
	done
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

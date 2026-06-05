# =============================================================================
# Makefile - Homelab Rust Binaries & Release Management
# =============================================================================

# Directory for compiled applications
APPS_DIR := apps

# Application names
CLIENT_NAME := CLIENT
HOST_NAME := HOST
LXC_NAME := LXC

# Source directories for each application
CLIENT_SRC := client-app
HOST_SRC := host-daemon
LXC_SRC := lxc-daemon

# GitHub container registry settings
GHCR_OWNER ?= kennypassenier
GHCR_LXC_IMAGE := ghcr.io/$(GHCR_OWNER)/homelab-lxc-daemon

# Get current versions from Cargo.toml files.
# Use recursive expansion so the value is re-read after version-bump targets run.
CLIENT_VERSION = $(shell grep '^version' $(CLIENT_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)
HOST_VERSION = $(shell grep '^version' $(HOST_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)
LXC_VERSION = $(shell grep '^version' $(LXC_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)

# Define all targets that are not actual files
.PHONY: help build build-client build-client-windows build-host build-lxc clean
.PHONY: docker release-host release-client release-lxc version-check version-bump-host
.PHONY: version-bump-client version-bump-lxc show-versions docker-build-only docker-push
.PHONY: push release-all

# Help menu providing information about available commands
help:
	@echo "Homelab Build & Release Targets"
	@echo ""
	@echo "Build Targets:"
	@echo "  make build                  - Build all binaries (client, host, lxc)"
	@echo "  make build-client           - Build CLIENT (Linux)"
	@echo "  make build-client-windows   - Build CLIENT (Windows)"
	@echo "  make build-host             - Build HOST daemon"
	@echo "  make build-lxc              - Build LXC daemon"
	@echo ""
	@echo "Docker Targets:"
	@echo "  make docker                 - Build and push LXC image to GHCR"
	@echo "  make docker-build-only      - Build LXC image locally (no push)"
	@echo "  make docker-push            - Push LXC image to GHCR"
	@echo ""
	@echo "Release Targets:"
	@echo "  make push                   - Release everything (Host, Client, LXC, and Docker)"
	@echo "  make release-all            - Alias for make push"
	@echo "  make release-host           - Build HOST, auto-bump patch, create GitHub release"
	@echo "  make release-client         - Build CLIENT (Linux+Windows), auto-bump patch, create GitHub release"
	@echo "  make release-lxc            - Build LXC, auto-bump patch, create GitHub release & push image"
	@echo ""
	@echo "Utility Targets:"
	@echo "  make show-versions          - Display current versions"
	@echo "  make version-check          - Check git tags vs Cargo.toml versions"
	@echo "  make clean                  - Clean all build artifacts"

# Target to build all standard Linux binaries
build: build-client build-host build-lxc

# Build the client application for Linux
build-client:
	cd $(CLIENT_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	@tmp="$(APPS_DIR)/$(CLIENT_NAME).new"; \
	cp $(CLIENT_SRC)/target/release/$(CLIENT_NAME) "$$tmp" && \
	chmod +x "$$tmp" && \
	mv -f "$$tmp" $(APPS_DIR)/$(CLIENT_NAME)

# Add target and build the client application for Windows
build-client-windows:
	rustup target add x86_64-pc-windows-gnu
	cd $(CLIENT_SRC) && cargo build --release --target x86_64-pc-windows-gnu
	@mkdir -p $(APPS_DIR)
	cp $(CLIENT_SRC)/target/x86_64-pc-windows-gnu/release/$(CLIENT_NAME).exe $(APPS_DIR)/$(CLIENT_NAME).exe

# Build the host daemon for Linux
build-host:
	cd $(HOST_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	@tmp="$(APPS_DIR)/$(HOST_NAME).new"; \
	cp $(HOST_SRC)/target/release/$(HOST_NAME) "$$tmp" && \
	chmod +x "$$tmp" && \
	mv -f "$$tmp" $(APPS_DIR)/$(HOST_NAME)

# Build the LXC daemon for Linux
build-lxc:
	cd $(LXC_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	@tmp="$(APPS_DIR)/$(LXC_NAME).new"; \
	cp $(LXC_SRC)/target/release/$(LXC_NAME) "$$tmp" && \
	chmod +x "$$tmp" && \
	mv -f "$$tmp" $(APPS_DIR)/$(LXC_NAME)

# Print current application versions from their Cargo.toml files
show-versions:
	@echo "Current Cargo.toml versions:"
	@echo "  CLIENT: $(CLIENT_VERSION)"
	@echo "  HOST:   $(HOST_VERSION)"
	@echo "  LXC:    $(LXC_VERSION)"

# Compare local versions with the latest tags in the Git repository
version-check:
	@echo "Checking git tags vs Cargo.toml versions..."
	@echo ""
	@echo "Latest HOST tag:"
	@git tag -l 'host-daemon-v*' --sort=-version:refname | head -1 || echo "  (none found)"
	@echo "  Current HOST version: $(HOST_VERSION)"
	@echo ""
	@echo "Latest LXC tag:"
	@git tag -l 'lxc-daemon-v*' --sort=-version:refname | head -1 || echo "  (none found)"
	@echo "  Current LXC version: $(LXC_VERSION)"

# Increment the patch version for the host application by calling bash directly
version-bump-host:
	@bash ./scripts/shared/bump-patch-version.sh $(HOST_SRC)/Cargo.toml HOST

# Increment the patch version for the client application by calling bash directly
version-bump-client:
	@bash ./scripts/shared/bump-patch-version.sh $(CLIENT_SRC)/Cargo.toml CLIENT client

# Increment the patch version for the lxc application by calling bash directly
version-bump-lxc:
	@bash ./scripts/shared/bump-patch-version.sh $(LXC_SRC)/Cargo.toml LXC lxc-daemon

# Build the Docker image and immediately push it
docker: docker-build-only docker-push

# Build the LXC Docker image and tag it appropriately.
# Does NOT depend on build-lxc: the Docker image runs its own cargo build inside the container.
docker-build-only:
	@echo "Building LXC daemon Docker image..."
	docker build \
		-f $(LXC_SRC)/Dockerfile \
		-t $(GHCR_LXC_IMAGE):latest \
		-t $(GHCR_LXC_IMAGE):v$(LXC_VERSION) \
		-t $(GHCR_LXC_IMAGE):sha-$$(git rev-parse --short HEAD) \
		.
	@echo "Docker image built successfully"

# Push the tagged Docker images to the GitHub container registry
docker-push:
	@echo "Pushing LXC daemon image to GHCR..."
	@if ! command -v docker > /dev/null 2>&1; then \
		echo "docker not found; cannot push image"; \
		exit 1; \
	fi
	docker push $(GHCR_LXC_IMAGE):latest
	docker push $(GHCR_LXC_IMAGE):v$(LXC_VERSION)
	docker push $(GHCR_LXC_IMAGE):sha-$$(git rev-parse --short HEAD)
	@echo "Docker image pushed successfully"

# HOST and CLIENT always release on push.
# LXC release is best-effort: a Docker failure does not block the rest.
push: release-host release-client
	@echo ""
	@$(MAKE) release-lxc || echo "⚠  LXC release failed (non-fatal) — HOST and CLIENT were released successfully"
	@echo ""
	@echo "✓ Push complete"

release-all: push

# Release sequence for the Host daemon
release-host: version-bump-host build-host
	@echo "Releasing host-daemon v$(HOST_VERSION)..."
	@mkdir -p $(APPS_DIR)
	@tmp="$(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu.new"; \
	cp $(HOST_SRC)/target/release/$(HOST_NAME) "$$tmp" && \
	chmod +x "$$tmp" && \
	mv -f "$$tmp" $(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu
	@chmod +x $(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu
	@git add $(HOST_SRC)/Cargo.toml
	@git diff --cached --quiet || git commit -m "Bump host-daemon version to v$(HOST_VERSION)"
	@if git rev-parse -q --verify "refs/tags/host-daemon-v$(HOST_VERSION)" > /dev/null 2>&1; then \
		echo "Tag host-daemon-v$(HOST_VERSION) already exists; skipping"; \
	else \
		git tag "host-daemon-v$(HOST_VERSION)" -m "Release host-daemon v$(HOST_VERSION)"; \
	fi
	@git push origin HEAD
	@git push origin "host-daemon-v$(HOST_VERSION)" || true
	@if command -v gh > /dev/null 2>&1; then \
		if gh release view "host-daemon-v$(HOST_VERSION)" > /dev/null 2>&1; then \
			echo "GitHub Release host-daemon-v$(HOST_VERSION) already exists; skipping"; \
		else \
			gh release create "host-daemon-v$(HOST_VERSION)" \
				$(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu \
				--title "host-daemon v$(HOST_VERSION)" \
				--generate-notes; \
		fi; \
	else \
		echo "gh not found; skipped GitHub Release for host-daemon-v$(HOST_VERSION)"; \
	fi
	@echo "✓ HOST release complete: v$(HOST_VERSION)"

# Release sequence for the Client application
release-client: version-bump-client build-client build-client-windows
	@echo "Releasing client v$(CLIENT_VERSION)..."
	@mkdir -p $(APPS_DIR)
	@tmp="$(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu.new"; \
	cp $(CLIENT_SRC)/target/release/$(CLIENT_NAME) "$$tmp" && \
	chmod +x "$$tmp" && \
	mv -f "$$tmp" $(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu
	@chmod +x $(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu
	@cp $(CLIENT_SRC)/target/x86_64-pc-windows-gnu/release/$(CLIENT_NAME).exe $(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe
	@chmod +x $(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe
	@git add $(CLIENT_SRC)/Cargo.toml
	@git diff --cached --quiet || git commit -m "Bump client version to v$(CLIENT_VERSION)"
	@if git rev-parse -q --verify "refs/tags/client-v$(CLIENT_VERSION)" > /dev/null 2>&1; then \
		echo "Tag client-v$(CLIENT_VERSION) already exists; skipping"; \
	else \
		git tag "client-v$(CLIENT_VERSION)" -m "Release client v$(CLIENT_VERSION)"; \
	fi
	@git push origin HEAD
	@git push origin "client-v$(CLIENT_VERSION)" || true
	@if command -v gh > /dev/null 2>&1; then \
		if gh release view "client-v$(CLIENT_VERSION)" > /dev/null 2>&1; then \
			echo "GitHub Release client-v$(CLIENT_VERSION) already exists; skipping"; \
		else \
			gh release create "client-v$(CLIENT_VERSION)" \
				$(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu \
				$(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe \
				--title "CLIENT v$(CLIENT_VERSION)" \
				--generate-notes; \
		fi; \
	else \
		echo "gh not found; skipped GitHub Release for client-v$(CLIENT_VERSION)"; \
	fi
	@echo "✓ CLIENT release complete: v$(CLIENT_VERSION)"

# Release sequence for the LXC daemon.
# Binary release (git/tag/gh) is the primary goal.
# Docker image build+push is best-effort and does not block the binary release.
release-lxc: version-bump-lxc build-lxc
	@echo "Releasing lxc-daemon v$(LXC_VERSION)..."
	@mkdir -p $(APPS_DIR)
	@tmp="$(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu.new"; \
	cp $(LXC_SRC)/target/release/$(LXC_NAME) "$$tmp" && \
	chmod +x "$$tmp" && \
	mv -f "$$tmp" $(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu
	@chmod +x $(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu
	@git add $(LXC_SRC)/Cargo.toml
	@git diff --cached --quiet || git commit -m "Bump lxc-daemon version to v$(LXC_VERSION)"
	@if git rev-parse -q --verify "refs/tags/lxc-daemon-v$(LXC_VERSION)" > /dev/null 2>&1; then \
		echo "Tag lxc-daemon-v$(LXC_VERSION) already exists; skipping"; \
	else \
		git tag "lxc-daemon-v$(LXC_VERSION)" -m "Release lxc-daemon v$(LXC_VERSION)"; \
	fi
	@git push origin HEAD
	@git push origin "lxc-daemon-v$(LXC_VERSION)" || true
	@if command -v gh > /dev/null 2>&1; then \
		if gh release view "lxc-daemon-v$(LXC_VERSION)" > /dev/null 2>&1; then \
			echo "GitHub Release lxc-daemon-v$(LXC_VERSION) already exists; skipping"; \
		else \
			gh release create "lxc-daemon-v$(LXC_VERSION)" \
				$(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu \
				--title "lxc-daemon v$(LXC_VERSION)" \
				--notes "Docker image: $(GHCR_LXC_IMAGE):v$(LXC_VERSION)" \
				--generate-notes; \
		fi; \
	else \
		echo "gh not found; skipped GitHub Release for lxc-daemon-v$(LXC_VERSION)"; \
	fi
	@echo "✓ LXC binary release complete: v$(LXC_VERSION)"
	@echo "Building and pushing Docker image (best-effort)..."
	@$(MAKE) docker || echo "⚠  Docker push failed (non-fatal) — LXC binary release succeeded"

# Clean up all compiled artifacts across the repository
clean:
	cargo clean --manifest-path $(CLIENT_SRC)/Cargo.toml
	cargo clean --manifest-path $(HOST_SRC)/Cargo.toml
	cargo clean --manifest-path $(LXC_SRC)/Cargo.toml
	@rm -f $(APPS_DIR)/$(CLIENT_NAME) $(APPS_DIR)/$(HOST_NAME) $(APPS_DIR)/$(LXC_NAME)
	@rm -f $(APPS_DIR)/$(CLIENT_NAME).exe
	@rm -f $(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu
	@rm -f $(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe
	@rm -f $(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu
	@rm -f $(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu
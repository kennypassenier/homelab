# =============================================================================
# Makefile — Homelab Rust Binaries & Release Management
# =============================================================================

APPS_DIR := apps
CLIENT_NAME := CLIENT
HOST_NAME := HOST
LXC_NAME := LXC
CLIENT_SRC := client-app
HOST_SRC := host-daemon
LXC_SRC := lxc-daemon

# GitHub container registry
GHCR_OWNER ?= kennypassenier
GHCR_LXC_IMAGE := ghcr.io/$(GHCR_OWNER)/homelab-lxc-daemon

# Get current versions from Cargo.toml
CLIENT_VERSION := $(shell grep '^version' $(CLIENT_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)
HOST_VERSION := $(shell grep '^version' $(HOST_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)
LXC_VERSION := $(shell grep '^version' $(LXC_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)

.PHONY: help build build-client build-client-windows build-host build-lxc clean
.PHONY: docker release-host release-client release-lxc version-check version-bump-host
.PHONY: version-bump-client version-bump-lxc show-versions docker-build-only docker-push

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
	@echo "  make release-host           - Build HOST, auto-bump patch, create GitHub release"
	@echo "  make release-client         - Build CLIENT (Linux+Windows), auto-bump patch, create GitHub release"
	@echo "  make release-lxc            - Build LXC, auto-bump patch, create GitHub release & push image"
	@echo ""
	@echo "Utility Targets:"
	@echo "  make show-versions          - Display current versions"
	@echo "  make version-check          - Check git tags vs Cargo.toml versions"
	@echo "  make clean                  - Clean all build artifacts"

build: build-client build-host build-lxc

build-client:
	cd $(CLIENT_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	cp $(CLIENT_SRC)/target/release/$(CLIENT_NAME) $(APPS_DIR)/$(CLIENT_NAME)

build-client-windows:
	rustup target add x86_64-pc-windows-gnu
	cd $(CLIENT_SRC) && cargo build --release --target x86_64-pc-windows-gnu
	@mkdir -p $(APPS_DIR)
	cp $(CLIENT_SRC)/target/x86_64-pc-windows-gnu/release/$(CLIENT_NAME).exe $(APPS_DIR)/$(CLIENT_NAME).exe

build-host:
	cd $(HOST_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	cp $(HOST_SRC)/target/release/$(HOST_NAME) $(APPS_DIR)/$(HOST_NAME)

build-lxc:
	cd $(LXC_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	cp $(LXC_SRC)/target/release/$(LXC_NAME) $(APPS_DIR)/$(LXC_NAME)

show-versions:
	@echo "Current Cargo.toml versions:"
	@echo "  CLIENT: $(CLIENT_VERSION)"
	@echo "  HOST:   $(HOST_VERSION)"
	@echo "  LXC:    $(LXC_VERSION)"

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

version-bump-host:
	@echo "Auto-bumping HOST version (patch increment)..."
	@./scripts/shared/bump-patch-version.sh $(HOST_SRC)/Cargo.toml HOST

version-bump-client:
	@echo "Auto-bumping CLIENT version (patch increment)..."
	@./scripts/shared/bump-patch-version.sh $(CLIENT_SRC)/Cargo.toml CLIENT

# Docker targets for LXC daemon image
docker: docker-build-only docker-push

docker-build-only: build-lxc
	@echo "Building LXC daemon Docker image..."
	docker build \
		-f $(LXC_SRC)/Dockerfile \
		-t $(GHCR_LXC_IMAGE):latest \
		-t $(GHCR_LXC_IMAGE):v$(LXC_VERSION) \
		-t $(GHCR_LXC_IMAGE):sha-$$(git rev-parse --short HEAD) \
		.
	@echo "✓ Docker image built successfully"

docker-push:
	@echo "Pushing LXC daemon image to GHCR..."
	@if ! command -v docker &> /dev/null; then \
		echo "✗ docker not found; cannot push image"; \
		exit 1; \
	fi
	docker push $(GHCR_LXC_IMAGE):latest
	docker push $(GHCR_LXC_IMAGE):v$(LXC_VERSION)
	docker push $(GHCR_LXC_IMAGE):sha-$$(git rev-parse --short HEAD)
	@echo "✓ Docker image pushed successfully"

# Release targets
# These auto-bump patch version, build, create git tag, and push to GitHub
release-host: version-bump-host build-host
	@echo "Creating HOST daemon release v$(HOST_VERSION)..."
	@if ! command -v gh &> /dev/null; then \
		echo "✗ gh (GitHub CLI) not found; cannot create release"; \
		exit 1; \
	fi
	@mkdir -p $(APPS_DIR)
	@cp $(HOST_SRC)/target/release/$(HOST_NAME) $(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu
	@chmod +x $(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu
	@git add $(HOST_SRC)/Cargo.toml && git commit -m "Bump host-daemon version to v$(HOST_VERSION)"
	@git tag "host-daemon-v$(HOST_VERSION)" -m "Release host-daemon v$(HOST_VERSION)"
	@git push origin main
	@git push origin "host-daemon-v$(HOST_VERSION)"
	@gh release create "host-daemon-v$(HOST_VERSION)" \
		$(APPS_DIR)/HOST-linux-x86_64-unknown-linux-gnu \
		--title "host-daemon v$(HOST_VERSION)" \
		--generate-notes
	@echo "✓ HOST release created successfully"

release-client: version-bump-client build-client build-client-windows
	@echo "Creating CLIENT release v$(CLIENT_VERSION)..."
	@if ! command -v gh &> /dev/null; then \
		echo "✗ gh (GitHub CLI) not found; cannot create release"; \
		exit 1; \
	fi
	@mkdir -p $(APPS_DIR)
	@cp $(CLIENT_SRC)/target/release/$(CLIENT_NAME) $(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu
	@chmod +x $(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu
	@cp $(CLIENT_SRC)/target/x86_64-pc-windows-gnu/release/$(CLIENT_NAME).exe $(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe
	@chmod +x $(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe
	@git add $(CLIENT_SRC)/Cargo.toml && git commit -m "Bump client version to v$(CLIENT_VERSION)"
	@git tag "client-v$(CLIENT_VERSION)" -m "Release client v$(CLIENT_VERSION)"
	@git push origin main
	@git push origin "client-v$(CLIENT_VERSION)"
	@gh release create "client-v$(CLIENT_VERSION)" \
		$(APPS_DIR)/CLIENT-linux-x86_64-unknown-linux-gnu \
		$(APPS_DIR)/CLIENT-windows-x86_64-pc-windows-gnu.exe \
		--title "CLIENT v$(CLIENT_VERSION)" \
		--generate-notes
	@echo "✓ CLIENT release created successfully"

release-lxc: version-bump-lxc docker-build-only docker-push build-lxc
	@echo "Creating LXC daemon release v$(LXC_VERSION)..."
	@if ! command -v gh &> /dev/null; then \
		echo "✗ gh (GitHub CLI) not found; cannot create release"; \
		exit 1; \
	fi
	@mkdir -p $(APPS_DIR)
	@cp $(LXC_SRC)/target/release/$(LXC_NAME) $(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu
	@chmod +x $(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu
	@git add $(LXC_SRC)/Cargo.toml && git commit -m "Bump lxc-daemon version to v$(LXC_VERSION)"
	@git tag "lxc-daemon-v$(LXC_VERSION)" -m "Release lxc-daemon v$(LXC_VERSION)"
	@git push origin main
	@git push origin "lxc-daemon-v$(LXC_VERSION)"
	@gh release create "lxc-daemon-v$(LXC_VERSION)" \
		$(APPS_DIR)/LXC-linux-x86_64-unknown-linux-gnu \
		--title "lxc-daemon v$(LXC_VERSION)" \
		--notes "Docker image: $(GHCR_LXC_IMAGE):v$(LXC_VERSION)" \
		--generate-notes
	@echo "✓ LXC release created and image pushed successfully"

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
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

# Latch settings
LATCH_AUTO_SYNC ?= 1
LATCH_SYNC_REQUIRED ?= 0

# Get current versions from Cargo.toml files
CLIENT_VERSION = $(shell grep '^version' $(CLIENT_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)
HOST_VERSION = $(shell grep '^version' $(HOST_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)
LXC_VERSION = $(shell grep '^version' $(LXC_SRC)/Cargo.toml | head -1 | cut -d'"' -f2)

.PHONY: help build build-client build-client-windows build-host build-lxc clean
.PHONY: release-host release-client release-lxc push release-all latch-sync-secrets

# --- Latch Secrets Sync ---
latch-sync-secrets:
	@echo "Syncing secrets with latch..."
	@if [ "$$CI" = "true" ] || [ "$(LATCH_AUTO_SYNC)" != "1" ]; then exit 0; fi
	@if command -v latch > /dev/null 2>&1; then \
		latch commit > /dev/null 2>&1; \
		latch push > /dev/null 2>&1; \
	fi

# --- Build Targets ---
build: latch-sync-secrets build-client build-host build-lxc

build-client: latch-sync-secrets
	cd $(CLIENT_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	@cp $(CLIENT_SRC)/target/release/$(CLIENT_NAME) $(APPS_DIR)/$(CLIENT_NAME)
	@chmod +x $(APPS_DIR)/$(CLIENT_NAME)

build-client-windows: latch-sync-secrets
	rustup target add x86_64-pc-windows-gnu
	cd $(CLIENT_SRC) && cargo build --release --target x86_64-pc-windows-gnu
	@mkdir -p $(APPS_DIR)
	@cp $(CLIENT_SRC)/target/x86_64-pc-windows-gnu/release/$(CLIENT_NAME).exe $(APPS_DIR)/$(CLIENT_NAME).exe

build-host: latch-sync-secrets
	cd $(HOST_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	@cp $(HOST_SRC)/target/release/$(HOST_NAME) $(APPS_DIR)/$(HOST_NAME)
	@chmod +x $(APPS_DIR)/$(HOST_NAME)

build-lxc: latch-sync-secrets
	cd $(LXC_SRC) && cargo build --release
	@mkdir -p $(APPS_DIR)
	@cp $(LXC_SRC)/target/release/$(LXC_NAME) $(APPS_DIR)/$(LXC_NAME)
	@chmod +x $(APPS_DIR)/$(LXC_NAME)

# --- Release Targets ---
# Clean is now part of the push flow to prevent accumulation of build artifacts
push: clean release-host release-client release-lxc

release-all: push

release-host: build-host
	@bash ./scripts/shared/bump-patch-version.sh $(HOST_SRC)/Cargo.toml HOST
	@git add $(HOST_SRC)/Cargo.toml
	@git commit -m "Bump host-daemon version to v$(HOST_VERSION)"
	@git tag "host-daemon-v$(HOST_VERSION)" -m "Release host-daemon v$(HOST_VERSION)"
	@git push origin HEAD --tags
	@gh release create "host-daemon-v$(HOST_VERSION)" $(APPS_DIR)/$(HOST_NAME) --title "host-daemon v$(HOST_VERSION)" --generate-notes
	@echo "✓ HOST release complete: v$(HOST_VERSION)"

release-client: build-client build-client-windows
	@bash ./scripts/shared/bump-patch-version.sh $(CLIENT_SRC)/Cargo.toml CLIENT client
	@git add $(CLIENT_SRC)/Cargo.toml
	@git commit -m "Bump client version to v$(CLIENT_VERSION)"
	@git tag "client-v$(CLIENT_VERSION)" -m "Release client v$(CLIENT_VERSION)"
	@git push origin HEAD --tags
	@gh release create "client-v$(CLIENT_VERSION)" $(APPS_DIR)/CLIENT $(APPS_DIR)/CLIENT.exe --title "CLIENT v$(CLIENT_VERSION)" --generate-notes
	@echo "✓ CLIENT release complete: v$(CLIENT_VERSION)"

release-lxc: build-lxc
	@bash ./scripts/shared/bump-patch-version.sh $(LXC_SRC)/Cargo.toml LXC lxc-daemon
	@git add $(LXC_SRC)/Cargo.toml
	@git commit -m "Bump lxc-daemon version to v$(LXC_VERSION)"
	@git tag "lxc-daemon-v$(LXC_VERSION)" -m "Release lxc-daemon v$(LXC_VERSION)"
	@git push origin HEAD --tags
	@gh release create "lxc-daemon-v$(LXC_VERSION)" $(APPS_DIR)/$(LXC_NAME) --title "lxc-daemon v$(LXC_VERSION)" --generate-notes
	@echo "✓ LXC release complete: v$(LXC_VERSION)"

# --- Utility ---
clean:
	@echo "Cleaning all target directories..."
	@find . -name "target" -type d -exec rm -rf {} +
	@rm -rf $(APPS_DIR)/*
	@echo "Clean complete"
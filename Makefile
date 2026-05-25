# =============================================================================
# Makefile — Homelab Rust Binaries
# =============================================================================

APPS_DIR := apps
CLIENT_NAME := CLIENT
HOST_NAME := HOST
LXC_NAME := LXC
CLIENT_SRC := client-app
HOST_SRC := host-daemon
LXC_SRC := lxc-daemon


.PHONY: build build-client build-client-windows build-host build-lxc clean

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

clean:
	cargo clean --manifest-path $(CLIENT_SRC)/Cargo.toml
	cargo clean --manifest-path $(HOST_SRC)/Cargo.toml
	cargo clean --manifest-path $(LXC_SRC)/Cargo.toml
	@rm -f $(APPS_DIR)/$(CLIENT_NAME) $(APPS_DIR)/$(HOST_NAME) $(APPS_DIR)/$(LXC_NAME)
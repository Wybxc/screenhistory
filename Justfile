# Variables

BIN_NAME := "screenhistory"
CLI_PKG := "screenhistory-cli"
BIN_DIR := "/usr/local/bin"
TARGET_BIN := "target/release/screenhistory"

# Build the release binary for the CLI crate
release:
    cargo build -p {{ CLI_PKG }} --release

# Install the release binary to /usr/local/bin (uses sudo if needed)
install: release
    mkdir -p {{ BIN_DIR }}
    if [ -w "{{ BIN_DIR }}" ]; then \
      install -m 0755 {{ TARGET_BIN }} {{ BIN_DIR }}/{{ BIN_NAME }}; \
    else \
      sudo install -m 0755 {{ TARGET_BIN }} {{ BIN_DIR }}/{{ BIN_NAME }}; \
    fi

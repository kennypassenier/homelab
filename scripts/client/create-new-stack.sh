#!/usr/bin/env bash
# Script Name: create-new-stack.sh
# Description: Generates a new application template with required files and labels.

set -euo pipefail

USE_DOCKER=""
USE_PROMTAIL=""
USE_WATCHTOWER=""

function show_help() {
    echo "Usage: $0 [OPTIONS] [STACK_NAME]"
    echo "Options:"
    echo "  -d    Force use Docker without prompting"
    echo "  -w    Include centralized Watchtower (requires Docker)"
    echo "  -p    Include centralized Promtail for Loki (requires Docker)"
    echo "  -h    Show this help message"
}

while getopts "dwph" opt; do
    case ${opt} in
        d ) USE_DOCKER="y" ;;
        w ) USE_WATCHTOWER="y" ;;
        p ) USE_PROMTAIL="y" ;;
        h ) show_help; exit 0 ;;
        \? ) show_help; exit 1 ;;
    esac
done
shift $((OPTIND -1))

STACK_NAME="${1:-}"

if [[ -z "$STACK_NAME" ]]; then
    read -r -p "Enter the name of the new stack (LXC container): " STACK_NAME
fi

if [[ -z "$STACK_NAME" ]]; then
    echo "Error: Stack name cannot be empty."
    exit 1
fi

# Ensure we are running from the root of the repo
if [[ ! -d "apps" ]]; then
    echo "Error: Run this script from the root of the repository."
    exit 1
fi

if [[ -z "$USE_DOCKER" ]]; then
    read -r -p "Will this stack use Docker? (y/n) [y]: " USE_DOCKER
    USE_DOCKER=${USE_DOCKER:-y}
fi

if [[ "$USE_DOCKER" =~ ^[Yy]$ ]]; then
    if [[ -z "$USE_WATCHTOWER" ]]; then
        read -r -p "Include Watchtower for automatic updates? (y/n) [y]: " USE_WATCHTOWER
        USE_WATCHTOWER=${USE_WATCHTOWER:-y}
    fi
    if [[ -z "$USE_PROMTAIL" ]]; then
        read -r -p "Include Promtail for centralized logging to Loki? (y/n) [n]: " USE_PROMTAIL
        USE_PROMTAIL=${USE_PROMTAIL:-n}
    fi
else
    USE_WATCHTOWER="n"
    USE_PROMTAIL="n"
fi

echo "Creating infrastructure template for stack ${STACK_NAME}..."

APPS=()

while true; do
    echo ""
    read -r -p "Enter app name for this stack (leave empty to finish): " APP_NAME
    if [[ -z "$APP_NAME" ]]; then
        break
    fi

    APP_DIR="apps/${STACK_NAME}/${APP_NAME}"
    mkdir -p "${APP_DIR}"
    APPS+=("${APP_NAME}")

    if [[ "$USE_DOCKER" =~ ^[Yy]$ ]]; then
        # Create a baseline docker-compose.yml with automated Watchtower and Restic labels
        cat <<EOF > "${APP_DIR}/docker-compose.yml"
services:
  ${APP_NAME}:
    image: lscr.io/linuxserver/${APP_NAME}:latest
    container_name: ${APP_NAME}
    env_file:
      - .env
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=Europe/Brussels
    volumes:
      # Automatically refers to the fast NVMe host storage isolated per stack
      - /appdata/${APP_NAME}/config:/config
    labels:
      # Enable automatic software updates via Watchtower
      - "com.centurylinklabs.watchtower.enable=true"
      # Pause container during Restic backups to prevent database corruption
      - "com.homelab.backup.pause=true"
    ports:
      - "8080:80"
    restart: unless-stopped
EOF

        # Create a plaintext .env file.
        echo "SECRET_EXAMPLE_TOKEN=vervang_dit_met_iets_geheims" > "${APP_DIR}/.env"
        echo "Docker template generated successfully in ${APP_DIR}."
    else
        echo "Directory created successfully in ${APP_DIR}."
    fi
done

# Generate central Watchtower for the stack if requested and Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [[ "$USE_WATCHTOWER" =~ ^[Yy]$ ]] && [ ${#APPS[@]} -gt 0 ]; then
    WT_DIR="apps/${STACK_NAME}/watchtower"
    mkdir -p "${WT_DIR}"
    cat <<EOF > "${WT_DIR}/docker-compose.yml"
services:
  watchtower:
    image: containrrr/watchtower
    container_name: watchtower-${STACK_NAME}
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    command: --cleanup --label-enable
    restart: unless-stopped
EOF
    echo "Central Watchtower configured in ${WT_DIR}."
fi

# Generate central Promtail for the stack if requested and Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [[ "$USE_PROMTAIL" =~ ^[Yy]$ ]]; then
    PROM_DIR="apps/${STACK_NAME}/promtail"
    mkdir -p "${PROM_DIR}"
    cat <<EOF > "${PROM_DIR}/docker-compose.yml"
services:
  promtail:
    image: grafana/promtail:latest
    container_name: promtail-${STACK_NAME}
    volumes:
      - /var/log:/var/log:ro
      - /var/lib/docker/containers:/var/lib/docker/containers:ro
      - ./config.yml:/etc/promtail/config.yml:ro
    command: -config.file=/etc/promtail/config.yml
    restart: unless-stopped
EOF

    cat <<EOF > "${PROM_DIR}/config.yml"
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://LOKI_IP:3100/loki/api/v1/push

scrape_configs:
  - job_name: system
    static_configs:
    - targets:
        - localhost
      labels:
        job: varlogs
        host: ${STACK_NAME}
        __path__: /var/log/*log

  - job_name: docker
    pipeline_stages:
      - docker: {}
    static_configs:
      - targets:
          - localhost
        labels:
          job: docker
          host: ${STACK_NAME}
          __path__: /var/lib/docker/containers/*/*log
EOF

    echo "Central Promtail configured in ${PROM_DIR}. (Remember to update LOKI_IP in config.yml)"
fi

echo ""
echo "Stack generation completed."
echo "You can now edit the docker-compose.yml and .env files directly."
echo "When you run 'git add', Git and SOPS will invisibly encrypt the .env files for you."

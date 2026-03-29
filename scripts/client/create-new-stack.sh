#!/usr/bin/env bash
# Script Name: create-new-stack.sh
# Description: Generates a new application template with required files and labels.

set -euo pipefail

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

read -r -p "Will this stack use Docker? (y/n) [y]: " USE_DOCKER
USE_DOCKER=${USE_DOCKER:-y}

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
      # Store config locally on the fast SSD, not on the 2TB backup/media drive
      - ./config:/config
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

# Generate central Watchtower for the stack if Docker is used
if [[ "$USE_DOCKER" =~ ^[Yy]$ ]] && [ ${#APPS[@]} -gt 0 ]; then
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

echo ""
echo "Stack generation completed."
echo "You can now edit the docker-compose.yml and .env files directly."
echo "When you run 'git add', Git and SOPS will invisibly encrypt the .env files for you."

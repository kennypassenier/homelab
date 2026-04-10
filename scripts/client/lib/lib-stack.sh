#!/usr/bin/env bash
# Script Name: lib-stack.sh
# Description: Shared library for stack and app generation.

# Ensure we are running from the root of the repo
require_repo_root() {
    if [[ ! -d "stacks" || ! -d "scripts" ]]; then
        echo "Error: Run this script from the root of the repository."
        exit 1
    fi
}

# Prompts the user to select an existing stack from the stacks directory
# Returns the selected stack name
prompt_stack_selection() {
    local stacks=()
    for dir in stacks/*/; do
        if [[ -d "$dir" ]]; then
            stacks+=("$(basename "$dir")")
        fi
    done

    if [[ ${#stacks[@]} -eq 0 ]]; then
        echo "No existing stacks found in stacks/." >&2
        return 1
    fi

    echo "Available stacks:" >&2
    for i in "${!stacks[@]}"; do
        echo "$((i+1)). ${stacks[$i]}" >&2
    done

    local choice
    while true; do
        read -r -p "Select a stack (1-${#stacks[@]}): " choice >&2
        if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#stacks[@]}" ]; then
            # Output the selected stack name to stdout
            echo "${stacks[$((choice-1))]}"
            return 0
        else
            echo "Invalid selection. Please enter a number between 1 and ${#stacks[@]}." >&2
        fi
    done
}

# Prompts the user to select an app from a specific stack
# Argument $1: stack_name
# Returns the selected app name
prompt_app_selection() {
    local stack_name="$1"
    if [[ -z "$stack_name" ]]; then
        echo "Error: prompt_app_selection requires a stack name." >&2
        return 1
    fi

    local stacks=()
    for dir in "stacks/${stack_name}"/*/; do
        if [[ -d "$dir" ]]; then
            stacks+=("$(basename "$dir")")
        fi
    done

    if [[ ${#stacks[@]} -eq 0 ]]; then
        echo "No existing stacks found in stacks/${stack_name}/." >&2
        return 1
    fi

    echo "Available stacks in ${stack_name}:" >&2
    for i in "${!stacks[@]}"; do
        echo "$((i+1)). ${stacks[$i]}" >&2
    done

    local choice
    while true; do
        read -r -p "Select an app (1-${#stacks[@]}): " choice >&2
        if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#stacks[@]}" ]; then
            echo "${stacks[$((choice-1))]}"
            return 0
        else
            echo "Invalid selection. Please enter a number between 1 and ${#stacks[@]}." >&2
        fi
    done
}

# Generates a new application template inside a stack
generate_app() {
    local stack_name="$1"
    local app_name="$2"
    local use_docker="${3:-y}"

    local app_dir="stacks/${stack_name}/${app_name}"
    mkdir -p "${app_dir}"

    if [[ "$use_docker" =~ ^[Yy]$ ]]; then
        # Create a baseline docker-compose.yml with automated Watchtower and Restic labels
        cat <<EOF > "${app_dir}/docker-compose.yml"
services:
  ${app_name}:
    image: lscr.io/linuxserver/${app_name}:latest
    container_name: ${app_name}
    env_file:
      - .env
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=Europe/Brussels
    volumes:
      # Automatically refers to the fast NVMe host storage isolated per stack
      - /appdata/${stack_name}/${app_name}/config:/config
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
        echo "SECRET_EXAMPLE_TOKEN=vervang_dit_met_iets_geheims" > "${app_dir}/.env"
        echo "Docker template generated successfully in ${app_dir}."
    else
        echo "Directory created successfully in ${app_dir}."
    fi
}

# Generates a central Watchtower configuration for a stack
generate_watchtower() {
    local stack_name="$1"
    local wt_dir="stacks/${stack_name}/watchtower"

    mkdir -p "${wt_dir}"
    cat <<EOF > "${wt_dir}/docker-compose.yml"
services:
  watchtower:
    image: containrrr/watchtower:latest
    container_name: watchtower-${stack_name}
    environment:
      - DOCKER_API_VERSION=1.41
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
    command: --cleanup --label-enable
    restart: unless-stopped
    labels:
      # Ensure Watchtower updates itself
      - "com.centurylinklabs.watchtower.enable=true"
EOF
    echo "Central Watchtower configured in ${wt_dir}."
}

# Generates a central Promtail configuration for a stack
generate_promtail() {
    local stack_name="$1"
    local prom_dir="stacks/${stack_name}/promtail"

    mkdir -p "${prom_dir}"
    cat <<EOF > "${prom_dir}/docker-compose.yml"
services:
  promtail:
    image: grafana/promtail:latest
    container_name: promtail-${stack_name}
    volumes:
      - /var/log:/var/log:ro
      - /var/lib/docker/containers:/var/lib/docker/containers:ro
      - ./config.yml:/etc/promtail/config.yml:ro
    command: -config.file=/etc/promtail/config.yml -config.expand-env=true
    env_file:
      - .env
    restart: unless-stopped
EOF

    echo "LOKI_IP=10.10.10.7" > "${prom_dir}/.env"

    cat <<EOF > "${prom_dir}/config.yml"
server:
  http_listen_port: 9080
  grpc_listen_port: 0

positions:
  filename: /tmp/positions.yaml

clients:
  - url: http://\${LOKI_IP}:3100/loki/api/v1/push

scrape_configs:
  - job_name: system
    static_configs:
    - targets:
        - localhost
      labels:
        job: varlogs
        host: ${stack_name}
        __path__: /var/log/*log

  - job_name: docker
    pipeline_stages:
      - docker: {}
    static_configs:
      - targets:
          - localhost
        labels:
          job: docker
          host: ${stack_name}
          __path__: /var/lib/docker/containers/*/*log
EOF

    echo "Central Promtail configured in ${prom_dir}. (Remember to update LOKI_IP in config.yml)"
}

#!/usr/bin/env bash
# Script Name: lib-stack.sh
# Description: Shared library for stack and app generation.

# Source the shared UI library so color variables are always available,
# even when this library is sourced before lib-ui.sh in the calling script.
_LIB_STACK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${_LIB_STACK_DIR}/../../shared/lib-ui.sh"

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

    # ui_choose handles gum (interactive picker) and fallback (numbered list).
    # "Cancel" as the last item is colored yellow in fallback mode.
    local result
    result=$(ui_choose --header "Select a stack:" "${stacks[@]}" "Cancel") || return 2
    [[ "$result" == "Cancel" ]] && return 2
    echo "$result"
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

    # ui_choose handles gum (interactive picker) and fallback (numbered list).
    # "Cancel" as the last item is colored yellow in fallback mode.
    local result
    result=$(ui_choose --header "Select an app in '${stack_name}':" "${stacks[@]}" "Cancel") || return 2
    [[ "$result" == "Cancel" ]] && return 2
    echo "$result"
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
        echo "SECRET_EXAMPLE_TOKEN=replace_with_your_actual_secret" > "${app_dir}/.env"
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
    local app_dir="stacks/${stack_name}/${app_name}"
    mkdir -p "${app_dir}"

    # --- Ensure pre-sync.sh will always create /appdata/<stack>/<app> dirs ---
    local pre_sync="stacks/${stack_name}/pre-sync.sh"
    local app_data_dir="/appdata/${stack_name}/${app_name}"
    if ! grep -q "mkdir -p ${app_data_dir}" "$pre_sync" 2>/dev/null; then
        # Insert mkdir -p before any infisical export or .env generation for this app
        if grep -q "infisical export --env=prod --path=${stack_name}/${app_name}/.env" "$pre_sync" 2>/dev/null; then
            sed -i "/infisical export --env=prod --path=${stack_name}\/${app_name}\/\.env/i mkdir -p ${app_data_dir}" "$pre_sync" 2>/dev/null
        else
            echo "mkdir -p ${app_data_dir}" >> "$pre_sync"
        fi
    fi

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
        echo "SECRET_EXAMPLE_TOKEN=replace_with_your_actual_secret" > "${app_dir}/.env"
        echo "Docker template generated successfully in ${app_dir}."
    else
        echo "Directory created successfully in ${app_dir}."
    fi
      - docker: {}
    static_configs:
      - targets:
          - localhost
        labels:
          job: docker
          host: ${stack_name}
          __path__: /var/lib/docker/containers/*/*log

  - job_name: node_sync
    static_configs:
      - targets:
          - localhost
        labels:
          job: node_sync
          host: ${stack_name}
          __path__: /var/log/node-sync.log
    pipeline_stages:
      - logfmt:
          mapping:
            ts:
            level:
            stack:
            app:
      - labels:
          level:
          stack:
          app:
      - timestamp:
          source: ts
          format: RFC3339
EOF

    echo "Central Promtail configured in ${prom_dir}. (Remember to update LOKI_IP in config.yml)"
}

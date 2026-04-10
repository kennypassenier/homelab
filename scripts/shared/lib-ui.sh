#!/usr/bin/env bash
# Script Name: lib-ui.sh
# Description: Shared UI library providing colors, icons, and pacman spinners for bash scripts.

# Determine if we are running in an interactive terminal to support colors safely
if [ -t 1 ]; then
    C_RED='\033[1;31m'
    C_GREEN='\033[1;32m'
    C_YELLOW='\033[1;33m'
    C_BLUE='\033[1;34m'
    C_CYAN='\033[1;36m'
    C_NC='\033[0m' # No Color
else
    C_RED=''
    C_GREEN=''
    C_YELLOW=''
    C_BLUE=''
    C_CYAN=''
    C_NC=''
fi

# --- Logging Functions ---

ui_info() {
    echo -e "${C_BLUE}ℹ️  INFO:${C_NC} $1"
}

ui_success() {
    echo -e "${C_GREEN}✅ SUCCESS:${C_NC} $1"
}

ui_warning() {
    echo -e "${C_YELLOW}⚠️  WARNING:${C_NC} $1"
}

ui_error() {
    echo -e "${C_RED}❌ ERROR:${C_NC} $1"
}

ui_step() {
    echo -e "${C_CYAN}➡️  ${C_NC}$1"
}

# --- Pacman Spinner ---

# Core pacman animation loop
# Usage: ui_pacman <pid> <message>
ui_pacman() {
    local pid=$1
    local message=$2
    local delay=0.15

    # Define the frames for Pacman eating the dots
    local frames=(
        "${C_YELLOW}C ${C_NC}• • •"
        " ${C_YELLOW}c ${C_NC}• • •"
        "  ${C_YELLOW}C ${C_NC}• •"
        "   ${C_YELLOW}c ${C_NC}• •"
        "    ${C_YELLOW}C ${C_NC}•"
        "     ${C_YELLOW}c ${C_NC}•"
        "      ${C_YELLOW}C ${C_NC}"
        "       ${C_YELLOW}c ${C_NC}"
    )

    tput civis # Hide cursor

    # Loop the animation as long as the background process (PID) is running
    while kill -0 "$pid" 2>/dev/null; do
        for frame in "${frames[@]}"; do
            # Break early if the process finishes mid-animation
            if ! kill -0 "$pid" 2>/dev/null; then
                break 2
            fi

            # \r returns to the start of the line
            # \033[K clears the line from the cursor to the end
            printf "\r\033[K%s  %s" "$frame" "$message"
            sleep $delay
        done
    done

    tput cnorm # Restore cursor
    printf "\r\033[K" # Clear the spinner line before moving on
}

# Wrapper to run any command with a pacman loading screen
# Usage: ui_run_pacman "Doing something cool..." command arg1 arg2
ui_run_pacman() {
    local message="$1"
    shift

    # Run the provided command in the background (suppressing its output so it doesn't break the UI)
    "$@" > /dev/null 2>&1 &
    local pid=$!

    # Start the pacman spinner attached to the background process
    ui_pacman "$pid" "$message"

    # Wait for the process to actually finish and capture its exit code
    wait "$pid"
    local status=$?

    if [ $status -eq 0 ]; then
        ui_success "$message"
    else
        ui_error "$message (Failed)"
    fi

    return $status
}

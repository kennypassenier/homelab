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

# Left margin applied to all output for visual consistency across any terminal width
UI_INDENT="  "

# Detect if gum (https://github.com/charmbracelet/gum) is available.
# Gum is only activated for interactive terminals — cron jobs and piped output
# fall back automatically to the plain POSIX implementations below.
if command -v gum &>/dev/null && [ -t 1 ]; then
    GUM_AVAILABLE="true"
else
    GUM_AVAILABLE="false"
fi

# --- Utility Functions ---

# Strips leading and trailing whitespace from a string.
# Usage: trimmed=$(ui_trim "  my input  ")
ui_trim() {
    local s="$1"
    s="${s#"${s%%[![:space:]]*}"}"
    s="${s%"${s##*[![:space:]]}"}"   
    echo "$s"
}

# --- Interactive TUI Functions ---
# When gum is installed, these wrappers use its rich components.
# When gum is absent, they fall back to standard read/printf/tput equivalents.
# All interactive prompts route through these functions — never call gum directly.

# Presents an interactive selection menu and returns the chosen item text via stdout.
# Items named "Exit" or "Cancel" are automatically colored yellow in fallback mode.
# Escape in gum (exit code 1) is propagated — callers should use: CHOICE=$(ui_choose ...) || ...
# Usage: CHOICE=$(ui_choose [--header "Prompt"] "Item 1" "Item 2" "Exit")
ui_choose() {
    local header=""
    if [[ "${1:-}" == "--header" ]]; then header="$2"; shift 2; fi
    local items=("$@")

    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        local args=(--padding "0 0 0 2")
        [[ -n "$header" ]] && args+=(--header "${UI_INDENT}${header}")
        gum choose "${args[@]}" "${items[@]}" || return 1
    else
        [[ -n "$header" ]] && echo -e "${UI_INDENT}${C_CYAN}${header}${C_NC}" >&2
        for i in "${!items[@]}"; do
            local item="${items[$i]}"
            local color="$C_GREEN"
            [[ "$item" == "Cancel" || "$item" == "Exit" ]] && color="$C_YELLOW"
            echo -e "  ${color}$((i+1)).${C_NC} ${item}" >&2
        done
        local choice
        while true; do
            read -r -p "${UI_INDENT}Select (1-${#items[@]}): " choice >&2
            if [[ "$choice" =~ ^[0-9]+$ ]] && (( choice >= 1 && choice <= ${#items[@]} )); then
                echo "${items[$((choice-1))]}"
                return 0
            fi
            echo -e "${UI_INDENT}${C_RED}Invalid selection.${C_NC}" >&2
        done
    fi
}

# Presents a multi-select menu and prints each chosen item on its own line via stdout.
# With gum: interactive checkbox picker (Space to toggle, Enter to confirm).
# Without gum: numbered list, user types space-separated numbers; empty input = nothing selected.
# Returns exit code 1 if the user cancels with Esc (gum only); callers should handle via || { ... exit 0; }.
# Usage: mapfile -t SELECTED < <(ui_multiselect [--header "Prompt"] [--selected "Item 1,Item 2"] "Item 1" "Item 2" ...)
ui_multiselect() {
    local header="" selected_csv=""
    while [[ "${1:-}" == "--header" || "${1:-}" == "--selected" ]]; do
        if [[ "$1" == "--header" ]];   then header="$2";       shift 2
        elif [[ "$1" == "--selected" ]]; then selected_csv="$2"; shift 2
        fi
    done
    local items=("$@")

    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        local args=(--no-limit --padding "0 0 0 2")
        [[ -n "$header" ]]       && args+=(--header "${UI_INDENT}${header}")
        [[ -n "$selected_csv" ]] && args+=(--selected "$selected_csv")
        gum choose "${args[@]}" "${items[@]}" || return 1
    else
        [[ -n "$header" ]] && echo -e "${UI_INDENT}${C_CYAN}${header}${C_NC}" >&2
        echo -e "${UI_INDENT}${C_CYAN}Enter space-separated numbers, or press Enter for none.${C_NC}" >&2
        for i in "${!items[@]}"; do
            echo -e "  ${C_GREEN}$((i+1)).${C_NC} ${items[$i]}" >&2
        done
        local input
        while true; do
            read -r -p "${UI_INDENT}Select (1-${#items[@]}, space-separated, Enter for none): " input >&2
            if [[ -z "$input" ]]; then
                return 0  # Empty = nothing selected — valid
            fi
            local valid=true
            local -a seen=()
            local -a selected=()
            for num in $input; do
                if [[ "$num" =~ ^[0-9]+$ ]] && (( num >= 1 && num <= ${#items[@]} )); then
                    # Deduplicate
                    local already=false
                    for s in "${seen[@]:-}"; do [[ "$s" == "$num" ]] && already=true; done
                    if ! $already; then
                        seen+=("$num")
                        selected+=("${items[$((num-1))]}")
                    fi
                else
                    valid=false; break
                fi
            done
            if $valid; then
                printf '%s\n' "${selected[@]}"
                return 0
            fi
            echo -e "${UI_INDENT}${C_RED}Invalid. Use numbers between 1 and ${#items[@]}.${C_NC}" >&2
        done
    fi
}

# Renders a styled single-line text input and returns the entered value via stdout.
# The placeholder is shown as greyed hint text in gum; not shown in fallback mode.
# If a default is given, pressing Enter without typing keeps it (gum pre-fills the field).
# Returns exit code 1 if the user cancels with Esc (gum only).
# Usage: VALUE=$(ui_input "Prompt" [placeholder] [default])
ui_input() {
    local prompt="$1"
    local placeholder="${2:-}"
    local default="${3:-}"

    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        echo -e "${UI_INDENT}${C_CYAN}${prompt}${C_NC}" >&2
        local args=(--prompt "${UI_INDENT}> " --placeholder "$placeholder" --padding "0 0 0 2")
        [[ -n "$default" ]] && args+=(--value "$default")
        gum input "${args[@]}" || return 1
    else
        local value
        if [[ -n "$default" ]]; then
            read -r -p "${UI_INDENT}${prompt} [${default}]: " value
            echo "${value:-$default}"
        else
            read -r -p "${UI_INDENT}${prompt}: " value
            echo "$value"
        fi
    fi
}

# Like ui_input, but loops until the user provides a non-empty value.
# Gum enforces this via --char-limit 0 check; fallback loops with an error message.
# Returns exit code 1 if the user cancels with Esc (gum only).
# Usage: VALUE=$(ui_input_required "Prompt" [placeholder] [default])
ui_input_required() {
    local prompt="$1"
    local placeholder="${2:-}"
    local default="${3:-}"

    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        local value
        while true; do
            value=$(ui_input "$prompt" "$placeholder" "$default") || return 1
            if [[ -n "$value" ]]; then
                echo "$value"
                return 0
            fi
            ui_error "This field cannot be empty."
        done
    else
        local value
        while true; do
            if [[ -n "$default" ]]; then
                read -r -p "${UI_INDENT}${prompt} [${default}]: " value
                value="${value:-$default}"
            else
                read -r -p "${UI_INDENT}${prompt}: " value
            fi
            if [[ -n "$value" ]]; then
                echo "$value"
                return 0
            fi
            ui_error "This field cannot be empty."
        done
    fi
}

# Renders a Yes/No confirmation prompt. Returns 0 (yes) or 1 (no/cancelled).
# Second arg "true" makes Yes the default — use for non-destructive prompts.
# Omit or pass "false" to default to No — always use this for destructive actions.
# Usage: ui_confirm "Question?" [true]
ui_confirm() {
    local prompt="$1"
    local default_yes="${2:-false}"

    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        gum confirm "${prompt}" \
            --affirmative "Yes" --negative "No" \
            --default="${default_yes}" || return 1
    else
        local default_hint answer defval
        if [[ "$default_yes" == "true" ]]; then
            default_hint="(Y/n)"; defval="y"
        else
            default_hint="(y/N)"; defval="n"
        fi
        read -r -p "${UI_INDENT}${prompt} ${default_hint}: " answer
        answer="${answer:-${defval}}"
        [[ "$answer" =~ ^[Yy]$ ]]
    fi
}

# Runs a command with a loading spinner. Prefer this over ui_run_pacman for new code.
# With gum: shows a dot spinner with a title. Without: uses the pacman animation.
# Usage: ui_spin "Loading message..." command args...
ui_spin() {
    local message="$1"
    shift
    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        gum spin --spinner dot --title "${UI_INDENT}${message}" -- "$@"
        local status=$?
        if [ $status -eq 0 ]; then ui_success "$message"; else ui_error "$message (Failed)"; fi
        return $status
    else
        ui_run_pacman "$message" "$@"
    fi
}

# Prints a full-width horizontal divider line, scaling to the current terminal width.
# Usage: ui_divider [color_var]   (defaults to C_CYAN)
ui_divider() {
    local color="${1:-$C_CYAN}"
    local width
    width=$(tput cols 2>/dev/null || echo 64)
    local line
    line=$(printf '%*s' "$width" '' | tr ' ' '─')
    echo -e "${color}${line}${C_NC}"
}

# Prints a full-width header block. Uses a styled gum box when available,
# otherwise falls back to: divider + indented title + divider.
# Scales to the current terminal width in both modes.
# Usage: ui_header "My Title" [color_var]
ui_header() {
    local title="$1"
    local color="${2:-$C_CYAN}"
    echo ""
    if [[ "$GUM_AVAILABLE" == "true" ]]; then
        local width
        width=$(( $(tput cols 2>/dev/null || echo 80) - 4 ))
        gum style \
            --foreground "14" --border-foreground "14" \
            --border rounded --align center \
            --width "$width" --padding "0 2" \
            "$title"
    else
        ui_divider "$color"
        echo -e "${color}${UI_INDENT}${title}${C_NC}"
        ui_divider "$color"
    fi
    echo ""
}

# Prints a section heading underlined with dashes matching the title length exactly.
# Width-independent: looks correct at any terminal size.
# Usage: ui_section "My Section"
ui_section() {
    local title="$1"
    local len=${#title}
    local underline
    underline=$(printf '─%.0s' $(seq 1 "$len"))
    echo ""
    echo -e "${UI_INDENT}${C_CYAN}${title}${C_NC}"
    echo -e "${UI_INDENT}${C_CYAN}${underline}${C_NC}"
    echo ""
}

# --- Logging Functions ---

ui_info() {
    echo -e "${UI_INDENT}${C_BLUE}ℹ️  INFO:${C_NC} $1"
}

ui_success() {
    echo -e "${UI_INDENT}${C_GREEN}✅ SUCCESS:${C_NC} $1"
}

ui_warning() {
    echo -e "${UI_INDENT}${C_YELLOW}⚠️  WARNING:${C_NC} $1"
}

ui_error() {
    echo -e "${UI_INDENT}${C_RED}❌ ERROR:${C_NC} $1"
}

ui_step() {
    echo -e "${UI_INDENT}${C_CYAN}➡️  ${C_NC}$1"
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

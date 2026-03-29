#!/bin/sh
# Watchtower pre-update script to prevent restarting Jellyfin during active streams.

# Ensure variables are set. JELLYFIN_API_KEY should be passed via docker-compose environment.
JELLYFIN_URL="http://localhost:8096"
API_KEY="${JELLYFIN_API_KEY}"

if [ -z "$API_KEY" ]; then
    echo "JELLYFIN_API_KEY environment variable is missing."
    echo "Please set it to a valid Jellyfin API key."
    echo "Allowing update as a fallback..."
    exit 0
fi

echo "Checking Jellyfin for active sessions..."

# Fetch current sessions from Jellyfin
SESSIONS=$(curl -s -X GET "${JELLYFIN_URL}/Sessions" -H "Authorization: MediaBrowser Token=${API_KEY}")

if [ -z "$SESSIONS" ]; then
    echo "Could not reach Jellyfin API or empty response. Allowing update."
    exit 0
fi

# Check if any session is actively playing media
if echo "$SESSIONS" | grep -qE '"IsPlaying"\s*:\s*true'; then
    echo "Active stream detected! Aborting Watchtower update."
    # Exit 1 tells Watchtower to abort the update process
    exit 1
else
    echo "No active streams detected. Safe to update."
    # Exit 0 tells Watchtower it is safe to proceed
    exit 0
fi

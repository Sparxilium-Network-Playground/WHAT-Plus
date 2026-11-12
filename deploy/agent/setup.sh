#!/usr/bin/env bash
# WHAT Plus agent setup (Linux). Run in this directory as the user whose logs should be read:
#   ./setup.sh            (use sudo only if your user cannot run docker)
set -euo pipefail
cd "$(dirname "$0")"

ok()   { printf 'OK  %s\n' "$*"; }
fail() { printf 'ERROR  %s\n' "$*" >&2; exit 1; }
command -v docker >/dev/null || fail "Docker is not installed."

# The user whose ~/.claude etc. are read (the invoking user when run with sudo).
owner=${SUDO_USER:-$(id -un)}
home=$(getent passwd "$owner" | cut -d: -f6)
uid=$(id -u "$owner")
gid=$(id -g "$owner")

pick() {
    # $1 = folder name under home; falls back to an empty placeholder when missing.
    if [ -d "$home/$1" ]; then echo "$home/$1"; else mkdir -p "empty/$1"; echo "./empty/$1"; fi
}

mkdir -p data keys
cat > .env <<EOF
CLAUDE_DIR=$(pick .claude)
CODEX_DIR=$(pick .codex)
GEMINI_DIR=$(pick .gemini)
AGENT_UID=$uid
AGENT_GID=$gid
EOF
chown -R "$uid:$gid" data keys
chmod 700 data keys
ok "Wrote .env:"
sed 's/^/    /' .env

read -r -p "Build and start the agent now? [Y/n]: " go
if [ "${go:-Y}" != "n" ]; then
    docker compose up -d --build
    ok "Agent started. It waits until it is configured."
fi
cat <<'EOT'

Next steps:
  1. Open the agent TUI:   docker compose exec agent whatplus-agent admin
       - Server address: the server's LAN address
       - Private key: generate a new key (stored in keys/)
  2. Register the shown public key as an agent key (client app > Machines).
  3. Back in the TUI: "Verify server and test login" and compare the fingerprint.
EOT

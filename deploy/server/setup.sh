#!/usr/bin/env bash
# WHAT Plus server setup (Linux). Run in this directory:  sudo ./setup.sh
#   sudo ./setup.sh upgrade   keep all settings, fix permissions, rebuild and restart
set -euo pipefail
cd "$(dirname "$0")"

CONFIG_VERSION=1
UID_GID=10001:10001
CONF=config/config.conf

say()  { printf '\n== %s\n' "$*"; }
ok()   { printf 'OK  %s\n' "$*"; }
fail() { printf 'ERROR  %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" -eq 0 ] || fail "Run with sudo: sudo ./setup.sh"
command -v docker >/dev/null || fail "Docker is not installed."

prepare_dirs() {
    # docker compose creates a missing bind-mount source as an empty directory.
    if [ -d "$CONF" ]; then
        if [ -z "$(ls -A "$CONF")" ]; then rmdir "$CONF"; else fail "$CONF is a directory; remove it and run again."; fi
    fi
    mkdir -p data keys secrets
}

write_config() {
    if [ ! -f "$CONF" ]; then
        cp config/config.conf.example "$CONF"
        read -r -p "Time zone for reports [Asia/Taipei]: " tz
        tz=${tz:-Asia/Taipei}
        sed -i "s|^timezone=.*|timezone=$tz|" "$CONF"
        ok "Created $CONF"
    fi
    local v
    v=$(sed -n 's/^config.version=//p' "$CONF")
    if [ "${v:-1}" -gt "$CONFIG_VERSION" ]; then
        fail "$CONF was written by a newer release (version $v)."
    fi
}

ask_token() {
    say "Discord bot token"
    echo "Discord Developer Portal > your application > Bot > Reset Token / Copy."
    if [ -s secrets/discord-token ]; then
        read -r -p "A token is already saved. Press Enter to keep it, or type 'n' to replace: " keep
        [ "${keep:-}" != "n" ] && return
    fi
    read -r -s -p "Paste the token (input is hidden): " token; echo
    [ -n "$token" ] || fail "Empty token."
    printf '%s' "$token" > secrets/discord-token
    unset token
    ok "Token saved to secrets/discord-token"
}

fix_permissions() {
    chown -R "$UID_GID" data secrets
    chmod 700 data
    chmod 500 secrets
    [ -f secrets/discord-token ] && chmod 400 secrets/discord-token
    chmod 755 keys
    find keys -type f -exec chmod 644 {} + 2>/dev/null || true
    chmod 644 "$CONF"
    ok "Permissions fixed"
}

start() {
    say "Building and starting the service (the first build takes several minutes)"
    docker compose up -d --build
    for _ in $(seq 1 30); do
        fp=$(docker compose logs server 2>/dev/null | sed -n 's/.*host key fingerprint: \(SHA256:[^ ]*\).*/\1/p' | tail -1)
        [ -n "$fp" ] && break
        sleep 2
    done
    [ -n "${fp:-}" ] || fail "The service did not start; check: docker compose logs --tail=40 server"
    ok "The service is running. Host key fingerprint: $fp"
}

next_steps() {
    cat <<'EOT'

Next steps:
  1. Copy the client's admin public key (and agents' keys) into keys/, e.g.
       scp ~/.ssh/id_ed25519.pub user@this-server:~/WHAT-Plus/deploy/server/keys/admin.pub
  2. Open the admin TUI:   docker compose exec server whatplus-server admin
       - Network allowlist: add your LAN, e.g. 192.168.0.0/16
       - Import keys: select the admin key and give it the admin role
  3. Connect with the Windows client and compare the fingerprint shown above.
EOT
}

prepare_dirs
if [ "${1:-}" = "upgrade" ]; then
    [ -f "$CONF" ] || fail "Not installed yet; run ./setup.sh without arguments."
    write_config
    fix_permissions
    start
    exit 0
fi
write_config
ask_token
fix_permissions
read -r -p "Start the service now? [Y/n]: " go
if [ "${go:-Y}" != "n" ]; then start; fi
next_steps

#!/usr/bin/env bash
# Interactive setup for the WHAT Plus server (this deploy directory):
#   config/config.conf   created from config.conf.example, upgraded to the current config.version
#   secrets/discord-token
#   the admin key (from keys/*.pub) and the IP allowlist, stored in the database
#   ownership and permissions for data/, secrets/ and keys/ (the container runs as uid 10001)
#   builds and starts the service and shows the server fingerprint
#
# Usage, in this directory:  sudo ./setup.sh
#                            sudo ./setup.sh upgrade   (after git pull: no questions)
# Safe to re-run: press Enter to keep existing values. On an existing installation the first
# question offers "Upgrade": keep every setting, fix permissions and rebuild/restart.
# It does not run git pull.
set -euo pipefail

SERVICE_UID=10001
SCRIPT=$(readlink -f "$0")
cd "$(dirname "$SCRIPT")"

if [ "$(id -u)" -ne 0 ]; then
  echo "setup.sh needs root to set file ownership; re-running with sudo..."
  exec sudo -- bash "$SCRIPT" "$@"
fi
# hidden input turns terminal echo off; turn it back on even after Ctrl+C
if [ -t 0 ]; then
  trap 'stty echo' EXIT
fi

CONFIG=config/config.conf
CONFIG_EXAMPLE=config/config.conf.example
# config.conf format version this script writes; matches config::CONFIG_VERSION in the server
CONFIG_VERSION=1
TOKEN=secrets/discord-token
# home of the user who ran sudo: the admin key is looked up in its .ssh/
USER_HOME=$(getent passwd "${SUDO_USER:-root}" | cut -d: -f6)
MODE=
case ${1:-} in
  "") ;;
  upgrade) MODE=upgrade ;;
  *) echo "Usage: sudo ./setup.sh [upgrade]" >&2; exit 2 ;;
esac

bold() { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32mOK\033[0m  %s\n' "$*"; }
warn() { printf '  \033[33m!!\033[0m  %s\n' "$*"; }
fail() { printf '  \033[31mERROR\033[0m  %s\n' "$*" >&2; exit 1; }

# ask_yes "Question" Y|N  -> exit status 0 for yes
ask_yes() {
  local answer hint="[y/N]"
  [ "$2" = Y ] && hint="[Y/n]"
  read -rp "  $1 $hint " answer
  answer=${answer:-$2}
  [[ $answer =~ ^[Yy] ]]
}

read_hidden() {
  local value
  read -rsp "  $1: " value
  echo >&2
  printf '%s' "$value"
}

# srv ARGS...: runs "whatplus-server setup ARGS" in a one-off container (same database)
srv() {
  docker compose --progress quiet run --rm -T --no-deps server setup "$@"
}

# status_value KEY: one line of "whatplus-server setup status"
status_value() {
  srv status | sed -n "s/^$1=//p"
}

command -v docker >/dev/null || fail "Docker is not installed. See https://docs.docker.com/engine/install/"
docker compose version >/dev/null 2>&1 || fail "The Docker Compose plugin is missing (docker compose)."
[ -f "$CONFIG_EXAMPLE" ] || fail "$CONFIG_EXAMPLE not found. Run this script from deploy/server of the project."
mkdir -p data keys secrets

# ---- config.conf: create it, or upgrade it to CONFIG_VERSION ----------------
# docker compose creates an empty directory for a bind-mounted file that does not exist yet
# (for example when it was started before setup.sh). Only remove it when it is empty.
if [ -d "$CONFIG" ]; then
  [ -z "$(ls -A "$CONFIG")" ] || fail "$CONFIG is a directory with files in it. Move them out, remove the directory and run this script again."
  rmdir "$CONFIG"
  warn "Removed $CONFIG: it was a directory created by docker compose, not a file"
fi
if [ ! -f "$CONFIG" ]; then
  install -m 644 "$CONFIG_EXAMPLE" "$CONFIG"
  ok "Created $CONFIG from $CONFIG_EXAMPLE"
fi
config_version=$(sed -n 's/^config\.version=//p' "$CONFIG" | tr -d '[:space:]')
[[ $config_version =~ ^[0-9]+$ ]] || fail "config.version in $CONFIG is '$config_version', not a number."
if [ "$config_version" -gt "$CONFIG_VERSION" ]; then
  fail "$CONFIG is version $config_version, newer than this setup.sh understands ($CONFIG_VERSION). Run git pull first."
fi
while [ "$config_version" -lt "$CONFIG_VERSION" ]; do
  case $config_version in
    # 1) ... edit settings for version 2 ... ;;
    *) fail "No upgrade step from config.version=$config_version." ;;
  esac
  config_version=$((config_version + 1))
  sed -i "s/^config\.version=.*/config.version=$config_version/" "$CONFIG"
  ok "Upgraded $CONFIG to version $config_version"
done

fix_permissions() {
  chown -R "$SERVICE_UID:$SERVICE_UID" data secrets
  chmod 700 data secrets
  find secrets -type f -exec chmod 400 {} +
  chmod 755 keys
  find keys -type f -exec chmod 644 {} +
  chmod 644 "$CONFIG"
  ok "data/ and secrets/ belong to uid $SERVICE_UID; keys/ is readable"
}

# installed -> exit status 0 when the token is saved and the database exists
installed() {
  [ -s "$TOKEN" ] && [ -f data/whatplus.db ]
}

# ---- Upgrade or setup --------------------------------------------------------
if installed; then
  if [ -z "$MODE" ]; then
    bold "Existing installation found"
    cat <<'EOF'
    1) Upgrade  keep every setting, fix permissions and rebuild/restart the service
                (run git pull first; this script does not)
    2) Setup    review or change the time zone, Discord token, admin keys and allowlist
EOF
    while [ -z "$MODE" ]; do
      read -rp "  Choose [1]: " choice
      case ${choice:-1} in
        1) MODE=upgrade ;;
        2) MODE=setup ;;
        *) warn "Type 1 or 2." ;;
      esac
    done
  fi
elif [ "$MODE" = upgrade ]; then
  fail "Nothing to upgrade: the server is not set up yet. Run sudo ./setup.sh without 'upgrade'."
else
  MODE=setup
fi

if [ "$MODE" = setup ]; then
# ---- 1. Time zone ------------------------------------------------------------
bold "1/6  Time zone"
echo "  Daily / weekly / monthly reports and their periods use this time zone (IANA name)."
current_tz=$(sed -n 's/^timezone=//p' "$CONFIG")
while true; do
  read -rp "  Time zone [${current_tz:-Asia/Taipei}]: " tz
  tz=${tz:-${current_tz:-Asia/Taipei}}
  if [ -e "/usr/share/zoneinfo/$tz" ] || [[ $tz =~ ^[A-Za-z_]+(/[A-Za-z0-9_+-]+)*$ ]]; then
    break
  fi
  warn "Use a name like Asia/Taipei, Europe/London or UTC."
done
sed -i "s|^timezone=.*|timezone=$tz|" "$CONFIG"
ok "timezone=$tz"

# ---- 2. Discord bot token --------------------------------------------------
bold "2/6  Discord bot token"
cat <<'EOF'
  Where to get it: https://discord.com/developers/applications -> your application -> "Bot"
  -> "Reset Token" -> "Copy". It looks like  MTIz...xyz.Gh1abc.AbCd...  (three parts with dots).
  Not the Application ID or Public Key from "General Information".
EOF
if [ -s "$TOKEN" ] && ! ask_yes "A Discord token is already saved. Replace it?" N; then
  ok "Keeping the existing token"
else
  while true; do
    token=$(read_hidden "Bot token (hidden)")
    if [ -z "$token" ]; then
      warn "The token is required."
    elif [[ ! $token =~ ^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$ ]]; then
      warn "That does not look like a bot token (three parts separated by dots)."
      ask_yes "Use it anyway?" N && break
    else
      break
    fi
  done
  (umask 077 && printf '%s' "$token" > "$TOKEN")
  unset token
  ok "Saved $TOKEN"
fi

# ---- 3. Build ----------------------------------------------------------------
bold "3/6  Building the image  (the first build takes several minutes)"
fix_permissions
docker compose build
ok "Image built"
fingerprint=$(status_value fingerprint)
ok "Server fingerprint: $fingerprint"

# ---- 4. Admin key ------------------------------------------------------------
bold "4/6  Admin key  (the Windows client app)"
SSH_DIR=$USER_HOME/.ssh
cat <<EOF
  The client app logs in with an SSH key (ssh-ed25519). This step lists every public key in
      $SSH_DIR/   *.pub and authorized_keys (the keys that may log in to this server)
      $PWD/keys/
  A file with several keys (one per line) shows each key separately; pick the one to use.

  To use a dedicated key instead, create it on the Windows computer that runs the client:
      ssh-keygen -t ed25519 -f \$env:USERPROFILE\\.ssh\\whatplus_admin -C "whatplus-admin"
  and copy whatplus_admin.pub (never the file without .pub) into $PWD/keys/, for example:
      scp \$env:USERPROFILE\\.ssh\\whatplus_admin.pub ${SUDO_USER:-user}@$(hostname -I 2>/dev/null | awk '{print $1}'):$PWD/keys/
EOF

# key_lines: "label<TAB>line" for every line of the public key files (input of "setup check").
# Files are read here on the host; the container only mounts keys/.
key_lines() {
  local f label n line
  for f in keys/*.pub keys/authorized_keys* "$SSH_DIR"/*.pub "$SSH_DIR"/authorized_keys*; do
    [ -f "$f" ] || continue
    case $f in
      keys/*) label="keys/${f##*/}" ;;
      *) label="~/.ssh/${f##*/}" ;;
    esac
    n=0
    while IFS= read -r line || [ -n "$line" ]; do
      n=$((n + 1))
      line=${line%$'\r'}
      [ -n "${line// /}" ] && printf '%s:%d\t%s\n' "$label" "$n" "$line"
    done <"$f"
  done
}

# import_line LABEL NAME LINE: copies a key from ~/.ssh into keys/ (only the public key line)
import_line() {
  local base file i=2
  base=$(printf '%s' "$2" | LC_ALL=C tr -c 'A-Za-z0-9_-' '_')
  [[ $base =~ [A-Za-z0-9] ]] || base=key
  file=keys/admin-$base.pub
  while [ -e "$file" ]; do
    file=keys/admin-$base-$i.pub
    i=$((i + 1))
  done
  printf '%s\n' "$3" >"$file"
  chmod 644 "$file"
  ok "Copied the public key from $1 to $file"
}

add_admin() {
  local checked rows new login n name default state fp label comment line
  while true; do
    fix_permissions >/dev/null
    checked=$(key_lines | srv check)
    # one row per key; the same key in several files is listed once
    mapfile -t rows < <(awk -F'\t' 'NF && ($2 == "-" || !seen[$2]++)' <<<"$checked")
    # fingerprints of keys that may also log in to this server over SSH
    login=$(awk -F'\t' '$3 ~ /authorized_keys/ { print $2 }' <<<"$checked")
    new=()
    echo
    if [ ${#rows[@]} -eq 0 ]; then
      warn "No public keys found in $SSH_DIR/ or $PWD/keys/."
    else
      echo "  Public keys found:"
      for row in "${rows[@]}"; do
        IFS=$'\t' read -r state fp label comment line <<<"$row"
        case $state in
          new)
            new+=("$row")
            printf '    %2d) %-34s %-24s %s\n' ${#new[@]} "$label" "$comment" "$fp" ;;
          unsupported)
            printf '        %-34s (not usable: %s)\n' "$label" "$comment" ;;
          *)
            printf '        %-34s %-24s already registered as %s\n' "$label" "$comment" "$state" ;;
        esac
      done
    fi
    if [ ${#new[@]} -eq 0 ]; then
      warn "No ssh-ed25519 key that is not registered yet."
      read -rp "  Copy the .pub file into $PWD/keys/ and press Enter to search again (or type s to skip): " n
      [ "$n" = s ] && return 1
      continue
    fi
    echo
    read -rp "  Which one is the client's admin key? [1-${#new[@]}, Enter = search again, s = skip]: " n
    [ "$n" = s ] && return 1
    [[ $n =~ ^[0-9]+$ ]] && [ "$n" -ge 1 ] && [ "$n" -le ${#new[@]} ] || continue
    IFS=$'\t' read -r state fp label comment line <<<"${new[n - 1]}"
    if grep -qxF -- "$fp" <<<"$login"; then
      echo "  This key can also log in to this server over SSH (it is in authorized_keys)."
      ask_yes "Use it as the WHAT Plus admin key too?" Y || continue
    fi
    default=$comment
    [ "$default" = - ] && default=admin
    read -rp "  Name for this key [$default]: " name
    name=${name:-$default}
    [[ $label == "~/.ssh/"* ]] && import_line "$label" "$name" "$line"
    if srv add-key "$fp" admin "$name" >/dev/null; then
      ok "Registered $fp as admin \"$name\""
      return 0
    fi
    warn "Could not register the key; see the message above."
  done
}
admins=$(status_value admins)
if [ "${admins:-0}" -eq 0 ]; then
  echo
  warn "Until an admin key is registered the server refuses every connection and sends nothing."
  add_admin || warn "Skipped. Run sudo ./setup.sh again or use the admin TUI (Import keys) later."
else
  ok "$admins admin key(s) registered"
  ask_yes "Register another admin key?" N && add_admin || true
fi

# ---- 5. Network allowlist ----------------------------------------------------
bold "5/6  Network allowlist"
cat <<'EOF'
  Only computers in these address ranges may connect (checked before the SSH handshake).
  An empty list refuses everything. Examples: 192.168.0.0/16 (every 192.168.x.x),
  192.168.1.0/24, 10.0.0.5 (a single computer).
EOF
suggest_range() {
  local ip
  ip=$(hostname -I 2>/dev/null | tr ' ' '\n' | grep -E '^(10|172|192)\.' | head -1 || true)
  case $ip in
    192.168.*) echo 192.168.0.0/16 ;;
    10.*) echo 10.0.0.0/8 ;;
    172.*) echo 172.16.0.0/12 ;;
    *) echo 192.168.0.0/16 ;;
  esac
}
add_range() {
  local range
  read -rp "  Range [$1]: " range
  range=${range:-$1}
  if [ "$range" = 0.0.0.0/0 ] || [ "$range" = ::/0 ]; then
    ask_yes "$range allows EVERY address. Are you sure?" N || return 0
  fi
  srv allow "$range" >/dev/null && ok "Allowed $range" || warn "Not added; see the message above."
}
allowlist=$(status_value allowlist)
if [ -z "$allowlist" ]; then
  add_range "$(suggest_range)"
else
  ok "Allowed: ${allowlist//,/, }"
fi
last_refused=$(status_value last_refused)
if [ -n "$last_refused" ]; then
  warn "Last refused connection: $last_refused"
  echo "      If that was one of your computers (or an agent on this same Docker host), allow it."
fi
while ask_yes "Add another range?" N; do
  add_range "${last_refused%% *}"
done
fi # MODE=setup

# ---- 6. Start ----------------------------------------------------------------
if [ "$MODE" = setup ]; then
  bold "6/6  Starting the service"
  fix_permissions
  if ! ask_yes "Start (or restart) the service now?" Y; then
    echo
    echo "  Start it later with:  docker compose up -d --force-recreate"
    exit 0
  fi
  docker compose up -d --force-recreate
else
  bold "Upgrading"
  fix_permissions
  docker compose up -d --build --force-recreate
fi

printf '  Waiting for the SSH service '
for _ in $(seq 1 30); do
  if docker compose logs --since 2m server 2>/dev/null | grep -q 'SSH service listening'; then
    echo
    ok "The service is running."
    echo
    echo "  Server fingerprint (compare it when the client or an agent connects the first time):"
    echo
    echo "      $(status_value fingerprint)"
    if docker compose logs --since 2m server 2>/dev/null | grep -q 'Discord rejected'; then
      warn "Discord rejected the bot token. Run sudo ./setup.sh again and replace it."
    fi
    [ "$MODE" = setup ] || exit 0
    cat <<EOF

  Next:
    - Windows client: connect to $(hostname -I 2>/dev/null | awk '{print $1}') port 33445 with the admin key,
      compare the fingerprint above, then add Discord channels and schedules.
    - Each computer to track: run deploy/agent/setup.sh (Linux) or setup.ps1 (Windows).
    - Settings on this server later:  docker compose exec server whatplus-server admin
EOF
    exit 0
  fi
  printf '.'
  sleep 2
done
echo
warn "The service did not start within 60 seconds. Recent log:"
docker compose logs --tail=40 server
exit 1

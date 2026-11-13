#!/usr/bin/env bash
# Interactive setup for a WHAT Plus agent (this deploy directory, Linux):
#   .env        which folders to read (~/.claude, ~/.codex, ~/.gemini) and the user id to read them as
#   keys/       a dedicated SSH key for this computer (generated here)
#   data/       agent.json: server address, verified server fingerprint, upload interval
#   builds and starts the agent and waits for the first upload
#
# Usage, in this directory, as the user whose usage should be uploaded:
#   ./setup.sh            (sudo is fine too; the folders of the user who ran sudo are used)
#   ./setup.sh upgrade    (after git pull: no questions)
# Safe to re-run: press Enter to keep existing values.
set -euo pipefail

SCRIPT=$(readlink -f "$0")
cd "$(dirname "$SCRIPT")"
MODE=
case ${1:-} in
  "") ;;
  upgrade) MODE=upgrade ;;
  *) echo "Usage: ./setup.sh [upgrade]" >&2; exit 2 ;;
esac

bold() { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32mOK\033[0m  %s\n' "$*"; }
warn() { printf '  \033[33m!!\033[0m  %s\n' "$*"; }
fail() { printf '  \033[31mERROR\033[0m  %s\n' "$*" >&2; exit 1; }

ask_yes() {
  local answer hint="[y/N]"
  [ "$2" = Y ] && hint="[Y/n]"
  read -rp "  $1 $hint " answer
  answer=${answer:-$2}
  [[ $answer =~ ^[Yy] ]]
}

# agent ARGS...: runs "whatplus-agent setup ARGS" in a one-off container
agent() {
  docker compose --progress quiet run --rm -T agent setup "$@"
}

# value KEY TEXT: the value of KEY=... in TEXT
value() {
  sed -n "s/^$1=//p" <<<"$2" | head -1
}

command -v docker >/dev/null || fail "Docker is not installed. See https://docs.docker.com/engine/install/"
docker info >/dev/null 2>&1 || fail "Cannot talk to Docker. Run with sudo, or add your user to the docker group and log in again."
docker compose version >/dev/null 2>&1 || fail "The Docker Compose plugin is missing (docker compose)."

# The user whose ~/.claude etc. are read (the invoking user when run with sudo).
OWNER=${SUDO_USER:-$(id -un)}
OWNER_HOME=$(getent passwd "$OWNER" | cut -d: -f6)
OWNER_UID=$(id -u "$OWNER")
OWNER_GID=$(id -g "$OWNER")
[ "$OWNER_UID" -ne 0 ] || warn "Running as root: the agent reads /root/.claude etc. Run as your own user to upload your usage."

mkdir -p data keys
fix_permissions() {
  chown -R "$OWNER_UID:$OWNER_GID" data keys
  chmod 700 data keys
}

configured() {
  [ -f .env ] && [ -f data/agent.json ] && grep -q '"server_fingerprint": *"SHA256' data/agent.json
}

if configured; then
  if [ -z "$MODE" ]; then
    bold "Existing installation found"
    cat <<'EOF'
    1) Upgrade  keep every setting and rebuild/restart the agent (run git pull first)
    2) Setup    review or change the folders, server, key and interval
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
  fail "Nothing to upgrade: the agent is not set up yet. Run ./setup.sh without 'upgrade'."
else
  MODE=setup
fi

if [ "$MODE" = upgrade ]; then
  bold "Upgrading"
  fix_permissions
  docker compose up -d --build --force-recreate
  ok "The agent was rebuilt and restarted."
  exit 0
fi

# ---- 1. Folders --------------------------------------------------------------
bold "1/6  Usage folders  (mounted read-only)"
pick() { # $1 = variable name, $2 = folder under home, $3 = tool
  local current path
  current=$(sed -n "s/^$1=//p" .env 2>/dev/null || true)
  path=${current:-$OWNER_HOME/$2}
  [[ $path == ./empty/* ]] && path=$OWNER_HOME/$2
  read -rp "  $3 folder [$path]: " answer
  path=${answer:-$path}
  if [ -d "$path" ]; then
    ok "$3: $path"
  else
    warn "$3: $path does not exist; $3 usage will be 0"
    mkdir -p "empty/$2"
    path=./empty/$2
  fi
  printf '%s=%s\n' "$1" "$path" >>.env.new
}
: >.env.new
pick CLAUDE_DIR .claude "Claude Code"
pick CODEX_DIR .codex "Codex"
pick GEMINI_DIR .gemini "Antigravity"
printf 'AGENT_UID=%s\nAGENT_GID=%s\n' "$OWNER_UID" "$OWNER_GID" >>.env.new
mv .env.new .env
ok "Saved .env (the agent runs as uid $OWNER_UID so it can read these folders)"

# ---- 2. Build ----------------------------------------------------------------
bold "2/6  Building the image  (the first build takes several minutes)"
fix_permissions
docker compose build
ok "Image built"

# ---- 3. Server address -------------------------------------------------------
bold "3/6  Server address"
echo "  The LAN address of the computer running the WHAT Plus server, e.g. 192.168.1.10."
status=$(agent status)
host=$(value server "$status"); port=$(value port "$status")
while true; do
  read -rp "  Server address${host:+ [$host]}: " answer
  answer=${answer:-$host}
  read -rp "  Port [${port:-33445}]: " p
  p=${p:-${port:-33445}}
  if [ -n "$answer" ] && agent server "$answer" "$p" >/dev/null; then
    host=$answer; port=$p
    ok "Server: $host:$port"
    break
  fi
  warn "Enter an address and a port number."
done

# ---- 4. Key ------------------------------------------------------------------
bold "4/6  Key for this computer"
out=$(agent key "$(hostname)")
public_key=$(value public_key "$out")
if [ "$(value generated "$out")" = yes ]; then
  ok "Generated a new key: keys/whatplus_agent_ed25519"
else
  ok "Using the existing key: keys/whatplus_agent_ed25519"
fi
cat <<EOF
  Register this PUBLIC key on the server as an agent key:

      $public_key

  Either: Windows client -> "Machines" page -> paste it -> Add.
  Or:     copy keys/whatplus_agent_ed25519.pub to the server's deploy/server/keys/ and use
          the server admin TUI -> Import keys -> role "agent".
EOF
read -rp "  Press Enter when it is registered... " _

# ---- 5. Verify the server ----------------------------------------------------
bold "5/6  Verify the server"
refused_hint() {
  warn "Could not connect; see the message above."
  echo "      If the server refused this computer's address: run the server setup again (Setup ->"
  echo "      step 5 shows the last refused address) or add it in the server admin TUI ->"
  echo "      Network allowlist. With Docker the address is often a gateway like 172.18.0.1."
}
verify() {
  local out code fp
  current_fp=$(value fingerprint "$(agent status)")
  if [ -z "$current_fp" ]; then
    out=$(agent probe) || { refused_hint; return 1; }
    fp=$(value fingerprint "$out")
    cat <<EOF
  First connection to $host:$port. Server fingerprint:

      $fp

  Compare it with the fingerprint shown by the server's setup.sh or admin TUI main menu.
EOF
    ask_yes "Is it exactly the same?" N || fail "Not trusted. If the server was reinstalled, run the server setup to see its fingerprint."
    current_fp=$fp
  fi
  set +e
  out=$(agent trust "$current_fp")
  code=$?
  set -e
  case $code in
    0) ok "Logged in as \"$(value name "$out")\""; return 0 ;;
    3) warn "The server rejected this key: it is not registered (or disabled, or this IP is not allowed)." ;;
    4) fail "This key is registered as an ADMIN key. Register it as an agent key instead." ;;
    5) fail "The server fingerprint CHANGED (now $(value actual "$out")). If you reinstalled the server, enter the server address again in setup to reset it; otherwise do not connect." ;;
    *) refused_hint ;;
  esac
  return 1
}
until verify; do
  read -rp "  Press Enter to try again (or type s to skip): " answer
  [ "$answer" = s ] && break
done

# ---- 6. Interval and start ---------------------------------------------------
bold "6/6  Upload interval and start"
interval=$(value interval "$(agent status)")
while true; do
  read -rp "  Upload every how many minutes? (1-1440) [${interval:-10}]: " answer
  answer=${answer:-${interval:-10}}
  agent interval "$answer" >/dev/null && break
  warn "Enter a number from 1 to 1440."
done
ok "Every $answer minutes"

echo
echo "  Usage found on this computer:"
if found=$(agent sources); then
  awk -F'\t' '{ printf "    %-12s %8s records   %s\n", $1, $2, ($4 != "" ? "error: " $4 : $3) }' <<<"$found"
else
  warn "Could not read the usage folders; see the message above."
fi

fix_permissions
docker compose up -d --force-recreate
printf '  Waiting for the first upload (the first one can take a few minutes) '
for _ in $(seq 1 90); do
  line=$(docker compose logs --since 10m agent 2>/dev/null | grep -E 'upload (finished|failed)' | tail -1 || true)
  if [ -n "$line" ]; then
    echo
    case $line in
      *finished*) ok "${line#*: }" ;;
      *) warn "${line#*: }" ;;
    esac
    echo
    echo "  Settings later:  docker compose exec agent whatplus-agent admin"
    exit 0
  fi
  printf '.'
  sleep 2
done
echo
warn "No upload yet. Check: docker compose logs --tail=40 agent"

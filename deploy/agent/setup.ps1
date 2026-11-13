# Interactive setup for a WHAT Plus agent on Windows (Docker Desktop, Linux containers).
# Same steps as setup.sh:
#   .env        which folders to read (%USERPROFILE%\.claude, .codex, .gemini)
#   keys\       a dedicated SSH key for this computer (generated here)
#   data\       agent.json: server address, verified server fingerprint, upload interval
#   builds and starts the agent and waits for the first upload
#
# Usage, in this directory:
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1 upgrade   (after git pull: no questions)
# Safe to re-run: press Enter to keep existing values.
param([string]$Mode = "")
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
if ($Mode -notin @("", "upgrade")) { Write-Host "Usage: .\setup.ps1 [upgrade]"; exit 2 }

function Bold($m) { Write-Host ""; Write-Host $m -ForegroundColor White }
function Ok($m) { Write-Host "  OK  " -ForegroundColor Green -NoNewline; Write-Host $m }
function Warn($m) { Write-Host "  !!  " -ForegroundColor Yellow -NoNewline; Write-Host $m }
function Fail($m) { Write-Host "  ERROR  $m" -ForegroundColor Red; exit 1 }
function AskYes($q, $default) {
    $hint = if ($default -eq "Y") { "[Y/n]" } else { "[y/N]" }
    $a = Read-Host "  $q $hint"
    if (-not $a) { $a = $default }
    return ($a -match '^[Yy]')
}
# Agent ARGS: runs "whatplus-agent setup ARGS" in a one-off container
function Agent { docker compose --progress quiet run --rm -T agent setup @args }
function Value($key, $lines) {
    $line = @($lines) | Where-Object { $_ -like "$key=*" } | Select-Object -First 1
    if ($line) { return $line.Substring($key.Length + 1) } else { return "" }
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { Fail "Docker Desktop is not installed." }
cmd /c "docker info >nul 2>&1"
if ($LASTEXITCODE -ne 0) { Fail "Docker Desktop is not running. Start it and run this script again." }
foreach ($d in "data", "keys") { New-Item -ItemType Directory -Force $d | Out-Null }

$configured = (Test-Path ".env") -and (Test-Path "data\agent.json") -and
    ((Get-Content "data\agent.json" -Raw) -match '"server_fingerprint":\s*"SHA256')
if ($configured) {
    if (-not $Mode) {
        Bold "Existing installation found"
        Write-Host "    1) Upgrade  keep every setting and rebuild/restart the agent (run git pull first)"
        Write-Host "    2) Setup    review or change the folders, server, key and interval"
        while (-not $Mode) {
            $c = Read-Host "  Choose [1]"
            switch ($(if ($c) { $c } else { "1" })) { "1" { $Mode = "upgrade" } "2" { $Mode = "setup" } default { Warn "Type 1 or 2." } }
        }
    }
} elseif ($Mode -eq "upgrade") {
    Fail "Nothing to upgrade: the agent is not set up yet. Run .\setup.ps1 without 'upgrade'."
} else { $Mode = "setup" }

if ($Mode -eq "upgrade") {
    Bold "Upgrading"
    docker compose up -d --build --force-recreate
    if ($LASTEXITCODE -ne 0) { Fail "docker compose failed." }
    Ok "The agent was rebuilt and restarted."
    exit 0
}

# ---- 1. Folders ----------------------------------------------------------------
Bold "1/6  Usage folders  (mounted read-only)"
$old = @{}
if (Test-Path ".env") { Get-Content ".env" | ForEach-Object { if ($_ -match '^(\w+)=(.*)$') { $old[$Matches[1]] = $Matches[2] } } }
$envText = ""
foreach ($item in @(@("CLAUDE_DIR", ".claude", "Claude Code"), @("CODEX_DIR", ".codex", "Codex"), @("GEMINI_DIR", ".gemini", "Antigravity"))) {
    $var, $folder, $tool = $item
    $path = $old[$var]
    if (-not $path -or $path -like "./empty/*") { $path = (Join-Path $env:USERPROFILE $folder) -replace '\\', '/' }
    $a = Read-Host "  $tool folder [$path]"
    if ($a) { $path = $a -replace '\\', '/' }
    if (Test-Path $path -PathType Container) {
        Ok "${tool}: $path"
    } else {
        Warn "${tool}: $path does not exist; $tool usage will be 0"
        New-Item -ItemType Directory -Force "empty\$folder" | Out-Null
        $path = "./empty/$folder"
    }
    $envText += "$var=$path`n"
}
[System.IO.File]::WriteAllText((Join-Path $PSScriptRoot ".env"), $envText, (New-Object System.Text.UTF8Encoding $false))
Ok "Saved .env"

# ---- 2. Build ------------------------------------------------------------------
Bold "2/6  Building the image  (the first build takes several minutes)"
docker compose build
if ($LASTEXITCODE -ne 0) { Fail "docker compose build failed." }
Ok "Image built"

# ---- 3. Server address ---------------------------------------------------------
Bold "3/6  Server address"
Write-Host "  The LAN address of the computer running the WHAT Plus server, e.g. 192.168.1.10."
Write-Host "  If the server runs on THIS computer in Docker Desktop, use host.docker.internal."
$status = Agent status
$hostName = Value server $status
$port = Value port $status
if (-not $port) { $port = "33445" }
while ($true) {
    $prompt = if ($hostName) { "  Server address [$hostName]" } else { "  Server address" }
    $a = Read-Host $prompt
    if (-not $a) { $a = $hostName }
    $p = Read-Host "  Port [$port]"
    if (-not $p) { $p = $port }
    if ($a) {
        Agent server $a $p | Out-Null
        if ($LASTEXITCODE -eq 0) { $hostName = $a; $port = $p; Ok "Server: ${hostName}:$port"; break }
    }
    Warn "Enter an address and a port number."
}

# ---- 4. Key --------------------------------------------------------------------
Bold "4/6  Key for this computer"
$out = Agent key $env:COMPUTERNAME
if ($LASTEXITCODE -ne 0) { Fail "Could not create or read keys\whatplus_agent_ed25519." }
$publicKey = Value public_key $out
if ((Value generated $out) -eq "yes") { Ok "Generated a new key: keys\whatplus_agent_ed25519" }
else { Ok "Using the existing key: keys\whatplus_agent_ed25519" }
Write-Host "  Register this PUBLIC key on the server as an agent key:"
Write-Host ""
Write-Host "      $publicKey"
Write-Host ""
Write-Host '  Either: Windows client -> "Machines" page -> paste it -> Add.'
Write-Host "  Or:     copy keys\whatplus_agent_ed25519.pub to the server's deploy/server/keys/ and use"
Write-Host '          the server admin TUI -> Import keys -> role "agent".'
Read-Host "  Press Enter when it is registered..." | Out-Null

# ---- 5. Verify the server ------------------------------------------------------
Bold "5/6  Verify the server"
function RefusedHint {
    Warn "Could not connect; see the message above."
    Write-Host "      If the server refused this computer's address: run the server setup again (Setup ->"
    Write-Host "      step 5 shows the last refused address) or add it in the server admin TUI ->"
    Write-Host "      Network allowlist. With Docker the address is often a gateway like 172.18.0.1."
}
function Verify {
    $fp = Value fingerprint (Agent status)
    if (-not $fp) {
        $probe = Agent probe
        if ($LASTEXITCODE -ne 0) { RefusedHint; return $false }
        $fp = Value fingerprint $probe
        Write-Host "  First connection to ${hostName}:$port. Server fingerprint:"
        Write-Host ""
        Write-Host "      $fp"
        Write-Host ""
        Write-Host "  Compare it with the fingerprint shown by the server's setup script or admin TUI main menu."
        if (-not (AskYes "Is it exactly the same?" "N")) { Fail "Not trusted. If the server was reinstalled, run the server setup to see its fingerprint." }
    }
    $r = Agent trust $fp
    switch ($LASTEXITCODE) {
        0 { Ok "Logged in as `"$(Value name $r)`""; return $true }
        3 { Warn "The server rejected this key: it is not registered (or disabled, or this IP is not allowed)." }
        4 { Fail "This key is registered as an ADMIN key. Register it as an agent key instead." }
        5 { Fail "The server fingerprint CHANGED (now $(Value actual $r)). If you reinstalled the server, enter the server address again in setup to reset it; otherwise do not connect." }
        default { RefusedHint }
    }
    return $false
}
while (-not (Verify)) {
    $a = Read-Host "  Press Enter to try again (or type s to skip)"
    if ($a -eq "s") { break }
}

# ---- 6. Interval and start -----------------------------------------------------
Bold "6/6  Upload interval and start"
$interval = Value interval (Agent status)
if (-not $interval) { $interval = "10" }
while ($true) {
    $a = Read-Host "  Upload every how many minutes? (1-1440) [$interval]"
    if (-not $a) { $a = $interval }
    Agent interval $a | Out-Null
    if ($LASTEXITCODE -eq 0) { break }
    Warn "Enter a number from 1 to 1440."
}
Ok "Every $a minutes"

Write-Host ""
Write-Host "  Usage found on this computer:"
$found = @(Agent sources | Where-Object { $_ })
if ($LASTEXITCODE -ne 0) { Warn "Could not read the usage folders; see the message above." }
foreach ($line in $found) {
    $f = $line.Split("`t")
    $detail = if ($f[3]) { "error: $($f[3])" } else { $f[2] }
    Write-Host ("    {0,-12} {1,8} records   {2}" -f $f[0], $f[1], $detail)
}

docker compose up -d --force-recreate
if ($LASTEXITCODE -ne 0) { Fail "docker compose failed." }
Write-Host -NoNewline "  Waiting for the first upload (the first one can take a few minutes) "
for ($i = 0; $i -lt 90; $i++) {
    $line = @(docker compose logs --since 10m agent) | Where-Object { $_ -match 'upload (finished|failed)' } | Select-Object -Last 1
    if ($line) {
        Write-Host ""
        $msg = ($line -split ': ', 2)[-1]
        if ($line -match 'finished') { Ok $msg } else { Warn $msg }
        Write-Host ""
        Write-Host "  Settings later:  docker compose exec agent whatplus-agent admin"
        exit 0
    }
    Write-Host -NoNewline "."
    Start-Sleep -Seconds 2
}
Write-Host ""
Warn "No upload yet. Check: docker compose logs --tail=40 agent"

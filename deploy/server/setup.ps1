# Interactive setup for the WHAT Plus server on Windows (Docker Desktop, Linux containers).
# Same steps as setup.sh:
#   config\config.conf   created from config.conf.example, upgraded to the current config.version
#   secrets\discord-token
#   the admin key (from keys\*.pub) and the IP allowlist, stored in the database
#   builds and starts the service and shows the server fingerprint
#
# Usage, in this directory:
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1 upgrade   (after git pull: no questions)
# Safe to re-run: press Enter to keep existing values.
param([string]$Mode = "")
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$ConfigVersion = 1   # matches config::CONFIG_VERSION in the server
$Config = "config\config.conf"
$Example = "config\config.conf.example"
$Token = "secrets\discord-token"
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
function Write-Utf8NoBom($path, $text) {
    [System.IO.File]::WriteAllText((Join-Path $PSScriptRoot $path), $text, (New-Object System.Text.UTF8Encoding $false))
}
# Srv ARGS: runs "whatplus-server setup ARGS" in a one-off container (same database)
function Srv { docker compose --progress quiet run --rm -T --no-deps server setup @args }
function StatusValue($key) {
    $line = Srv status | Where-Object { $_ -like "$key=*" } | Select-Object -First 1
    if ($line) { return $line.Substring($key.Length + 1) } else { return "" }
}
function LanAddress {
    $ip = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object { $_.IPAddress -match '^(10|172|192)\.' -and $_.PrefixOrigin -ne 'WellKnown' -and $_.InterfaceAlias -notmatch 'vEthernet|WSL|Docker' } |
        Select-Object -First 1
    if ($ip) { return $ip.IPAddress } else { return "" }
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { Fail "Docker Desktop is not installed." }
cmd /c "docker info >nul 2>&1"
if ($LASTEXITCODE -ne 0) { Fail "Docker Desktop is not running. Start it and run this script again." }
if (-not (Test-Path $Example)) { Fail "$Example not found. Run this script from deploy\server of the project." }
foreach ($d in "data", "keys", "secrets") { New-Item -ItemType Directory -Force $d | Out-Null }

# ---- config.conf: create it, or upgrade it to $ConfigVersion -----------------
if (Test-Path $Config -PathType Container) {
    if ((Get-ChildItem $Config -Force | Measure-Object).Count -ne 0) { Fail "$Config is a directory with files in it. Move them out, remove it and run again." }
    Remove-Item $Config
    Warn "Removed ${Config}: it was a directory created by docker compose, not a file"
}
if (-not (Test-Path $Config)) {
    Write-Utf8NoBom $Config ((Get-Content $Example -Raw) -replace "`r`n", "`n")
    Ok "Created $Config from $Example"
}
$m = Select-String -Path $Config -Pattern '^config\.version=(\d+)' | Select-Object -First 1
if (-not $m) { Fail "config.version is missing in $Config." }
$version = [int]$m.Matches[0].Groups[1].Value
if ($version -gt $ConfigVersion) { Fail "$Config is version $version, newer than this script understands ($ConfigVersion). Run git pull first." }
if ($version -lt $ConfigVersion) { Fail "No upgrade step from config.version=$version." }

function Installed { (Test-Path $Token) -and ((Get-Item $Token).Length -gt 0) -and (Test-Path "data\whatplus.db") }

if (Installed) {
    if (-not $Mode) {
        Bold "Existing installation found"
        Write-Host "    1) Upgrade  keep every setting and rebuild/restart the service (run git pull first)"
        Write-Host "    2) Setup    review or change the time zone, Discord token, admin keys and allowlist"
        while (-not $Mode) {
            $c = Read-Host "  Choose [1]"
            switch ($(if ($c) { $c } else { "1" })) { "1" { $Mode = "upgrade" } "2" { $Mode = "setup" } default { Warn "Type 1 or 2." } }
        }
    }
} elseif ($Mode -eq "upgrade") {
    Fail "Nothing to upgrade: the server is not set up yet. Run .\setup.ps1 without 'upgrade'."
} else { $Mode = "setup" }

if ($Mode -eq "setup") {
    # ---- 1. Time zone ----------------------------------------------------------
    Bold "1/6  Time zone"
    Write-Host "  Daily / weekly / monthly reports and their periods use this time zone (IANA name)."
    $current = ((Select-String -Path $Config -Pattern '^timezone=(.*)$').Matches | Select-Object -First 1).Groups[1].Value
    if (-not $current) { $current = "Asia/Taipei" }
    do {
        $tz = Read-Host "  Time zone [$current]"
        if (-not $tz) { $tz = $current }
        $valid = $tz -match '^[A-Za-z_]+(/[A-Za-z0-9_+-]+)*$'
        if (-not $valid) { Warn "Use a name like Asia/Taipei, Europe/London or UTC." }
    } until ($valid)
    Write-Utf8NoBom $Config (((Get-Content $Config -Raw) -replace "(?m)^timezone=.*$", "timezone=$tz") -replace "`r`n", "`n")
    Ok "timezone=$tz"

    # ---- 2. Discord bot token --------------------------------------------------
    Bold "2/6  Discord bot token"
    Write-Host '  Where to get it: https://discord.com/developers/applications -> your application -> "Bot"'
    Write-Host '  -> "Reset Token" -> "Copy". It looks like  MTIz...xyz.Gh1abc.AbCd...  (three parts with dots).'
    Write-Host '  Not the Application ID or Public Key from "General Information".'
    if ((Test-Path $Token) -and ((Get-Item $Token).Length -gt 0) -and -not (AskYes "A Discord token is already saved. Replace it?" "N")) {
        Ok "Keeping the existing token"
    } else {
        while ($true) {
            $secure = Read-Host "  Bot token (hidden)" -AsSecureString
            $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
            if (-not $plain) { Warn "The token is required."; continue }
            if ($plain -notmatch '^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$') {
                Warn "That does not look like a bot token (three parts separated by dots)."
                if (-not (AskYes "Use it anyway?" "N")) { continue }
            }
            break
        }
        Write-Utf8NoBom $Token $plain
        $plain = $null
        Ok "Saved $Token"
    }

    # ---- 3. Build --------------------------------------------------------------
    Bold "3/6  Building the image  (the first build takes several minutes)"
    docker compose build
    if ($LASTEXITCODE -ne 0) { Fail "docker compose build failed." }
    Ok "Image built"
    Ok "Server fingerprint: $(StatusValue fingerprint)"

    # ---- 4. Admin key ----------------------------------------------------------
    Bold "4/6  Admin key  (the Windows client app)"
    Write-Host "  The client app logs in with an SSH key. On the computer that runs the client:"
    Write-Host '      ssh-keygen -t ed25519 -f $env:USERPROFILE\.ssh\whatplus_admin -C "whatplus-admin"'
    Write-Host "  Then copy the PUBLIC key (whatplus_admin.pub, never the file without .pub) into"
    Write-Host "      $PSScriptRoot\keys\"
    Write-Host "  One file may contain several keys, one per line. Only ssh-ed25519 keys are supported."
    function AddAdmin {
        while ($true) {
            $rows = @(Srv scan | Where-Object { $_ })
            $new = @($rows | Where-Object { $_.Split("`t")[0] -eq "new" })
            Write-Host ""
            if ($rows.Count -gt 0) {
                Write-Host "  Keys in keys\:"
                foreach ($r in $rows) {
                    $f = $r.Split("`t")
                    $state = if ($f[0] -eq "new") { "" } else { "[$($f[0])]" }
                    Write-Host ("    {0,-12} {1,-52} {2}  {3}" -f $state, $f[1], $f[2], $f[3])
                }
            }
            if ($new.Count -eq 0) {
                Warn "No new ssh-ed25519 key in keys\ yet."
                $a = Read-Host "  Copy the .pub file there and press Enter to search again (or type s to skip)"
                if ($a -eq "s") { return $false }
                continue
            }
            Write-Host ""
            Write-Host "  Not registered yet:"
            for ($i = 0; $i -lt $new.Count; $i++) {
                $f = $new[$i].Split("`t")
                Write-Host ("    {0}) {1}  {2}  {3}" -f ($i + 1), $f[1], $f[2], $f[3])
            }
            $n = Read-Host "  Which one is the client's admin key? [1-$($new.Count), Enter = search again]"
            if ($n -notmatch '^\d+$' -or [int]$n -lt 1 -or [int]$n -gt $new.Count) { continue }
            $f = $new[[int]$n - 1].Split("`t")
            $default = if ($f[3]) { $f[3] } else { "admin" }
            $name = Read-Host "  Name for this key [$default]"
            if (-not $name) { $name = $default }
            Srv add-key $f[1] admin $name | Out-Null
            if ($LASTEXITCODE -eq 0) { Ok "Registered $($f[1]) as admin `"$name`""; return $true }
            Warn "Could not register the key; see the message above."
        }
    }
    $admins = [int](StatusValue admins)
    if ($admins -eq 0) {
        Write-Host ""
        Warn "Until an admin key is registered the server refuses every connection and sends nothing."
        if (-not (AddAdmin)) { Warn "Skipped. Run this script again or use the admin TUI (Import keys) later." }
    } else {
        Ok "$admins admin key(s) registered"
        if (AskYes "Register another admin key?" "N") { AddAdmin | Out-Null }
    }

    # ---- 5. Network allowlist --------------------------------------------------
    Bold "5/6  Network allowlist"
    Write-Host "  Only computers in these address ranges may connect (checked before the SSH handshake)."
    Write-Host "  An empty list refuses everything. Examples: 192.168.0.0/16 (every 192.168.x.x),"
    Write-Host "  192.168.1.0/24, 10.0.0.5 (a single computer)."
    Write-Host ""
    Warn "Docker Desktop: connections may reach the server from a Docker gateway address (for"
    Warn "example 172.17.0.1) instead of the real computer. If a client is refused, open the admin"
    Warn "TUI -> System status after it tries, and see the user guide (docs/INSTALL.md)."
    function AddRange($default) {
        $r = Read-Host "  Range [$default]"
        if (-not $r) { $r = $default }
        if ($r -in @("0.0.0.0/0", "::/0") -and -not (AskYes "$r allows EVERY address. Are you sure?" "N")) { return }
        Srv allow $r | Out-Null
        if ($LASTEXITCODE -eq 0) { Ok "Allowed $r" } else { Warn "Not added; see the message above." }
    }
    $allow = StatusValue allowlist
    if (-not $allow) {
        $lan = LanAddress
        $suggest = if ($lan -like "10.*") { "10.0.0.0/8" } elseif ($lan -like "172.*") { "172.16.0.0/12" } else { "192.168.0.0/16" }
        AddRange $suggest
    } else { Ok "Allowed: $($allow -replace ',', ', ')" }
    $lastRefused = StatusValue last_refused
    if ($lastRefused) {
        Warn "Last refused connection: $lastRefused"
        Write-Host "      If that was one of your computers (or an agent on this same Docker host), allow it."
    }
    while (AskYes "Add another range?" "N") { AddRange ($lastRefused -split ' ')[0] }
}

# ---- 6. Start ------------------------------------------------------------------
if ($Mode -eq "setup") {
    Bold "6/6  Starting the service"
    if (-not (AskYes "Start (or restart) the service now?" "Y")) {
        Write-Host ""
        Write-Host "  Start it later with:  docker compose up -d --force-recreate"
        exit 0
    }
    docker compose up -d --force-recreate
} else {
    Bold "Upgrading"
    docker compose up -d --build --force-recreate
}
if ($LASTEXITCODE -ne 0) { Fail "docker compose failed." }

Write-Host -NoNewline "  Waiting for the SSH service "
for ($i = 0; $i -lt 30; $i++) {
    $logs = (docker compose logs --since 2m server) -join "`n"
    if ($logs -match 'SSH service listening') {
        Write-Host ""
        Ok "The service is running."
        Write-Host ""
        Write-Host "  Server fingerprint (compare it when the client or an agent connects the first time):"
        Write-Host ""
        Write-Host "      $(StatusValue fingerprint)"
        if ($logs -match 'Discord rejected') { Warn "Discord rejected the bot token. Run this script again and replace it." }
        if ($Mode -ne "setup") { exit 0 }
        Write-Host ""
        Write-Host "  Next:"
        Write-Host "    - Windows client: connect to $(LanAddress) port 33445 with the admin key,"
        Write-Host "      compare the fingerprint above, then add Discord channels and schedules."
        Write-Host "    - Each computer to track: run deploy\agent\setup.ps1 (Windows) or setup.sh (Linux)."
        Write-Host "    - Settings on this server later:  docker compose exec server whatplus-server admin"
        exit 0
    }
    Write-Host -NoNewline "."
    Start-Sleep -Seconds 2
}
Write-Host ""
Warn "The service did not start within 60 seconds. Recent log:"
docker compose logs --tail=40 server
exit 1

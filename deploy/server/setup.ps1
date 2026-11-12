# WHAT Plus server setup (Windows, Docker Desktop). Run in this directory:
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1 upgrade
param([string]$Mode = "")
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$ConfigVersion = 1
$Conf = "config\config.conf"

function Ok($m) { Write-Host "OK  $m" }
function Fail($m) { Write-Host "ERROR  $m" -ForegroundColor Red; exit 1 }

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { Fail "Docker Desktop is not installed." }

# docker compose creates a missing bind-mount source as an empty directory.
if (Test-Path $Conf -PathType Container) {
    if ((Get-ChildItem $Conf -Force | Measure-Object).Count -eq 0) { Remove-Item $Conf } else { Fail "$Conf is a directory; remove it and run again." }
}
foreach ($d in "data", "keys", "secrets") { New-Item -ItemType Directory -Force $d | Out-Null }

function Write-Utf8NoBom($path, $text) {
    [System.IO.File]::WriteAllText((Join-Path $PSScriptRoot $path), $text, (New-Object System.Text.UTF8Encoding $false))
}

if (-not (Test-Path $Conf)) {
    $tz = Read-Host "Time zone for reports [Asia/Taipei]"
    if (-not $tz) { $tz = "Asia/Taipei" }
    $text = (Get-Content "config\config.conf.example" -Raw) -replace "(?m)^timezone=.*$", "timezone=$tz"
    Write-Utf8NoBom $Conf ($text -replace "`r`n", "`n")
    Ok "Created $Conf"
}
$v = (Select-String -Path $Conf -Pattern '^config.version=(\d+)').Matches | ForEach-Object { [int]$_.Groups[1].Value }
if ($v -and $v -gt $ConfigVersion) { Fail "$Conf was written by a newer release (version $v)." }

if ($Mode -ne "upgrade") {
    Write-Host "`n== Discord bot token"
    Write-Host "Discord Developer Portal > your application > Bot > Reset Token / Copy."
    $replace = $true
    if ((Test-Path "secrets\discord-token") -and (Get-Item "secrets\discord-token").Length -gt 0) {
        $keep = Read-Host "A token is already saved. Press Enter to keep it, or type 'n' to replace"
        $replace = ($keep -eq "n")
    }
    if ($replace) {
        $secure = Read-Host "Paste the token (input is hidden)" -AsSecureString
        $plain = [Runtime.InteropServices.Marshal]::PtrToStringBSTR([Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure))
        if (-not $plain) { Fail "Empty token." }
        Write-Utf8NoBom "secrets\discord-token" $plain
        $plain = $null
        Ok "Token saved to secrets\discord-token"
    }
    $go = Read-Host "Start the service now? [Y/n]"
    if ($go -eq "n") { exit 0 }
}

Write-Host "`n== Building and starting the service (the first build takes several minutes)"
docker compose up -d --build
if ($LASTEXITCODE -ne 0) { Fail "docker compose failed." }
$fp = $null
for ($i = 0; $i -lt 30 -and -not $fp; $i++) {
    Start-Sleep -Seconds 2
    $m = (docker compose logs server 2>$null | Select-String 'host key fingerprint: (SHA256:\S+)' | Select-Object -Last 1)
    if ($m) { $fp = $m.Matches[0].Groups[1].Value }
}
if (-not $fp) { Fail "The service did not start; check: docker compose logs --tail=40 server" }
Ok "The service is running. Host key fingerprint: $fp"

Write-Host @"

Next steps:
  1. Copy the client's admin public key (and agents' keys) into keys\, e.g.
       copy %USERPROFILE%\.ssh\id_ed25519.pub keys\admin.pub
  2. Open the admin TUI:   docker compose exec server whatplus-server admin
       - Network allowlist: add your LAN (see README: Docker Desktop and client IPs)
       - Import keys: select the admin key and give it the admin role
  3. Connect with the Windows client and compare the fingerprint shown above.
"@

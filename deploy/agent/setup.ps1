# WHAT Plus agent setup (Windows, Docker Desktop). Run in this directory:
#   powershell -ExecutionPolicy Bypass -File .\setup.ps1
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Ok($m) { Write-Host "OK  $m" }
function Fail($m) { Write-Host "ERROR  $m" -ForegroundColor Red; exit 1 }
if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { Fail "Docker Desktop is not installed." }

function Pick($name) {
    $p = Join-Path $env:USERPROFILE $name
    if (Test-Path $p -PathType Container) { return ($p -replace '\\', '/') }
    New-Item -ItemType Directory -Force "empty\$name" | Out-Null
    return "./empty/$name"
}

foreach ($d in "data", "keys") { New-Item -ItemType Directory -Force $d | Out-Null }
$env_text = "CLAUDE_DIR=$(Pick '.claude')`nCODEX_DIR=$(Pick '.codex')`nGEMINI_DIR=$(Pick '.gemini')`n"
[System.IO.File]::WriteAllText((Join-Path $PSScriptRoot ".env"), $env_text, (New-Object System.Text.UTF8Encoding $false))
Ok "Wrote .env:"
Get-Content .env | ForEach-Object { "    $_" }

$go = Read-Host "Build and start the agent now? [Y/n]"
if ($go -ne "n") {
    docker compose up -d --build
    if ($LASTEXITCODE -ne 0) { Fail "docker compose failed." }
    Ok "Agent started. It waits until it is configured."
}

Write-Host @"

Next steps:
  1. Open the agent TUI:   docker compose exec agent whatplus-agent admin
       - Server address: the server's LAN address
       - Private key: generate a new key (stored in keys\)
  2. Register the shown public key as an agent key (client app > Machines).
  3. Back in the TUI: "Verify server and test login" and compare the fingerprint.
"@

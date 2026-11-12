$ErrorActionPreference = 'Stop'

$project = $PSScriptRoot
$webRoot = Join-Path $project 'web'
$wasmSource = Join-Path $project 'target\wasm32-unknown-unknown\release\valox_engine.wasm'
$wasmTarget = Join-Path $webRoot 'valox.wasm'
$port = 8765
$url = "http://127.0.0.1:$port/"
$pidFile = Join-Path $env:TEMP 'valox-local-web.pid'

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargoCommand) { $cargoPath = $cargoCommand.Source }
else { $cargoPath = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargoPath)) { throw 'Cargo wurde nicht gefunden. Installiere Rust oder öffne Valox in der Visual-Studio-Entwickler-PowerShell.' }

& $cargoPath build --release --target wasm32-unknown-unknown --lib
if ($LASTEXITCODE -ne 0) { throw 'Der WebAssembly-Build ist fehlgeschlagen.' }
Copy-Item -LiteralPath $wasmSource -Destination $wasmTarget -Force

$existing = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
if ($existing) { throw "Port $port ist bereits belegt. Beende den vorhandenen Valox-Webserver oder ändere den Port in run-browser.ps1." }

$pythonCommand = Get-Command python -ErrorAction SilentlyContinue
if (-not $pythonCommand) { throw 'Python wurde nicht gefunden. Installiere Python 3 oder nutze den Befehl aus README.md.' }
$arguments = '-m http.server {0} --bind 127.0.0.1 --directory "{1}"' -f $port, $webRoot
$server = Start-Process -FilePath $pythonCommand.Source -ArgumentList $arguments -WindowStyle Hidden -PassThru
Set-Content -LiteralPath $pidFile -Value $server.Id -Encoding ascii
Start-Sleep -Milliseconds 900

try {
    Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 5 | Out-Null
} catch {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
    throw 'Der lokale Webserver konnte nicht gestartet werden.'
}

Start-Process $url
Write-Host "Valox läuft im Browser: $url"
Write-Host "Zum Beenden: .\stop-browser.ps1"

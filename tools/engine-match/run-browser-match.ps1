param(
  [ValidateRange(1,40)][int]$Games = 40,
  [ValidateRange(0,60)][int]$Minutes = 0,
  [ValidateRange(1,180)][int]$Seconds = 15,
  [ValidateRange(1320,3190)][int]$StockfishElo = 2000
)

$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent | Split-Path -Parent
$port = 8766
$url = "http://127.0.0.1:$port/match.html"
$clockMs = if ($Minutes -gt 0) { $Minutes * 60 * 1000 } else { $Seconds * 1000 }
$clockLabel = if ($Minutes -gt 0) { "$Minutes Min. pro Engine" } else { "$Seconds Sek. pro Engine" }
$pidFile = Join-Path $env:TEMP 'valox-live-match.pid'
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path -LiteralPath $cargo)) { $cmd = Get-Command cargo -ErrorAction SilentlyContinue; if ($cmd) { $cargo = $cmd.Source } }
if (-not (Test-Path -LiteralPath $cargo)) { throw 'Cargo fehlt. Installiere Rust, um Valox zu bauen.' }
& $cargo build --release --target-dir (Join-Path $project 'target\valox-upgrade')
if ($LASTEXITCODE -ne 0) { throw 'Valox konnte nicht gebaut werden.' }
$engineBuild = Join-Path $project 'target\valox-upgrade\release\valox.exe'
$engineLaunch = Join-Path $project 'build\valox.exe'
Copy-Item -LiteralPath $engineBuild -Destination $engineLaunch -Force
if (Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue) { throw "Port $port ist bereits belegt. Beende erst ein laufendes Valox-Match mit .\stop-browser-match.ps1." }
$node = Get-Command node -ErrorAction SilentlyContinue
if (-not $node) { throw 'Node.js wurde für den lokalen Match-Server nicht gefunden.' }
$env:VALOX_MATCH_GAMES = [string]$Games
$env:VALOX_MATCH_TIME_MS = [string]$clockMs
$env:VALOX_STOCKFISH_ELO = [string]$StockfishElo
$server = Start-Process -FilePath $node.Source -ArgumentList 'server.js' -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -PassThru
Set-Content -LiteralPath $pidFile -Value $server.Id -Encoding ascii
$ready = $false
for ($i = 0; $i -lt 40; $i++) {
  Start-Sleep -Milliseconds 250
  try { Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 1 | Out-Null; $ready = $true; break } catch {}
}
if (-not $ready) { Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue; throw 'Der Match-Server konnte nicht gestartet werden.' }
Start-Process $url
Write-Host "Das Live-Match ist geöffnet: $url"
Write-Host "Match: $Games Partie(n), $clockLabel, ohne Zuschlag pro Zug."
Write-Host "Stockfish ist auf UCI_Elo $StockfishElo eingestellt."
if ($Games -eq 1) { Write-Host 'Valox spielt in dieser Partie Weiß.' }
Write-Host 'Beenden: .\stop-browser-match.ps1'

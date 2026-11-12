$ErrorActionPreference = 'Stop'
$port = 8766
$pidFile = Join-Path $env:TEMP 'valox-live-match.pid'
try { Invoke-WebRequest -Uri "http://127.0.0.1:$port/api/stop" -Method Post -UseBasicParsing -TimeoutSec 2 | Out-Null } catch {}
if (Test-Path -LiteralPath $pidFile) {
  $serverId = [int](Get-Content -LiteralPath $pidFile -Raw)
  Stop-Process -Id $serverId -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $pidFile -Force -ErrorAction SilentlyContinue
}
Write-Host 'Der lokale Match-Server wurde beendet.'

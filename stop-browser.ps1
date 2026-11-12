$ErrorActionPreference = 'Stop'
$pidFile = Join-Path $env:TEMP 'valox-local-web.pid'
if (-not (Test-Path -LiteralPath $pidFile)) { Write-Host 'Kein Valox-Webserver ist registriert.'; exit 0 }

$serverId = [int](Get-Content -LiteralPath $pidFile -Raw)
$process = Get-CimInstance Win32_Process -Filter "ProcessId = $serverId" -ErrorAction SilentlyContinue
if ($process -and $process.CommandLine -like '*http.server 8765*' -and $process.CommandLine -like '*Valox Engine\web*') {
    Stop-Process -Id $serverId -Force
    Write-Host 'Valox-Webserver wurde beendet.'
} else {
    Write-Host 'Der registrierte Prozess ist kein Valox-Webserver mehr.'
}
Remove-Item -LiteralPath $pidFile -Force

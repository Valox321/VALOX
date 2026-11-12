param(
  [string]$Engine = (Join-Path (Split-Path $PSScriptRoot -Parent | Split-Path -Parent) 'build\valox.exe')
)

$ErrorActionPreference = 'Stop'
$enginePath = (Resolve-Path -LiteralPath $Engine).Path
$process = [System.Diagnostics.Process]::new()
$process.StartInfo.FileName = $enginePath
$process.StartInfo.UseShellExecute = $false
$process.StartInfo.RedirectStandardInput = $true
$process.StartInfo.RedirectStandardOutput = $true
if (-not $process.Start()) { throw "Could not start $enginePath" }
$script:UciOutput = [System.Collections.Generic.List[string]]::new()

function Send-Uci([string]$Command) {
  $process.StandardInput.WriteLine($Command)
  $process.StandardInput.Flush()
}

function Read-UciUntil([string]$ExpectedPrefix, [int]$TimeoutSeconds = 30) {
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    $remaining = [Math]::Max(1, [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds)
    $read = $process.StandardOutput.ReadLineAsync()
    $line = $read.WaitAsync([TimeSpan]::FromMilliseconds($remaining)).GetAwaiter().GetResult()
    if ($null -eq $line) { throw "Engine exited before '$ExpectedPrefix'" }
    $script:UciOutput.Add($line)
    Write-Host $line
    if ($line.StartsWith($ExpectedPrefix, [StringComparison]::Ordinal)) { return $line }
  }
  throw "Timed out waiting for '$ExpectedPrefix'"
}

try {
  Send-Uci 'uci'
  Read-UciUntil 'uciok' | Out-Null
  if (-not ($script:UciOutput -join "`n").Contains('option name UCI_Chess960 type check')) {
    throw 'Engine did not advertise UCI_Chess960'
  }

  Send-Uci 'isready'
  Read-UciUntil 'readyok' | Out-Null
  Send-Uci 'setoption name Threads value 8'
  Send-Uci 'setoption name Hash value 128'
  Send-Uci 'position startpos'
  Send-Uci 'go depth 6'
  $standardBest = Read-UciUntil 'bestmove'
  if ($standardBest -notmatch '^bestmove [a-h][1-8][a-h][1-8][qrbn]?$') {
    throw "Invalid standard bestmove response: $standardBest"
  }

  Send-Uci 'setoption name UCI_Chess960 value true'
  Send-Uci 'position fen 4k3/8/8/8/8/8/8/R1K4R w HA - 0 1 moves c1a1'
  Send-Uci 'go depth 4'
  $chess960Best = Read-UciUntil 'bestmove'
  if ($chess960Best -notmatch '^bestmove [a-h][1-8][a-h][1-8][qrbn]?$') {
    throw "Invalid Chess960 bestmove response: $chess960Best"
  }

  Send-Uci 'go infinite'
  Read-UciUntil 'info' | Out-Null
  Send-Uci 'stop'
  $stoppedBest = Read-UciUntil 'bestmove'
  if ($stoppedBest -notmatch '^bestmove [a-h][1-8][a-h][1-8][qrbn]?$') {
    throw "Invalid bestmove after stop: $stoppedBest"
  }

  Send-Uci 'quit'
  $process.WaitForExit(5000) | Out-Null
  if (-not $process.HasExited) { throw 'Engine did not exit after quit' }
  Write-Output "UCI verification passed: standard=$standardBest chess960=$chess960Best stop=$stoppedBest"
}
finally {
  if (-not $process.HasExited) {
    $process.Kill()
    $process.WaitForExit()
  }
  $process.Dispose()
}

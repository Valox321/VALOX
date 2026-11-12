param(
  [int]$Rounds = 10,
  [string]$TimeControl = '15+0',
  [ValidateRange(1, 8)]
  [int]$Concurrency = 2
)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$project = Split-Path $root -Parent | Split-Path -Parent
$valox = Join-Path $project 'build\valox.exe'
$syzygy = Join-Path $project 'syzygy\3-4-5-wdl'
$stockfish = Join-Path $root 'stockfish\stockfish\stockfish-windows-x86-64-universal.exe'
$cute = Join-Path $root 'cutechess\cutechess-1.5.1-win64\cutechess-cli.exe'
$openings = Join-Path $root 'openings.pgn'
$results = Join-Path $root 'results'
New-Item -ItemType Directory -Force -Path $results | Out-Null
foreach ($path in @($valox,$syzygy,$stockfish,$cute,$openings)) { if (-not (Test-Path -LiteralPath $path)) { throw "Datei fehlt: $path" } }
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$pgn = Join-Path $results "valox-stockfish-2000-$stamp.pgn"
$log = Join-Path $results "valox-stockfish-2000-$stamp.txt"
$arguments = @(
  '-engine', 'name=Valox', "cmd=$valox", 'proto=uci', 'option.Hash=256', "option.SyzygyPath=$syzygy",
  '-engine', 'name=Stockfish19-2000', "cmd=$stockfish", 'proto=uci', 'option.Threads=1', 'option.Hash=256', "option.SyzygyPath=$syzygy", 'option.UCI_LimitStrength=true', 'option.UCI_Elo=2000',
  '-each', "tc=$TimeControl", 'timemargin=100',
  '-games', '2', '-rounds', "$Rounds", '-repeat', '-concurrency', "$Concurrency", '-srand', '20261111',
  '-openings', "file=$openings", 'format=pgn', 'order=sequential', 'plies=8',
  '-event', "Valox vs Stockfish 19 at UCI_Elo 2000 ($Concurrency concurrent games)", '-site', 'Valox local benchmark',
  '-pgnout', $pgn, '-ratinginterval', '2'
)
& $cute @arguments 2>&1 | Tee-Object -FilePath $log
if ($LASTEXITCODE -ne 0) { throw "Cute Chess endete mit Fehlercode $LASTEXITCODE. Siehe $log" }
Write-Host "PGN: $pgn"
Write-Host "Log: $log"

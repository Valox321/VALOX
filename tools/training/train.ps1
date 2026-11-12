param(
    [string]$TrainingData = ".\data-1500\train.data",
    [string]$ValidationData = ".\data-1500\validation.data",
    [string]$CheckpointFolder = ".\checkpoints-1500",
    [float]$WdlMix = 0.5
)

$ErrorActionPreference = "Stop"
$cudaPathValox = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v13.5"
if (-not (Test-Path -LiteralPath (Join-Path $cudaPathValox "bin\nvcc.exe"))) {
    throw "CUDA Toolkit compiler was not found at $cudaPathValox."
}

$env:CUDA_PATH = $cudaPathValox
$env:Path = "$cudaPathValox\bin;$cudaPathValox\bin\x64;$env:Path"

Push-Location $PSScriptRoot
try {
    cargo run --release --bin valox-trainer --features cuda -- $TrainingData $ValidationData $CheckpointFolder $WdlMix
    if ($LASTEXITCODE -ne 0) { throw "Bullet training failed with exit code $LASTEXITCODE." }
}
finally {
    Pop-Location
}

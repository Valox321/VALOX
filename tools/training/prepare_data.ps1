param(
    [string]$BulletUtilsPath = "",
    [string]$DataDirectory = (Join-Path $PSScriptRoot "data")
)

$ErrorActionPreference = "Stop"
$dataDir = (Resolve-Path -LiteralPath $DataDirectory).Path
$trainText = Join-Path $dataDir "train.data"
$validationText = Join-Path $dataDir "validation.data"

foreach ($path in @($trainText, $validationText)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Training data file not found: $path. Run generate_data.py first."
    }
}

if (-not $BulletUtilsPath) {
    $installed = Get-Command bullet-utils.exe -ErrorAction SilentlyContinue
    if ($installed) {
        $BulletUtilsPath = $installed.Source
    }
}

if (-not $BulletUtilsPath) {
    $bulletSource = Join-Path $PSScriptRoot ".bullet-source"
    $bulletManifest = Join-Path $bulletSource "Cargo.toml"
    $bulletUtils = Join-Path $bulletSource "target\release\bullet-utils.exe"
    $bulletRevision = "5d70177680437b7c0ddf958bd4722d95afa78b0c"

    if (-not (Test-Path -LiteralPath $bulletManifest -PathType Leaf)) {
        if (Test-Path -LiteralPath $bulletSource) {
            throw "Incomplete Bullet checkout exists at $bulletSource; remove that folder and rerun."
        }
        git clone --filter=blob:none --no-checkout https://github.com/jw1912/bullet $bulletSource
        if ($LASTEXITCODE -ne 0) { throw "Could not download Bullet source." }
        git -C $bulletSource checkout --detach $bulletRevision
        if ($LASTEXITCODE -ne 0) { throw "Could not select the pinned Bullet revision." }
    }

    cargo build --release --package bullet-utils --manifest-path $bulletManifest
    if ($LASTEXITCODE -ne 0) { throw "Could not build Bullet's data utility." }
    $BulletUtilsPath = $bulletUtils
}

if (-not (Test-Path -LiteralPath $BulletUtilsPath -PathType Leaf)) {
    throw "bullet-utils executable not found: $BulletUtilsPath"
}

$trainRaw = Join-Path $dataDir "train-unshuffled.bin"
$validationRaw = Join-Path $dataDir "validation-unshuffled.bin"
$trainBinary = Join-Path $dataDir "train.bin"
$validationBinary = Join-Path $dataDir "validation.bin"
Remove-Item -LiteralPath @($trainRaw, $validationRaw, $trainBinary, $validationBinary) -Force -ErrorAction SilentlyContinue

& $BulletUtilsPath convert --from text --input $trainText --output $trainRaw
if ($LASTEXITCODE -ne 0) { throw "Could not convert training data." }
& $BulletUtilsPath convert --from text --input $validationText --output $validationRaw
if ($LASTEXITCODE -ne 0) { throw "Could not convert validation data." }

& $BulletUtilsPath shuffle --input $trainRaw --output $trainBinary --mem-used-mb 512
if ($LASTEXITCODE -ne 0) { throw "Could not shuffle training data." }
& $BulletUtilsPath shuffle --input $validationRaw --output $validationBinary --mem-used-mb 512
if ($LASTEXITCODE -ne 0) { throw "Could not shuffle validation data." }

& $BulletUtilsPath validate --input $trainBinary
if ($LASTEXITCODE -ne 0) { throw "Bullet rejected the training dataset." }
& $BulletUtilsPath validate --input $validationBinary
if ($LASTEXITCODE -ne 0) { throw "Bullet rejected the validation dataset." }

Remove-Item -LiteralPath @($trainRaw, $validationRaw) -Force
Write-Host "Prepared shuffled Bullet datasets:"
Write-Host "  $trainBinary"
Write-Host "  $validationBinary"

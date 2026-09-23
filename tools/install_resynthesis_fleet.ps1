param(
    [string]$OutputRoot = (Join-Path (Split-Path $PSScriptRoot -Parent) 'output\resynthesis-profiles-20260923'),
    [string]$Mods = 'D:\BeamMP\current\mods',
    [string]$Backup = (Join-Path (Split-Path $PSScriptRoot -Parent) 'output\beamng-bess-backup-before-resynthesis-profiles-20260923')
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
$repository = (Resolve-Path -LiteralPath $repository).Path
$modsRoot = (Resolve-Path -LiteralPath $Mods).Path
$outputRootResolved = (Resolve-Path -LiteralPath $OutputRoot).Path
$backupParent = (Resolve-Path -LiteralPath (Split-Path $Backup -Parent)).Path
if (-not $outputRootResolved.StartsWith($repository + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or
    -not $backupParent.StartsWith($repository + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Output and backup folders must stay inside the BESS workspace.'
}
$cars = @(Get-ChildItem -LiteralPath (Join-Path $repository 'cars') -Filter '*.zip' -File | Sort-Object Name)
$previous = @(Get-ChildItem -LiteralPath $Mods -Filter 'bess-variant-*.zip' -File)
if ($cars.Count -ne 12 -or $previous.Count -ne 12) {
    throw "Expected 12 source ZIPs and 12 previous BESS add-ons; found $($cars.Count) and $($previous.Count)."
}
if (Test-Path -LiteralPath $Backup) { throw "Backup already exists: $Backup" }

$sourceHashes = @{}
$addOns = [Collections.Generic.List[object]]::new()
foreach ($car in $cars) {
    $installed = Join-Path $Mods $car.Name
    if (-not (Test-Path -LiteralPath $installed -PathType Leaf)) { throw "Missing active original: $installed" }
    $sourceHash = (Get-FileHash -LiteralPath $car.FullName -Algorithm SHA256).Hash
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne $sourceHash) {
        throw "Original vehicle differs from local source: $($car.Name)"
    }
    $sourceHashes[$car.Name] = $sourceHash
    $stem = [IO.Path]::GetFileNameWithoutExtension($car.Name) -replace '^bunchyearth23_', ''
    foreach ($profile in @('natural', 'smooth', 'raw')) {
        $directory = Join-Path $OutputRoot "$stem-$profile"
        $zip = @(Get-ChildItem -LiteralPath $directory -Filter 'bess-variant-*.zip' -File)
        if ($zip.Count -ne 1) { throw "Expected one generated add-on in $directory" }
        $addOns.Add($zip[0])
    }
}
if ($addOns.Count -ne 36 -or @($addOns.Name | Select-Object -Unique).Count -ne 36) {
    throw 'Generated add-on names are incomplete or duplicated.'
}

New-Item -ItemType Directory -Path $Backup | Out-Null
foreach ($old in $previous) {
    if (-not $old.FullName.StartsWith($modsRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Add-on path escapes the configured mods directory: $($old.FullName)"
    }
    Move-Item -LiteralPath $old.FullName -Destination $Backup
}
foreach ($zip in $addOns) {
    Copy-Item -LiteralPath $zip.FullName -Destination $Mods
}
foreach ($zip in $addOns) {
    $installed = Join-Path $Mods $zip.Name
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne
        (Get-FileHash -LiteralPath $zip.FullName -Algorithm SHA256).Hash) {
        throw "Installed add-on hash mismatch: $($zip.Name)"
    }
}
foreach ($car in $cars) {
    $installed = Join-Path $Mods $car.Name
    if ((Get-FileHash -LiteralPath $installed -Algorithm SHA256).Hash -ne $sourceHashes[$car.Name]) {
        throw "Original vehicle changed during installation: $($car.Name)"
    }
}
$active = @(Get-ChildItem -LiteralPath $Mods -Filter '*.zip' -File)
$newCount = @(Get-ChildItem -LiteralPath $Mods -Filter 'bess-variant-*.zip' -File).Count
if ($active.Count -ne 48 -or $newCount -ne 36) {
    throw "Unexpected active mod count after install: $($active.Count) ZIPs, $newCount BESS add-ons"
}
Write-Output "Installed 36 profiles beside 12 unchanged originals. Backed up 12 old add-ons to $Backup"

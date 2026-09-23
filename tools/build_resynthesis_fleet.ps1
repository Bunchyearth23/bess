param(
    [string]$OutputRoot = (Join-Path (Split-Path $PSScriptRoot -Parent) 'output\resynthesis-profiles-20260923'),
    [ValidateRange(1, 8)][int]$Parallel = 4
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $repository 'target\release\BESS.exe'
$cars = Join-Path $repository 'cars'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw 'Build BESS.exe first.' }
if (-not (Test-Path -LiteralPath $cars -PathType Container)) { throw 'The local cars folder is missing.' }
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null

$profiles = @(
    @{ Name = 'Natural'; Preset = 0 },
    @{ Name = 'Smooth'; Preset = 1 },
    @{ Name = 'Raw'; Preset = 2 }
)
$vehicles = @(Get-ChildItem -LiteralPath $cars -Filter '*.zip' -File | Sort-Object Name)
if ($vehicles.Count -ne 12) { throw "Expected 12 local vehicle ZIPs; found $($vehicles.Count)." }
$queue = [Collections.Generic.Queue[object]]::new()
foreach ($vehicle in $vehicles) {
    foreach ($profile in $profiles) {
        $stem = [IO.Path]::GetFileNameWithoutExtension($vehicle.Name) -replace '^bunchyearth23_', ''
        $directory = Join-Path $OutputRoot "$stem-$($profile.Name.ToLowerInvariant())"
        if (Test-Path -LiteralPath $directory) {
            $zip = @(Get-ChildItem -LiteralPath $directory -Filter 'bess-variant-*.zip' -File)
            if ($zip.Count -eq 1) {
                Write-Output "READY $stem $($profile.Name)"
                continue
            }
            throw "Incomplete output directory: $directory"
        }
        $queue.Enqueue([pscustomobject]@{
            Vehicle = $vehicle.FullName
            Profile = $profile.Name
            Preset = $profile.Preset
            Directory = $directory
        })
    }
}
$running = [Collections.Generic.List[object]]::new()
$failures = [Collections.Generic.List[string]]::new()
while ($queue.Count -gt 0 -or $running.Count -gt 0) {
    while ($queue.Count -gt 0 -and $running.Count -lt $Parallel) {
        $item = $queue.Dequeue()
        $process = Start-Process -FilePath $exe -ArgumentList @('--beamng-profile', $item.Vehicle, $item.Directory, $item.Profile, [string]$item.Preset) -PassThru -WindowStyle Hidden
        $running.Add([pscustomobject]@{ Item = $item; Process = $process })
        Write-Output "START $([IO.Path]::GetFileNameWithoutExtension($item.Vehicle)) $($item.Profile)"
    }
    Start-Sleep -Seconds 2
    for ($i = $running.Count - 1; $i -ge 0; $i--) {
        $task = $running[$i]
        if (-not $task.Process.HasExited) { continue }
        $task.Process.Refresh()
        $zip = @(Get-ChildItem -LiteralPath $task.Item.Directory -Filter 'bess-variant-*.zip' -File -ErrorAction SilentlyContinue)
        if ($task.Process.ExitCode -ne 0 -or $zip.Count -ne 1) {
            $failure = "$($task.Item.Vehicle) $($task.Item.Profile): exit $($task.Process.ExitCode)"
            $errorFile = Join-Path $task.Item.Directory 'error.txt'
            if (Test-Path -LiteralPath $errorFile) { $failure += ": $(Get-Content -LiteralPath $errorFile -Raw)" }
            $failures.Add($failure)
            Write-Output "FAIL $failure"
        } else {
            Write-Output "DONE $($task.Item.Profile) $($zip[0].Name)"
        }
        $running.RemoveAt($i)
    }
}
if ($failures.Count) { throw ($failures -join [Environment]::NewLine) }
$total = 0
foreach ($vehicle in $vehicles) {
    foreach ($profile in $profiles) {
        $stem = [IO.Path]::GetFileNameWithoutExtension($vehicle.Name) -replace '^bunchyearth23_', ''
        $directory = Join-Path $OutputRoot "$stem-$($profile.Name.ToLowerInvariant())"
        $total += @(Get-ChildItem -LiteralPath $directory -Filter 'bess-variant-*.zip' -File).Count
    }
}
if ($total -ne 36) { throw "Expected 36 generated add-ons; found $total." }
Write-Output "COMPLETE $total profiles"

param([int]$Minutes = 10, [string]$ProcessFile = "processes.json", [string]$ReportFile = "performance.json")
$ErrorActionPreference = "Stop"
$workspace = (Resolve-Path "$PSScriptRoot/../..").Path
$run = Get-Content "$workspace/output/acceptance/$ProcessFile" -Raw | ConvertFrom-Json
$samples = @()
for ($i=0; $i -le $Minutes*4; $i++) {
    $all = @(Get-CimInstance Win32_Process)
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add([int]$run.app)
    do {
        $before=$ids.Count
        foreach($entry in $all) { if($ids.Contains([int]$entry.ParentProcessId)) { [void]$ids.Add([int]$entry.ProcessId) } }
    } while($ids.Count -gt $before)
    $processes = @($ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
    $samples += [PSCustomObject]@{ elapsed_seconds=$i*15; timestamp=(Get-Date).ToString("o"); process_count=$processes.Count; working_set_bytes=($processes|Measure-Object WorkingSet64 -Sum).Sum; private_bytes=($processes|Measure-Object PrivateMemorySize64 -Sum).Sum; cpu_seconds=($processes|Measure-Object CPU -Sum).Sum; responding=(Get-Process -Id $run.app -ErrorAction SilentlyContinue).Responding }
    $samples | ConvertTo-Json | Set-Content "$workspace/output/acceptance/$ReportFile" -Encoding UTF8
    if($i -lt $Minutes*4) { Start-Sleep -Seconds 15 }
}
Write-Output "Saved $($samples.Count) process-tree samples"

$ErrorActionPreference = "Stop"
if ($env:USERNAME -ne "WDAGUtilityAccount") { throw "Run only inside Windows Sandbox" }
$installer = @(Get-ChildItem -LiteralPath "C:\Package" -Filter "*-setup.exe")[0].FullName
$installDir = "C:\CCUsageAcceptance"
$dataDir = Join-Path $env:APPDATA "dev.ningz.cc-usage"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$report = [ordered]@{ environment="Windows Sandbox"; install=$false; upgrade=$false; uninstall=$false }
try {
    $process = Start-Process $installer -ArgumentList @("/S", "/D=$installDir") -WindowStyle Hidden -Wait -PassThru
    if($process.ExitCode -ne 0 -or !(Test-Path "$installDir/cc-usage.exe")) { throw "Install failed" }
    $report.install=$true
    New-Item -ItemType Directory -Force -Path $dataDir | Out-Null
    Set-Content "$dataDir/acceptance-retention.txt" "preserve" -Encoding UTF8
    New-Item -Path $runKey -Force | Out-Null
    New-ItemProperty -Path $runKey -Name "CCUsage" -Value "$installDir/cc-usage.exe" -PropertyType String -Force | Out-Null
    $process = Start-Process $installer -ArgumentList @("/S", "/UPDATE", "/D=$installDir") -WindowStyle Hidden -Wait -PassThru
    if($process.ExitCode -ne 0 -or !(Test-Path "$dataDir/acceptance-retention.txt")) { throw "Upgrade lost data" }
    if(!(Get-ItemProperty -Path $runKey -Name "CCUsage" -ErrorAction SilentlyContinue)) { throw "Upgrade removed autostart preference" }
    $report.upgrade=$true
    $process = Start-Process "$installDir/uninstall.exe" -ArgumentList "/S" -WindowStyle Hidden -Wait -PassThru
    $deadline=(Get-Date).AddSeconds(30)
    while((Test-Path "$installDir/cc-usage.exe") -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 250 }
    if(Test-Path "$installDir/cc-usage.exe") { throw "Uninstall retained executable" }
    if(!(Test-Path "$dataDir/acceptance-retention.txt")) { throw "Default uninstall deleted user data" }
    if(Get-ItemProperty -Path $runKey -Name "CCUsage" -ErrorAction SilentlyContinue) { throw "Uninstall retained autostart" }
    $report.uninstall=$true
} catch { $report.error=$_.Exception.Message } finally {
    $report | ConvertTo-Json | Set-Content "C:\Results\sandbox-result.json" -Encoding UTF8
}

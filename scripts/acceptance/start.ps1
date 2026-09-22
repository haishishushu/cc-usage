$ErrorActionPreference = "Stop"
$workspace = (Resolve-Path "$PSScriptRoot/../..").Path
$profile = Join-Path $workspace ("output/acceptance/profile/" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $profile | Out-Null
New-Item -ItemType File -Force -Path (Join-Path $profile ".acceptance-profile") | Out-Null
& python "$PSScriptRoot/fixtures.py"
if ($LASTEXITCODE -ne 0) { throw "Fixture generation failed" }
$oldProfile = $env:CC_USAGE_TEST_DIR
$oldWebview = $env:WEBVIEW2_USER_DATA_FOLDER
$oldArguments = $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
try {
    $env:CC_USAGE_TEST_DIR = $profile
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $profile "webview"
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9337"
    $app = Start-Process (Join-Path $workspace "backend/target/debug/cc-usage.exe") -WorkingDirectory $workspace -WindowStyle Hidden -PassThru -RedirectStandardOutput "$workspace/output/acceptance/app.log" -RedirectStandardError "$workspace/output/acceptance/app-error.log"
    $gateway = Start-Process python -ArgumentList @("scripts/acceptance/gateway.py") -WorkingDirectory $workspace -WindowStyle Hidden -PassThru
    @{app=$app.Id;gateway=$gateway.Id;profile=$profile} | ConvertTo-Json | Set-Content "$workspace/output/acceptance/processes.json" -Encoding UTF8
    Write-Output "Isolated app PID: $($app.Id); gateway PID: $($gateway.Id)"
} finally {
    $env:CC_USAGE_TEST_DIR = $oldProfile
    $env:WEBVIEW2_USER_DATA_FOLDER = $oldWebview
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $oldArguments
}

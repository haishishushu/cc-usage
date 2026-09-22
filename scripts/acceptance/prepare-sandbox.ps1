$ErrorActionPreference = "Stop"
$workspace = (Resolve-Path "$PSScriptRoot/../..").Path
$result = Join-Path $workspace "output/acceptance/sandbox"
New-Item -ItemType Directory -Force -Path $result | Out-Null
$package = [Security.SecurityElement]::Escape((Join-Path $workspace "backend/target/release/bundle/nsis"))
$scripts = [Security.SecurityElement]::Escape($PSScriptRoot)
$escapedResult = [Security.SecurityElement]::Escape($result)
@"
<Configuration>
  <MappedFolders>
    <MappedFolder><HostFolder>$package</HostFolder><SandboxFolder>C:\Package</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$scripts</HostFolder><SandboxFolder>C:\Scripts</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$escapedResult</HostFolder><SandboxFolder>C:\Results</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\Scripts\sandbox.ps1</Command></LogonCommand>
</Configuration>
"@ | Set-Content (Join-Path $result "install-check.wsb") -Encoding UTF8
Write-Output (Join-Path $result "install-check.wsb")

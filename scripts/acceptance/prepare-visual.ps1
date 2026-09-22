$ErrorActionPreference = "Stop"
$workspace = (Resolve-Path "$PSScriptRoot/../..").Path
Copy-Item -LiteralPath "$PSScriptRoot/states.tsx" -Destination "$workspace/frontend/__acceptance.tsx"
'<div id="root"></div><script type="module" src="/__acceptance.tsx"></script>' | Set-Content "$workspace/frontend/__acceptance.html" -Encoding UTF8

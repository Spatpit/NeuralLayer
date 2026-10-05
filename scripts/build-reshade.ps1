param([string]$Checkout = 'artifacts/reshade-build-source')
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $root
try {
    $revision = '18deaa52de0c425a78b329e9cb3c497281cd00ec'
    if (!(Test-Path -LiteralPath $Checkout)) {
        git clone --depth 1 --branch v6.8.0 --recurse-submodules --shallow-submodules https://github.com/crosire/reshade.git $Checkout
        if ($LASTEXITCODE -ne 0) { throw 'ReShade source fetch failed.' }
    }
    if ((git -C $Checkout rev-parse HEAD) -ne $revision) { throw 'Unexpected ReShade revision.' }
    git -C $Checkout submodule update --init --recursive
    if ($LASTEXITCODE -ne 0) { throw 'ReShade submodule fetch failed.' }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if (!$vs) { throw 'Install Visual Studio C++ build tools.' }
    $msbuild = Join-Path $vs 'MSBuild/Current/Bin/MSBuild.exe'
    Push-Location -LiteralPath $Checkout
    try {
        & $msbuild ReShade.sln /t:ReShade /m /p:Configuration=Release '/p:Platform=64-bit' /p:WindowsTargetPlatformVersion=10.0 /v:minimal
        if ($LASTEXITCODE -ne 0) { throw 'ReShade compilation failed.' }
    } finally { Pop-Location }
    New-Item -ItemType Directory -Force -Path 'bundled-runtime' | Out-Null
    Copy-Item -LiteralPath (Join-Path $Checkout 'bin/x64/Release/ReShade64.dll') -Destination 'bundled-runtime/ReShade64.dll' -Force
    (Get-FileHash -LiteralPath 'bundled-runtime/ReShade64.dll' -Algorithm SHA256).Hash.ToLowerInvariant() | Set-Content -LiteralPath 'vendor/reshade/BUNDLED.sha256' -Encoding ascii
    Write-Output 'ReShade built. Rebuild the app so its bundled-runtime integrity pin matches this DLL.'
} finally { Pop-Location }

param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $projectRoot
try {
    if ((Get-Content -LiteralPath 'build-channel.txt' -Raw).Trim()) { throw 'Versioned packaging requires a blank build channel.' }
    $version = [regex]::Match((Get-Content -LiteralPath Cargo.toml -Raw), '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    $bundled = Join-Path $projectRoot 'bundled-runtime/ReShade64.dll'
    $pin = (Get-Content -LiteralPath vendor/reshade/BUNDLED.sha256 -Raw).Trim()
    if (!(Test-Path -LiteralPath $bundled) -or (Get-FileHash -LiteralPath $bundled).Hash -ine $pin) { throw 'Build the pinned ReShade runtime with scripts/build-reshade.ps1 first.' }
    # Build in a separate folder with machine-specific path prefixes remapped,
    # so the published binary does not contain the builder's user or project
    # paths. The encoded form keeps paths with spaces intact and must repeat
    # the flags from .cargo/config.toml, which it replaces.
    # When several prefixes match, rustc applies the last one given.
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME '.cargo' }
    $rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $HOME '.rustup' }
    $flags = @('-C', 'target-feature=+crt-static',
        "--remap-path-prefix=$([IO.Path]::GetFullPath($HOME))=home",
        "--remap-path-prefix=$([IO.Path]::GetFullPath($rustupHome))=rustup",
        "--remap-path-prefix=$([IO.Path]::GetFullPath($cargoHome))=cargo",
        "--remap-path-prefix=$([IO.Path]::GetFullPath($projectRoot))=neurallayer")
    $releaseDir = 'target/package/release'
    if (!$SkipBuild) {
        $previous = $env:CARGO_ENCODED_RUSTFLAGS
        $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]0x1f
        try {
            cargo build --locked --release --offline --target-dir target/package
            if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
        } finally { $env:CARGO_ENCODED_RUSTFLAGS = $previous }
    }
    $exe = Get-Item -LiteralPath "$releaseDir/neurallayer.exe"
    if ($exe.VersionInfo.ProductVersion -ne "v$version" -or $exe.VersionInfo.FileVersion -ne $version) { throw 'Executable does not match the release version.' }
    $distRoot = [IO.Path]::GetFullPath((Join-Path $projectRoot 'dist'))
    $stage = [IO.Path]::GetFullPath((Join-Path $distRoot "github-$version"))
    if (!$stage.StartsWith($distRoot + '\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Stage escaped dist.' }
    if (Test-Path -LiteralPath $stage) { throw "Staging folder already exists: $stage. Move it aside before packaging again." }
    New-Item -ItemType Directory -Force -Path $stage | Out-Null
    Copy-Item -LiteralPath $exe.FullName -Destination (Join-Path $stage 'NeuralLayer.exe')
    Copy-Item -LiteralPath "$releaseDir/nvngx.dll_dlssnr.dll" -Destination $stage
    # All documentation, licenses and checksums live in docs/.
    $docs = Join-Path $stage 'docs'
    $reshadeLicenses = Join-Path $docs 'licenses/reshade'
    New-Item -ItemType Directory -Force -Path $reshadeLicenses | Out-Null
    Copy-Item -LiteralPath 'README.md','CHANGELOG.md','LICENSE','LICENSE-MIT.txt','THIRD-PARTY-NOTICES.txt' -Destination $docs
    Copy-Item -LiteralPath 'vendor/reshade/LICENSE.md','vendor/reshade/PROVENANCE.md' -Destination $reshadeLicenses
    Copy-Item -LiteralPath 'vendor/reshade/runtime-notices' -Destination $reshadeLicenses -Recurse
    $runtime = Join-Path $stage 'runtime-optiscaler'
    New-Item -ItemType Directory -Force -Path (Join-Path $runtime 'shaders'),(Join-Path $runtime 'textures'),(Join-Path $runtime 'looks') | Out-Null
    Copy-Item -LiteralPath $bundled -Destination $runtime
    Copy-Item -LiteralPath 'native/SpatpitClarity.fx' -Destination (Join-Path $runtime 'shaders')
    # Imported looks live in subfolders of shaders/ and textures/.
    @"
[GENERAL]
EffectSearchPaths=.\shaders\**
TextureSearchPaths=.\textures\**
PresetPath=.\SpatpitNeural.ini
PerformanceMode=0
SkipLoadingDisabledEffects=0
[OVERLAY]
TutorialProgress=4
[INPUT]
KeyOverlay=36,0,0,0
"@ | Set-Content -LiteralPath (Join-Path $runtime 'ReShade.ini') -Encoding ascii
    'Techniques=' | Set-Content -LiteralPath (Join-Path $runtime 'SpatpitNeural.ini') -Encoding ascii
    'Techniques=SpatpitClarity@SpatpitClarity.fx' | Set-Content -LiteralPath (Join-Path $runtime 'SpatpitPreset.ini') -Encoding ascii
    $source = Join-Path $stage 'Source'
    New-Item -ItemType Directory -Path $source | Out-Null
    # Explicit source roots: never include ignored runtime files, videos or test artifacts.
    foreach ($name in @('Cargo.toml','Cargo.lock','build.rs','build-channel.txt','.gitignore','.gitattributes','.cargo','README.md','CHANGELOG.md','LICENSE','LICENSE-MIT.txt','THIRD-PARTY-NOTICES.txt','src','native','assets','vendor','scripts','docs')) {
        Copy-Item -LiteralPath (Join-Path $projectRoot $name) -Destination $source -Recurse
    }
    $files = @(Get-ChildItem -LiteralPath $stage -Recurse -File)
    # Published files must not reveal the builder's account or folders.
    $private = @(
        [IO.Path]::GetFullPath($HOME).TrimEnd('\'),
        [IO.Path]::GetFullPath($projectRoot).TrimEnd('\')
    )
    foreach ($file in $files) {
        $bytes = [IO.File]::ReadAllBytes($file.FullName)
        $texts = @([Text.Encoding]::ASCII.GetString($bytes), [Text.Encoding]::Unicode.GetString($bytes))
        foreach ($needle in $private) {
            foreach ($text in $texts) {
                if ($text.IndexOf($needle, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                    throw "Private path '$needle' found in release file: $($file.FullName)"
                }
            }
        }
    }
    $forbidden = @($files | Where-Object { $_.Name -match '(?i)^(nvngx_dlssnr\.dll|(Spatpit|Silver)(OptiScaler|Effects|Overlay)\.ini)$|lumenite|luminate|komplex|realxiv|\.(mp4|rgba|log|pdb)$' })
    if ($forbidden.Count) { throw "Forbidden release files: $($forbidden.FullName -join ', ')" }
    $allowedBinaries = @('NeuralLayer.exe','nvngx.dll_dlssnr.dll','runtime-optiscaler/ReShade64.dll')
    foreach ($file in $files | Where-Object { $_.Extension -in @('.exe','.dll','.addon64') }) {
        $relative = $file.FullName.Substring($stage.Length + 1).Replace('\','/')
        if ($relative -notin $allowedBinaries) { throw "Unexpected release binary: $relative" }
    }
    $files | Sort-Object FullName | ForEach-Object {
        $relative = $_.FullName.Substring($stage.Length + 1).Replace('\','/')
        "$((Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant())  $relative"
    } | Set-Content -LiteralPath (Join-Path $docs 'SHA256SUMS.txt') -Encoding ascii
    $zip = Join-Path $distRoot "NeuralLayer-$version-portable-win64.zip"
    Compress-Archive -LiteralPath @(Get-ChildItem -LiteralPath $stage -Force | ForEach-Object FullName) -DestinationPath $zip
    "$((Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($zip))" | Set-Content -LiteralPath ($zip + '.sha256') -Encoding ascii
    Write-Output "GitHub release ready: $zip"
} finally { Pop-Location }

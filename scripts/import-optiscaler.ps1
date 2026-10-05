param([Parameter(Mandatory=$true)][string]$Source, [string]$AppFolder = (Split-Path -Parent $PSScriptRoot))
$ErrorActionPreference = 'Stop'
$inputFile = if (Test-Path -LiteralPath $Source -PathType Container) { Join-Path $Source 'nvngx_dlssnr.dll' } else { $Source }
$inputFile = (Resolve-Path -LiteralPath $inputFile).Path
if ([IO.Path]::GetFileName($inputFile) -ine 'nvngx_dlssnr.dll') { throw 'Select nvngx_dlssnr.dll or its containing folder.' }
$runtime = Join-Path ([IO.Path]::GetFullPath($AppFolder)) 'runtime-optiscaler'
$destination = Join-Path $runtime 'nvngx_dlssnr.dll'
if (Test-Path -LiteralPath $destination) { throw 'A runtime is already installed. Close the app and move the old DLL aside before replacing it.' }
$inputStream = [IO.File]::OpenRead($inputFile)
try {
    $reader = [IO.BinaryReader]::new($inputStream)
    if ($reader.ReadUInt16() -ne 0x5a4d) { throw 'Not a Windows DLL.' }
    $inputStream.Position = 60
    $offset = $reader.ReadUInt32()
    if ($offset -lt 64 -or $offset + 26 -gt $inputStream.Length) { throw 'Invalid DLL header.' }
    $inputStream.Position = $offset
    if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw 'Select a 64-bit Windows DLL.' }
    $inputStream.Position = $offset + 22
    if (($reader.ReadUInt16() -band 0x2000) -eq 0 -or $reader.ReadUInt16() -ne 0x20b) { throw 'Select a 64-bit Windows DLL.' }
    $inputStream.Position = 0
    New-Item -ItemType Directory -Force -Path $runtime | Out-Null
    $temporary = Join-Path $runtime 'nvngx_dlssnr.importing'
    $outputStream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
    try { $inputStream.CopyTo($outputStream); $outputStream.Flush($true) }
    catch { $outputStream.Dispose(); Remove-Item -LiteralPath $temporary -Force; throw }
    finally { $outputStream.Dispose() }
    try { [IO.File]::Move($temporary, $destination) }
    catch { Remove-Item -LiteralPath $temporary -Force; throw }
} finally { $inputStream.Dispose() }
Write-Output "Neural DLL imported into $runtime. No shaders, presets, or other add-ons were copied."

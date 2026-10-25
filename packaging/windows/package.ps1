param(
    [string]$BinaryPath = '',
    [string]$OutputDirectory = ''
)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$manifest = [IO.File]::ReadAllText((Join-Path $projectRoot 'Cargo.toml'), [Text.Encoding]::UTF8)
$versionMatch = [regex]::Match($manifest, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"\s*$')
if (-not $versionMatch.Success) { throw 'Cargo.toml must contain a stable semantic version' }
$version = $versionMatch.Groups[1].Value
if ($env:GITHUB_REF_TYPE -eq 'tag' -and $env:GITHUB_REF_NAME -ne "v$version") {
    throw "The release tag must match Cargo.toml: v$version"
}
if (-not $BinaryPath) { $BinaryPath = Join-Path $projectRoot 'target/x86_64-pc-windows-msvc/release/zendaya-cursor.exe' }
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot 'dist' }
$bytes = [IO.File]::ReadAllBytes($BinaryPath)
if ($bytes.Length -lt 4096 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Missing or invalid Windows executable' }
$pe = [BitConverter]::ToInt32($bytes, 0x3c)
if ($pe -lt 0 -or $pe -gt $bytes.Length - 96 -or [BitConverter]::ToUInt32($bytes, $pe) -ne 0x00004550) { throw 'Invalid PE header' }
if ([BitConverter]::ToUInt16($bytes, $pe + 4) -ne 0x8664) { throw 'The executable must be Windows x64' }
if ([BitConverter]::ToUInt16($bytes, $pe + 24) -ne 0x20b) { throw 'The executable must use PE32+' }
if ([BitConverter]::ToUInt16($bytes, $pe + 24 + 68) -ne 2) { throw 'Build in release mode to produce a GUI executable without a console' }
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$stem = "zendaya-cursor-$version-windows-x64"
$exe = Join-Path $OutputDirectory "$stem.exe"
Copy-Item -LiteralPath $BinaryPath -Destination $exe
# Stage in a unique temporary directory; never remove a pre-existing project directory.
$stage = Join-Path ([IO.Path]::GetTempPath()) ('zendaya-cursor-package-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $stage
Copy-Item -LiteralPath $BinaryPath -Destination (Join-Path $stage 'zendaya-cursor.exe')
Copy-Item -LiteralPath (Join-Path $projectRoot 'README.md'), (Join-Path $projectRoot 'LICENSE') -Destination $stage
$zip = Join-Path $OutputDirectory "$stem-portable.zip"
Compress-Archive -LiteralPath (Join-Path $stage 'zendaya-cursor.exe'), (Join-Path $stage 'README.md'), (Join-Path $stage 'LICENSE') -DestinationPath $zip -Force
$checksums = @($exe, $zip) | ForEach-Object {
    $hash = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $([IO.Path]::GetFileName($_))"
}
[IO.File]::WriteAllText((Join-Path $OutputDirectory 'SHA256SUMS.txt'), (($checksums -join "`n") + "`n"), [Text.UTF8Encoding]::new($false))
# Check the absolute temporary path before removing this run's three staging files.
$resolvedStage = [IO.Path]::GetFullPath($stage)
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
if (-not $resolvedStage.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedStage) -notlike 'zendaya-cursor-package-*') { throw 'Unexpected staging directory' }
foreach ($name in @('zendaya-cursor.exe', 'README.md', 'LICENSE')) { Remove-Item -LiteralPath (Join-Path $resolvedStage $name) }
Remove-Item -LiteralPath $resolvedStage
if ($env:GITHUB_OUTPUT) { [IO.File]::AppendAllText($env:GITHUB_OUTPUT, "version=$version`n", [Text.UTF8Encoding]::new($false)) }
Write-Output "Packaged Zendaya Cursor ${version}: $exe and $zip"

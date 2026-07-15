# Package pure-study for Windows: self-contained app folders per architecture
# (arm64 / x64 / x86), each with the data pack next to the exe so it runs out
# of the box (core::home resolves <exe dir>/data). Produces dist/*.zip ready
# to attach to a GitHub Release.
#
#   pwsh scripts/package-windows.ps1 [-Arch arm64,x64,x86] [-Version v0.2.0]
#
# Works on an ARM64 or x64 dev box and on windows-latest CI — the MSVC Build
# Tools cross-target every listed arch from either host.

param(
    [string[]]$Arch = @("arm64", "x64", "x86"),
    [string]$Version = "dev"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path $PSScriptRoot -Parent
$dist = Join-Path $repo "dist"
New-Item -ItemType Directory -Force $dist | Out-Null

$rustTarget = @{
    arm64 = "aarch64-pc-windows-msvc"
    x64   = "x86_64-pc-windows-msvc"
    x86   = "i686-pc-windows-msvc"
}

foreach ($a in $Arch) {
    $rt = $rustTarget[$a]
    if (-not $rt) { throw "unknown arch: $a" }

    Write-Host "== $a ($rt) =="

    # The engine DLL. On the matching host a plain build also works, but an
    # explicit target keeps the output path uniform for the csproj mapping.
    & cargo build -p pure-ffi --release --target $rt
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $rt" }
    # The csproj maps win-arm64 -> target/release on the arm64 dev box; give
    # it the uniform per-target path too by copying when they differ.
    if ($a -eq "arm64") {
        New-Item -ItemType Directory -Force (Join-Path $repo "target\release") | Out-Null
        Copy-Item (Join-Path $repo "target\$rt\release\pure_ffi.dll") `
                  (Join-Path $repo "target\release\pure_ffi.dll") -Force
    }

    & dotnet publish (Join-Path $repo "apps\windows\PureStudyWin") `
        -c Release -r "win-$a" --self-contained true -p:WindowsAppSDKSelfContained=true
    if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed for win-$a" }

    $publish = Join-Path $repo "apps\windows\PureStudyWin\bin\Release\net8.0-windows10.0.26100.0\win-$a\publish"
    if (-not (Test-Path (Join-Path $publish "PureStudyWin.exe"))) {
        throw "publish output missing for win-$a"
    }

    # Stage: app + the FOSS data pack + authored weaves + docs.
    $stage = Join-Path $dist "pure-study-$Version-win-$a"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Force $stage | Out-Null
    Copy-Item "$publish\*" $stage -Recurse
    Copy-Item (Join-Path $repo "data") (Join-Path $stage "data") -Recurse
    if (Test-Path (Join-Path $repo "weaves")) {
        Copy-Item (Join-Path $repo "weaves") (Join-Path $stage "weaves") -Recurse
    }
    Copy-Item (Join-Path $repo "LICENSE") $stage
    Copy-Item (Join-Path $repo "BIBLIOGRAPHY.md") $stage
    Set-Content (Join-Path $stage "README.txt") @"
pure study $Version (Windows $a)

Run PureStudyWin.exe — no install needed. The 1769 KJV corpus, Strong's
dictionary and study artifacts are in data\ next to the exe. Your own
threads, tags and weave links are stored per-user under %APPDATA%.

Double-click a word for its Strong's study; Ctrl+F to search; the mode
button switches between Simple reader and Full study.
"@

    $zip = Join-Path $dist "pure-study-$Version-win-$a.zip"
    if (Test-Path $zip) { Remove-Item -Force $zip }
    Compress-Archive -Path "$stage\*" -DestinationPath $zip
    Remove-Item -Recurse -Force $stage
    Write-Host "wrote $zip"
}

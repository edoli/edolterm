param([string]$OutputDir = 'target/release', [ValidateSet('release', 'release-dist')][string]$Profile = 'release')
$ErrorActionPreference = 'Stop'
function Copy-BundleFile([string]$Source, [string]$Destination) {
    if ((Test-Path -LiteralPath $Destination -PathType Leaf) -and
        (Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash -eq
        (Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash) {
        return
    }
    Copy-Item -LiteralPath $Source -Destination $Destination -Force
}
$projectDir = Split-Path -Parent $PSScriptRoot
$packageVersion = '1.25.260930003'
$expectedHash = '02B07B349AF66D801159BDF9E440D4A1CE78BB951F37FC8609731665AFDAE7EE'
$cacheDir = Join-Path $projectDir 'target/conpty-comparison'
$archive = Join-Path $cacheDir "conpty-$packageVersion.zip"
$packageDir = Join-Path $cacheDir 'package'
[IO.Directory]::CreateDirectory($cacheDir) | Out-Null
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest "https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/$packageVersion/microsoft.windows.console.conpty.$packageVersion.nupkg" -OutFile $archive
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expectedHash) { throw 'ConPTY package hash mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath $packageDir -Force
$destination = Join-Path $projectDir $OutputDir
[IO.Directory]::CreateDirectory($destination) | Out-Null
$sourceExe = Join-Path $projectDir "target/$Profile/edolterm.exe"
$destinationExe = Join-Path $destination 'edolterm.exe'
if (-not (Test-Path -LiteralPath $sourceExe)) { throw 'Build edolterm with cargo build --release first' }
if ([IO.Path]::GetFullPath($sourceExe) -ne [IO.Path]::GetFullPath($destinationExe)) {
    Copy-BundleFile $sourceExe $destinationExe
}
Copy-BundleFile (Join-Path $packageDir 'runtimes/win-x64/native/conpty.dll') (Join-Path $destination 'conpty.dll')
# Alacritty 0.26 uses the compatibility CreatePseudoConsole exports, which
# resolve OpenConsole.exe next to conpty.dll (verified in the integration runs).
Copy-BundleFile (Join-Path $packageDir 'build/native/runtimes/x64/OpenConsole.exe') (Join-Path $destination 'OpenConsole.exe')
Copy-BundleFile (Join-Path $PSScriptRoot 'explorer-context-menu.ps1') (Join-Path $destination 'explorer-context-menu.ps1')
Invoke-WebRequest 'https://raw.githubusercontent.com/microsoft/terminal/v1.25.2733.0/LICENSE' -OutFile (Join-Path $destination 'LICENSE-ConPTY.txt')
$manifest = [ordered]@{
    package = 'Microsoft.Windows.Console.ConPTY'
    version = $packageVersion
    packageSha256 = $expectedHash
    source = "https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY/$packageVersion"
    files = @(Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $destination 'edolterm.exe'),(Join-Path $destination 'conpty.dll'),(Join-Path $destination 'OpenConsole.exe') | Select-Object @{n='name';e={Split-Path -Leaf $_.Path}},Hash)
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $destination 'runtime.json') -Encoding utf8
Write-Output "Bundle: $destination"

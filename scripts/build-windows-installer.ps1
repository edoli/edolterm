param(
    [string]$OutputDir = 'dist/windows',
    [ValidateSet('release', 'release-dist')][string]$Profile = 'release-dist',
    [string]$Repository = 'edoli/edolterm',
    [string]$SigningPfxPath,
    [string]$AppIdentity,
    [string]$AppDisplayName,
    [string]$CertificateThumbprint,
    [string]$InnoCompiler,
    [switch]$SkipAppBuild,
    [switch]$SkipInstaller
)
$ErrorActionPreference = 'Stop'
$projectDir = Split-Path -Parent $PSScriptRoot
$version = (& python (Join-Path $PSScriptRoot 'release-metadata.py') version)
if ($LASTEXITCODE -ne 0) { throw 'Invalid Cargo version or release tag.' }
$env:EDOLTERM_RELEASE_REPOSITORY = $Repository
$output = [IO.Path]::GetFullPath((Join-Path $projectDir $OutputDir))
$payload = Join-Path $output 'payload'
$shellPayload = Join-Path $payload 'shell-integration'
$work = Join-Path $projectDir 'target/windows-package'
$packageSource = Join-Path $work 'msix'
$source = Join-Path $projectDir 'packaging/windows'
foreach ($dir in @($payload, $shellPayload, $work, $packageSource, (Join-Path $packageSource 'Assets'))) {
    [IO.Directory]::CreateDirectory($dir) | Out-Null
}
function Invoke-Native([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}
if (-not $SkipAppBuild) {
    Push-Location -LiteralPath $projectDir
    try { Invoke-Native 'cargo' @('build', '--locked', '--profile', $Profile) }
    finally { Pop-Location }
}
& (Join-Path $PSScriptRoot 'bundle-windows.ps1') -OutputDir $OutputDir\payload -Profile $Profile
& (Join-Path $PSScriptRoot 'build-openconsole.ps1') -OutputDirectory (Join-Path $OutputDir 'payload/shell-integration')
Invoke-Native 'cmake' @('-S', $source, '-B', (Join-Path $work 'native'), '-A', 'x64', '-DBUILD_TESTING=OFF')
Invoke-Native 'cmake' @('--build', (Join-Path $work 'native'), '--config', 'Release')
Copy-Item -LiteralPath (Join-Path $work 'native/Release/Edolterm.ExplorerCommand.dll') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $work 'native/Release/Edolterm.ExplorerHost.exe') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $work 'native/Release/Edolterm.TerminalHost.dll') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $work 'native/Release/Edolterm.HandoffProxy.dll') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $source 'LICENSE-MicrosoftTerminal.txt') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $source 'NOTICE-OpenConsole.txt') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'windows-shell-integration.ps1') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'windows-default-terminal.ps1') -Destination $shellPayload -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'windows-terminal-support.ps1') -Destination $shellPayload -Force

$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$sdk = Get-ChildItem -LiteralPath $sdkRoot -Directory |
    Where-Object { $_.Name -match '^10\.' -and (Test-Path -LiteralPath (Join-Path $_.FullName 'x64/makeappx.exe')) } |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $sdk) { throw 'Windows SDK MakeAppx, MakePri, SignTool, and mt.exe are required.' }
$sdkBin = Join-Path $sdk.FullName 'x64'

$exeVersion = (Get-Item -LiteralPath (Join-Path $payload 'edolterm.exe')).VersionInfo.ProductVersion
if ($exeVersion -ne $version) { throw "Executable version $exeVersion does not match Cargo version $version" }
# Keep the private key in the current user's certificate store, never in the payload.
$signingDir = Join-Path $projectDir 'target/signing'
[IO.Directory]::CreateDirectory($signingDir) | Out-Null
$thumbFile = Join-Path $signingDir 'development-thumbprint.txt'
if (-not $CertificateThumbprint -and (Test-Path -LiteralPath $thumbFile)) {
    $CertificateThumbprint = ([IO.File]::ReadAllText($thumbFile)).Trim()
}
if ($SigningPfxPath) {
    if (-not $env:EDOLTERM_SIGNING_PASSWORD) { throw 'EDOLTERM_SIGNING_PASSWORD is required with -SigningPfxPath.' }
    $password = ConvertTo-SecureString $env:EDOLTERM_SIGNING_PASSWORD -AsPlainText -Force
    $imported = Import-PfxCertificate -FilePath $SigningPfxPath -Password $password -CertStoreLocation 'Cert:/CurrentUser/My'
    $CertificateThumbprint = $imported.Thumbprint
}
$certificate = $null
if ($CertificateThumbprint) {
    $certificate = Get-Item -LiteralPath "Cert:/CurrentUser/My/$CertificateThumbprint"
} else {
    $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=Edolterm Development' `
        -FriendlyName 'Edolterm development package signing' -CertStoreLocation 'Cert:/CurrentUser/My' `
        -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy NonExportable `
        -NotAfter (Get-Date).AddYears(3)
    [IO.File]::WriteAllText($thumbFile, $certificate.Thumbprint)
}
if (-not $certificate.HasPrivateKey -or $certificate.Subject -cne 'CN=Edolterm Development' -or $certificate.NotAfter -le (Get-Date)) {
    throw 'An unexpired Edolterm Development code-signing certificate with its private key is required.'
}
Export-Certificate -Cert $certificate -FilePath (Join-Path $shellPayload 'Edolterm.Development.cer') -Force | Out-Null

# Only the shell integration endpoint has package identity. Keep the terminal's
# native GPUI manifest intact so COM activation cannot terminate its process.
$app = Join-Path $payload 'edolterm.exe'
$appManifest = Join-Path $work 'edolterm.manifest'
Invoke-Native (Join-Path $sdkBin 'mt.exe') @('-nologo', "-inputresource:$app;#1", "-out:$appManifest")
[xml]$nativeManifest = Get-Content -LiteralPath $appManifest -Raw
if ($nativeManifest.GetElementsByTagName('msix', 'urn:schemas-microsoft-com:msix.v1').Count) {
    throw 'edolterm.exe must not contain Explorer package identity. Rebuild the application.'
}
$shellHost = Join-Path $shellPayload 'Edolterm.ExplorerHost.exe'
Invoke-Native (Join-Path $sdkBin 'mt.exe') @('-nologo', "-inputresource:$shellHost;#1", '-manifest', (Join-Path $source 'Identity.manifest'), "-outputresource:$shellHost;#1")
$consoleHost = Join-Path $shellPayload 'Edolterm.OpenConsole.exe'
Invoke-Native (Join-Path $sdkBin 'mt.exe') @('-nologo', "-inputresource:$consoleHost;#1", '-manifest', (Join-Path $source 'Identity.manifest'), "-outputresource:$consoleHost;#1")
$terminalHost = Join-Path $shellPayload 'Edolterm.TerminalHost.dll'
foreach ($file in @($app, $shellHost, $consoleHost, $terminalHost, (Join-Path $shellPayload 'Edolterm.HandoffProxy.dll'), (Join-Path $shellPayload 'Edolterm.ExplorerCommand.dll'))) {
    Invoke-Native (Join-Path $sdkBin 'signtool.exe') @('sign', '/fd', 'SHA256', '/s', 'My', '/sha1', $certificate.Thumbprint, $file)
}

[xml]$manifest = Get-Content -LiteralPath (Join-Path $source 'AppxManifest.xml') -Raw
$manifest.Package.Identity.Version = "$version.0"
$manifest.Save((Join-Path $packageSource 'AppxManifest.xml'))
& (Join-Path $PSScriptRoot 'windows-icon-assets.ps1') -Executable $app -OutputDirectory (Join-Path $packageSource 'Assets')
# The shell selects targetsize/altform variants through the package resource index.
$priConfig = Join-Path $work 'priconfig.xml'
Invoke-Native (Join-Path $sdkBin 'makepri.exe') @('createconfig', '/cf', $priConfig, '/dq', 'en-US', '/pv', '10.0.0', '/o')
Invoke-Native (Join-Path $sdkBin 'makepri.exe') @('new', '/pr', $packageSource, '/cf', $priConfig, '/of', (Join-Path $packageSource 'resources.pri'), '/in', $manifest.Package.Identity.Name, '/o')
# Sparse-package visual assets resolve relative to the external install location.
# Keeping them only inside the MSIX leaves the running app with a blank taskbar icon.
Copy-Item -LiteralPath (Join-Path $packageSource 'Assets') -Destination $shellPayload -Recurse -Force
Copy-Item -LiteralPath (Join-Path $packageSource 'resources.pri') -Destination $shellPayload -Force
$package = Join-Path $shellPayload 'Edolterm.Explorer.msix'
Invoke-Native (Join-Path $sdkBin 'makeappx.exe') @('pack', '/o', '/nv', '/d', $packageSource, '/p', $package)
Invoke-Native (Join-Path $sdkBin 'signtool.exe') @('sign', '/fd', 'SHA256', '/s', 'My', '/sha1', $certificate.Thumbprint, $package)
# bundle-windows records the unsigned executable hash; refresh it for the signed payload.
$runtimeFile = Join-Path $payload 'runtime.json'
$runtime = Get-Content -LiteralPath $runtimeFile -Raw -Encoding UTF8 | ConvertFrom-Json
$runtime.files = @(foreach ($name in @('edolterm.exe', 'conpty.dll', 'OpenConsole.exe')) {
    [pscustomobject]@{ name = $name; Hash = (Get-FileHash -LiteralPath (Join-Path $payload $name) -Algorithm SHA256).Hash }
})
$runtime | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $runtimeFile -Encoding UTF8
# Recompute hashes after signing the staged executable.
Get-ChildItem -LiteralPath $payload -File -Recurse | Where-Object Name -ne 'SHA256SUMS.txt' |
    Get-FileHash -Algorithm SHA256 | ForEach-Object { '{0}  {1}' -f $_.Hash, $_.Path.Substring($payload.Length + 1).Replace('\', '/') } |
    Set-Content -LiteralPath (Join-Path $payload 'SHA256SUMS.txt') -Encoding UTF8
if ($SkipInstaller) { Write-Output "Signed payload: $payload"; return }
if (-not $InnoCompiler) { $InnoCompiler = & (Join-Path $PSScriptRoot 'install-inno.ps1') }
if (-not (Test-Path -LiteralPath $InnoCompiler)) { throw 'Specified Inno compiler does not exist.' }
$innoArgs = @("/DAppVersion=$version", "/DPayloadDir=$payload", "/O$output")
if ($AppIdentity) { $innoArgs += "/DAppIdentity=$AppIdentity" }
if ($AppDisplayName) { $innoArgs += "/DAppDisplayName=$AppDisplayName" }
Invoke-Native $InnoCompiler ($innoArgs + @((Join-Path $source 'edolterm.iss')))
$installer = Join-Path $output 'edolterm-installer-exe.exe'
Invoke-Native (Join-Path $sdkBin 'signtool.exe') @('sign', '/fd', 'SHA256', '/s', 'My', '/sha1', $certificate.Thumbprint, $installer)
Write-Output "Installer: $installer"
Write-Output "Public certificate thumbprint: $($certificate.Thumbprint)"

$zip = Join-Path $output 'edolterm-windows.zip'
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip }
$portableFiles = @('edolterm.exe', 'conpty.dll', 'OpenConsole.exe', 'LICENSE-ConPTY.txt', 'runtime.json', 'explorer-context-menu.ps1')
Compress-Archive -LiteralPath @($portableFiles | ForEach-Object { Join-Path $payload $_ }) -DestinationPath $zip -CompressionLevel Optimal
Invoke-Native 'python' @((Join-Path $PSScriptRoot 'release-metadata.py'), 'record', $output)
Invoke-Native 'python' @((Join-Path $PSScriptRoot 'release-metadata.py'), 'check', $output)

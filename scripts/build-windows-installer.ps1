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
$work = Join-Path $projectDir 'target/windows-package'
$packageSource = Join-Path $work 'msix'
$source = Join-Path $projectDir 'packaging/windows'
foreach ($dir in @($payload, $work, $packageSource, (Join-Path $packageSource 'Assets'))) {
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
Invoke-Native 'cmake' @('-S', $source, '-B', (Join-Path $work 'native'), '-A', 'x64', '-DBUILD_TESTING=OFF')
Invoke-Native 'cmake' @('--build', (Join-Path $work 'native'), '--config', 'Release')
Copy-Item -LiteralPath (Join-Path $work 'native/Release/Edolterm.ExplorerCommand.dll') -Destination $payload -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'windows-shell-integration.ps1') -Destination $payload -Force

$sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$sdk = Get-ChildItem -LiteralPath $sdkRoot -Directory |
    Where-Object { $_.Name -match '^10\.' -and (Test-Path -LiteralPath (Join-Path $_.FullName 'x64/makeappx.exe')) } |
    Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
if (-not $sdk) { throw 'Windows SDK MakeAppx, SignTool, and mt.exe are required.' }
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
Export-Certificate -Cert $certificate -FilePath (Join-Path $payload 'Edolterm.Development.cer') -Force | Out-Null

# Merge package identity with the executable's existing GPUI application manifest.
$app = Join-Path $payload 'edolterm.exe'
Invoke-Native (Join-Path $sdkBin 'mt.exe') @('-nologo', "-inputresource:$app;#1", '-manifest', (Join-Path $source 'Identity.manifest'), "-outputresource:$app;#1")
foreach ($file in @($app, (Join-Path $payload 'Edolterm.ExplorerCommand.dll'))) {
    Invoke-Native (Join-Path $sdkBin 'signtool.exe') @('sign', '/fd', 'SHA256', '/s', 'My', '/sha1', $certificate.Thumbprint, $file)
}

[xml]$manifest = Get-Content -LiteralPath (Join-Path $source 'AppxManifest.xml') -Raw
$manifest.Package.Identity.Version = "$version.0"
$manifest.Save((Join-Path $packageSource 'AppxManifest.xml'))
Add-Type -AssemblyName System.Drawing
$icon = [Drawing.Icon]::ExtractAssociatedIcon($app)
try {
    $bitmap = $icon.ToBitmap()
    try {
        foreach ($size in @(44, 150, 256)) {
            $image = [Drawing.Bitmap]::new($bitmap, [Drawing.Size]::new($size, $size))
            try {
                $name = if ($size -eq 256) { 'Logo.png' } else { "Logo$size.png" }
                $image.Save((Join-Path $packageSource "Assets/$name"), [Drawing.Imaging.ImageFormat]::Png)
            }
            finally { $image.Dispose() }
        }
    }
    finally { $bitmap.Dispose() }
}
finally { $icon.Dispose() }
# Sparse-package visual assets resolve relative to the external install location.
# Keeping them only inside the MSIX leaves the running app with a blank taskbar icon.
Copy-Item -LiteralPath (Join-Path $packageSource 'Assets') -Destination $payload -Recurse -Force
$package = Join-Path $payload 'Edolterm.Explorer.msix'
Invoke-Native (Join-Path $sdkBin 'makeappx.exe') @('pack', '/o', '/nv', '/d', $packageSource, '/p', $package)
Invoke-Native (Join-Path $sdkBin 'signtool.exe') @('sign', '/fd', 'SHA256', '/s', 'My', '/sha1', $certificate.Thumbprint, $package)
# bundle-windows records the unsigned executable hash; refresh it for the signed payload.
$runtimeFile = Join-Path $payload 'runtime.json'
$runtime = Get-Content -LiteralPath $runtimeFile -Raw -Encoding UTF8 | ConvertFrom-Json
$runtime.files = @(foreach ($name in @('edolterm.exe', 'conpty.dll', 'OpenConsole.exe')) {
    [pscustomobject]@{ name = $name; Hash = (Get-FileHash -LiteralPath (Join-Path $payload $name) -Algorithm SHA256).Hash }
})
$runtime | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $runtimeFile -Encoding UTF8
# Recompute hashes after embedding identity and signing the staged executable.
Get-ChildItem -LiteralPath $payload -File | Where-Object Name -ne 'SHA256SUMS.txt' |
    Get-FileHash -Algorithm SHA256 | ForEach-Object { '{0}  {1}' -f $_.Hash, [IO.Path]::GetFileName($_.Path) } |
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

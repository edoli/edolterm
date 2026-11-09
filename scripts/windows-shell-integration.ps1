param(
    [ValidateSet('Install', 'Uninstall', 'TrustCertificate', 'RemoveCertificate')]
    [string]$Action = 'Install',
    [string]$AppDirectory = $PSScriptRoot,
    [ValidateSet('User', 'Machine')][string]$Scope = 'User'
)
$ErrorActionPreference = 'Stop'
# Inno can inherit PowerShell 7's PSModulePath from its launching process.
# Load Windows PowerShell's system modules rather than incompatible Core modules.
if ($PSEdition -eq 'Core') {
    $windowsPowerShell = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    & $windowsPowerShell -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $PSCommandPath -Action $Action -AppDirectory $AppDirectory -Scope $Scope
    exit $LASTEXITCODE
}
$env:PSModulePath = Join-Path $PSHOME 'Modules'
$AppDirectory = (Get-Item -LiteralPath $AppDirectory).FullName
$logDirectory = Join-Path $env:LOCALAPPDATA 'Edolterm/Logs'
[IO.Directory]::CreateDirectory($logDirectory) | Out-Null
Start-Transcript -Path (Join-Path $logDirectory "explorer-$Action.log") -Append | Out-Null
$packageName = 'Edoli.Edolterm.Explorer'
$certificateFile = Join-Path $AppDirectory 'Edolterm.Development.cer'
$packageFile = Join-Path $AppDirectory 'Edolterm.Explorer.msix'
function Is-Administrator {
    $principal = [Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())
    $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}
function Read-PublicCertificate {
    $certificate = [Security.Cryptography.X509Certificates.X509Certificate2]::new($certificateFile)
    $signature = Get-AuthenticodeSignature -LiteralPath $packageFile
    if ($certificate.Subject -cne 'CN=Edolterm Development' -or
        $certificate.HasPrivateKey -or -not $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint -cne $certificate.Thumbprint) {
        throw 'Package signer does not match the Edolterm public development certificate.'
    }
    return $certificate
}
if ($Action -eq 'TrustCertificate' -or $Action -eq 'RemoveCertificate') {
    if (-not (Is-Administrator)) { throw 'Certificate trust changes require administrator approval.' }
    $certificate = Read-PublicCertificate
    if ($Action -eq 'TrustCertificate') {
        Import-Certificate -FilePath $certificateFile -CertStoreLocation 'Cert:/LocalMachine/TrustedPeople' | Out-Null
        Write-Output 'Trusted the Edolterm development signing certificate.'
    } else {
        if (Get-AppxPackage -AllUsers -Name $packageName) { throw 'Uninstall Edolterm Explorer integration for all users before removing its certificate.' }
        $path = "Cert:/LocalMachine/TrustedPeople/$($certificate.Thumbprint)"
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path }
        Write-Output 'Removed the Edolterm development certificate trust.'
    }
    exit 0
}
$legacyScript = Join-Path $AppDirectory 'explorer-context-menu.ps1'
if ($Action -eq 'Uninstall') {
    if ([Environment]::OSVersion.Version.Build -ge 22000) {
        if ($Scope -eq 'Machine') {
            if (-not (Is-Administrator)) { throw 'All-users menu removal requires administrator approval.' }
            Get-AppxProvisionedPackage -Online | Where-Object DisplayName -eq $packageName |
                Remove-AppxProvisionedPackage -Online -AllUsers | Out-Null
            Get-AppxPackage -AllUsers -Name $packageName | Remove-AppxPackage -AllUsers
        } else {
            Get-AppxPackage -Name $packageName | Remove-AppxPackage
        }
    }
    if (Test-Path -LiteralPath $legacyScript) { & $legacyScript -Action Uninstall -Scope $Scope }
    exit 0
}
if ([Environment]::OSVersion.Version.Build -lt 22000) {
    # ConPTY supports Windows 10 1809; sparse identity is neither needed nor installed here.
    & $legacyScript -Action Install -Scope $Scope -ExecutablePath (Join-Path $AppDirectory 'edolterm.exe')
    exit 0
}
$certificate = Read-PublicCertificate
$trustPath = "Cert:/LocalMachine/TrustedPeople/$($certificate.Thumbprint)"
if (-not (Test-Path -LiteralPath $trustPath)) {
    # Only this helper elevates. Add-AppxPackage must run as the installing user.
    $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "{0}" -Action TrustCertificate -AppDirectory "{1}"' -f $PSCommandPath, $AppDirectory
    $process = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -Verb RunAs -WindowStyle Hidden -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $trustPath)) {
        throw 'Certificate trust was not approved. Explorer integration has not been installed.'
    }
}
# A rebuild with the same version must replace the earlier package registration.
if ($Scope -eq 'Machine') {
    if (-not (Is-Administrator)) { throw 'All-users menu registration requires administrator approval.' }
    Add-AppxPackage -Stage -Path $packageFile -ExternalLocation $AppDirectory -ErrorAction Stop
    Add-AppxProvisionedPackage -Online -PackagePath $packageFile -SkipLicense -ErrorAction Stop | Out-Null
    # Provisioning covers other users on sign-in; register immediately for the installing account.
    Add-AppxPackage -Path $packageFile -ExternalLocation $AppDirectory -ErrorAction Stop
} else {
    Get-AppxPackage -Name $packageName | Remove-AppxPackage
    Add-AppxPackage -Path $packageFile -ExternalLocation $AppDirectory -ErrorAction Stop
}
# Remove our legacy verbs only after the new command is successfully registered.
& $legacyScript -Action Uninstall -Scope $Scope
Write-Output 'Registered Edolterm in the Windows 11 Explorer menu for the current user.'
Stop-Transcript | Out-Null

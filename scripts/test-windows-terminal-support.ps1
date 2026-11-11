$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-terminal-support.ps1')
$cases = @{
    '10.0.17763.9999' = $false # Older Windows 10 retains ordinary tabs/registry menus.
    '10.0.19041.3031' = $false
    '10.0.19044.9999' = $false # 21H2 is not the documented desktop support target.
    '10.0.19045.0' = $false
    '10.0.19045.3030' = $false
    '10.0.19045.3031' = $true  # KB5026435 boundary.
    '10.0.19045.3032' = $true
    '10.0.19045.7000' = $true  # Superseding updates need not list the original KB.
    '10.0.22000.0' = $true    # Windows 11 21H2.
    '10.0.22621.0' = $true
    '10.0.26100.0' = $true
    '6.3.9600.0' = $false
}
foreach ($entry in $cases.GetEnumerator()) {
    if ((Test-EdoltermDefaultTerminalSupport ([version]$entry.Key)) -ne $entry.Value) {
        throw "Incorrect default-terminal support decision for $($entry.Key)"
    }
}
[xml]$manifest = Get-Content (Join-Path $PSScriptRoot '../packaging/windows/AppxManifest.xml') -Raw
if ([version]$manifest.Package.Dependencies.TargetDeviceFamily.MinVersion -gt [version]'10.0.19045.3031') {
    throw 'The sparse-package minimum OS blocks a supported Windows 10 release.'
}
Write-Output "PASS: $($cases.Count) Windows support boundaries and package minimum OS"
Write-Output "Host: $(Get-EdoltermWindowsVersion)"

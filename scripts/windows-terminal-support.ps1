# Default-terminal OS support documented by Microsoft:
# https://learn.microsoft.com/windows/terminal/install#set-your-default-terminal-application
function Get-EdoltermWindowsVersion {
    $version = [Environment]::OSVersion.Version
    # GetVersionEx reports a zero revision; UBR tracks superseding cumulative updates.
    $revision = [Microsoft.Win32.Registry]::GetValue(
        'HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion', 'UBR', 0)
    return [version]::new($version.Major, $version.Minor, $version.Build, [int]$revision)
}

function Test-EdoltermDefaultTerminalSupport([version]$Version) {
    return $Version.Major -eq 10 -and (
        $Version.Build -ge 22000 -or
        ($Version.Build -eq 19045 -and $Version.Revision -ge 3031))
}

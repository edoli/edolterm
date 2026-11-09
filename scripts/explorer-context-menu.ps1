param(
    [ValidateSet('Install', 'Uninstall')]
    [string]$Action = 'Install',
    [string]$ExecutablePath,
    [ValidateSet('User', 'Machine')][string]$Scope = 'User'
)
$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    throw 'Explorer context menus are supported on Windows only.'
}

$registry = if ($Scope -eq 'Machine') { [Microsoft.Win32.Registry]::LocalMachine } else { [Microsoft.Win32.Registry]::CurrentUser }
# Register only our own verbs in the selected installation scope.
$entries = @(
    @{ Path = 'Software\Classes\Directory\shell\Edolterm'; Argument = '%1' },
    @{ Path = 'Software\Classes\Directory\Background\shell\Edolterm'; Argument = '%V' },
    @{ Path = 'Software\Classes\Drive\shell\Edolterm'; Argument = '%1' }
)
if ($Action -eq 'Uninstall') {
    foreach ($entry in $entries) {
        $registry.DeleteSubKeyTree($entry.Path, $false)
    }
    Write-Output 'Removed Edolterm from the Explorer context menu for the current user.'
    return
}

if (-not $ExecutablePath) {
    $ExecutablePath = Join-Path $PSScriptRoot 'edolterm.exe'
    if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
        $ExecutablePath = Join-Path (Split-Path -Parent $PSScriptRoot) 'target/release/edolterm.exe'
    }
}
if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
    throw "Executable not found: $ExecutablePath. Build edolterm with cargo build --release first."
}
$ExecutablePath = (Get-Item -LiteralPath $ExecutablePath).FullName
if ([IO.Path]::GetExtension($ExecutablePath) -ine '.exe') {
    throw 'ExecutablePath must point to an .exe file.'
}
foreach ($entry in $entries) {
    $key = $registry.CreateSubKey($entry.Path)
    try {
        $key.SetValue('', 'Edolterm에서 열기', [Microsoft.Win32.RegistryValueKind]::String)
        $key.SetValue('Icon', ('"{0}",0' -f $ExecutablePath), [Microsoft.Win32.RegistryValueKind]::String)
        $key.SetValue('NoWorkingDirectory', '', [Microsoft.Win32.RegistryValueKind]::String)
        $key.SetValue('MultiSelectModel', 'Single', [Microsoft.Win32.RegistryValueKind]::String)
        $command = $key.CreateSubKey('command')
        try {
            # Append \. so quoted drive roots never end in a backslash before ".
            # Launch the executable directly, without a shell interpreting paths.
            $command.SetValue('', ('"{0}" --working-directory "{1}\."' -f $ExecutablePath, $entry.Argument), [Microsoft.Win32.RegistryValueKind]::String)
        }
        finally { $command.Dispose() }
    }
    finally { $key.Dispose() }
}
Write-Output "Registered Edolterm: $ExecutablePath"
Write-Output 'Right-click a folder, folder background, or drive. On Windows 11, choose Show more options.'

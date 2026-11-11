param(
    [ValidateSet('Set', 'Restore', 'Status')][string]$Action = 'Status'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'windows-terminal-support.ps1')
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$terminal = '{F36B1E31-6A3A-4BD1-92E5-801C26933378}'
$console = '{93C0413C-F8D7-49D7-8C07-62E4D1039ED4}'
$automatic = '{00000000-0000-0000-0000-000000000000}'
$startup = 'HKCU:\Console\%%Startup'
$backup = 'HKCU:\Software\Edoli\Edolterm\DefaultTerminalBackup'
function Read-Value([string]$Path, [string]$Name) {
    $value = Get-ItemPropertyValue -LiteralPath $Path -Name $Name -ErrorAction SilentlyContinue
    if ($null -eq $value) { return $automatic }
    return [string]$value
}
$currentTerminal = Read-Value $startup 'DelegationTerminal'
$currentConsole = Read-Value $startup 'DelegationConsole'
if ($Action -eq 'Status') {
    [pscustomobject]@{
        IsDefault = $currentTerminal -eq $terminal -and $currentConsole -eq $console
        Terminal = $currentTerminal
        Console = $currentConsole
    }
    return
}
if ($Action -eq 'Set') {
    $windowsVersion = Get-EdoltermWindowsVersion
    if (-not (Test-EdoltermDefaultTerminalSupport $windowsVersion)) {
        throw "기본 터미널 연동에는 Windows 11 또는 Windows 10 22H2 빌드 19045.3031 이상(KB5026435 이후 업데이트)이 필요합니다. 현재 버전: $windowsVersion"
    }
    # The signed package registers Edolterm's source-built OpenConsole and proxy.
    if ($PSEdition -eq 'Core') {
        & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $PSCommandPath -Action Set
        if ($LASTEXITCODE -ne 0) { throw '기본 터미널 등록에 실패했습니다.' }
        return
    }
    $env:PSModulePath = Join-Path $PSHOME 'Modules'
    $package = Get-AppxPackage -Name 'Edoli.Edolterm.ShellIntegration'
    if (-not $package) { throw 'Windows 연동 등록이 없습니다. Edolterm 설치 프로그램을 다시 실행하세요.' }
    $manifest = Get-AppxPackageManifest -Package $package.PackageFullName
    if (-not ($manifest.OuterXml -match 'F36B1E31-6A3A-4BD1-92E5-801C26933378') -or
        -not ($manifest.OuterXml -match '93C0413C-F8D7-49D7-8C07-62E4D1039ED4') -or
        -not ($manifest.GetElementsByTagName('ExeServer', 'http://schemas.microsoft.com/appx/manifest/com/windows10') | Where-Object Executable -eq 'Edolterm.OpenConsole.exe') -or
        -not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'Edolterm.OpenConsole.exe'))) {
        throw 'Edolterm Windows 연동을 최신 버전으로 다시 설치하세요.'
    }
    if ($currentTerminal -ne $terminal) {
        if (-not (Test-Path -LiteralPath $backup)) { New-Item -Path $backup -Force | Out-Null }
        New-ItemProperty -LiteralPath $backup -Name 'Terminal' -Value $currentTerminal -PropertyType String -Force | Out-Null
        New-ItemProperty -LiteralPath $backup -Name 'Console' -Value $currentConsole -PropertyType String -Force | Out-Null
    }
    if (-not (Test-Path -LiteralPath $startup)) { New-Item -Path $startup -Force | Out-Null }
    try {
        New-ItemProperty -LiteralPath $startup -Name 'DelegationConsole' -Value $console -PropertyType String -Force | Out-Null
        New-ItemProperty -LiteralPath $startup -Name 'DelegationTerminal' -Value $terminal -PropertyType String -Force | Out-Null
    } catch {
        New-ItemProperty -LiteralPath $startup -Name 'DelegationConsole' -Value $currentConsole -PropertyType String -Force | Out-Null
        New-ItemProperty -LiteralPath $startup -Name 'DelegationTerminal' -Value $currentTerminal -PropertyType String -Force | Out-Null
        throw
    }
    Write-Output '새로 실행하는 콘솔을 Edolterm에서 엽니다.'
} elseif ($currentTerminal -eq $terminal) {
    $previousTerminal = Read-Value $backup 'Terminal'
    $previousConsole = Read-Value $backup 'Console'
    New-ItemProperty -LiteralPath $startup -Name 'DelegationConsole' -Value $previousConsole -PropertyType String -Force | Out-Null
    New-ItemProperty -LiteralPath $startup -Name 'DelegationTerminal' -Value $previousTerminal -PropertyType String -Force | Out-Null
    Write-Output '이전 기본 터미널 설정을 복원했습니다.'
}

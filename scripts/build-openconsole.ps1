param(
    [string]$OutputDirectory = 'target/openconsole',
    # Tests use a unique class so their package cannot replace a real installation.
    [Guid]$ConsoleClassId = '93C0413C-F8D7-49D7-8C07-62E4D1039ED4'
)
$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$version = '1.25.2733.0'
$archiveHash = '1D2B9D701E66333D698412DBE4C8240E5239837B763E7DA79C8DEBE2B57CDAEA'
$cache = Join-Path $project 'target/openconsole-source'
$archive = Join-Path $cache "terminal-v$version.zip"
$source = Join-Path $cache "terminal-$version"
$destination = if ([IO.Path]::IsPathRooted($OutputDirectory)) { [IO.Path]::GetFullPath($OutputDirectory) } else { [IO.Path]::GetFullPath((Join-Path $project $OutputDirectory)) }
$class = $ConsoleClassId.ToString().ToUpperInvariant()
$scriptHash = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash
$key = "$archiveHash/$scriptHash/$class"
[IO.Directory]::CreateDirectory($cache) | Out-Null
[IO.Directory]::CreateDirectory($destination) | Out-Null
# All variants share the upstream libraries; serialize changes to its EXE header.
$lock = $null
$deadline = [DateTime]::UtcNow.AddMinutes(30)
while (-not $lock) {
    try { $lock = [IO.File]::Open((Join-Path $cache 'build.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
    catch [IO.IOException] {
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Timed out waiting for the OpenConsole build lock.' }
        Start-Sleep -Milliseconds 500
    }
}
try {
    $artifact = Join-Path $cache "$class.exe"
    $receipt = Join-Path $cache "$class.json"
    if ((Test-Path -LiteralPath $artifact) -and (Test-Path -LiteralPath $receipt)) {
        $record = Get-Content -LiteralPath $receipt -Raw | ConvertFrom-Json
        if ($record.key -eq $key -and $record.sha256 -eq (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash) {
            Copy-Item -LiteralPath $artifact -Destination (Join-Path $destination 'Edolterm.OpenConsole.exe') -Force
            Copy-Item -LiteralPath $receipt -Destination (Join-Path $destination 'openconsole-build.json') -Force
            return
        }
    }
    if (-not (Test-Path -LiteralPath $archive)) {
        Write-Host "Downloading OpenConsole source v$version..."
        Invoke-WebRequest "https://codeload.github.com/microsoft/terminal/zip/refs/tags/v$version" -OutFile $archive
    }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $archiveHash) { throw 'OpenConsole source SHA-256 mismatch.' }
    $ready = Join-Path $source '.edolterm-extracted'
    if (-not (Test-Path -LiteralPath $ready)) {
        Write-Host 'Extracting verified OpenConsole source...'
        Expand-Archive -LiteralPath $archive -DestinationPath $cache -Force
        [IO.File]::WriteAllText($ready, $archiveHash)
    }
    # Read the original header from the verified archive, even after a test build.
    # Give its COM server our own CLSID; the console implementation stays upstream.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($archive)
    try {
        $entry = $zip.GetEntry("terminal-$version/src/host/exe/CConsoleHandoff.h")
        $reader = [IO.StreamReader]::new($entry.Open())
        try { $header = $reader.ReadToEnd() } finally { $reader.Dispose() }
    } finally { $zip.Dispose() }
    $branding = '(?s)#if defined\(WT_BRANDING_RELEASE\).*?#endif'
    if ([regex]::Matches($header, $branding).Count -ne 1) { throw 'Upstream console CLSID declaration changed; review the source patch.' }
    $patched = [regex]::Replace($header, $branding,
        "// Edolterm modification: independent console handoff COM class.`n#define __CLSID_CConsoleHandoff `"$class`"")
    $headerPath = Join-Path $source 'src/host/exe/CConsoleHandoff.h'
    if ([IO.File]::ReadAllText($headerPath) -cne $patched) {
        [IO.File]::WriteAllText($headerPath, $patched, [Text.UTF8Encoding]::new($false))
    }
    # Upstream pins VS 2026 for dependencies; this build uses VS 2022 throughout.
    $triplet = Join-Path $source 'dep/vcpkg-overlay-triplets/x64-windows-static.cmake'
    $tripletText = [IO.File]::ReadAllText($triplet)
    if ($tripletText.Contains('VCPKG_PLATFORM_TOOLSET v145')) {
        [IO.File]::WriteAllText($triplet, $tripletText.Replace('VCPKG_PLATFORM_TOOLSET v145', 'VCPKG_PLATFORM_TOOLSET v143'))
    }
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    $msbuild = & $vswhere -latest -version '[17.0,18.0)' -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'MSBuild/**/Bin/MSBuild.exe' | Select-Object -First 1
    if (-not $msbuild) { throw 'Visual Studio 2022 C++ build tools and Windows SDK 10.0.26100 are required.' }
    $nugetLog = Join-Path $cache 'nuget.log'
    Write-Host 'Restoring pinned OpenConsole build dependencies...'
    & (Join-Path $source 'dep/nuget/nuget.exe') restore (Join-Path $source 'dep/nuget/packages.config') -ConfigFile (Join-Path $source 'NuGet.Config') -PackagesDirectory (Join-Path $source 'packages') -NonInteractive *> $nugetLog
    if ($LASTEXITCODE -ne 0) { Get-Content $nugetLog -Tail 30; throw "OpenConsole NuGet restore failed. See $nugetLog" }
    $buildLog = Join-Path $cache 'build.log'
    Write-Host "Building Edolterm OpenConsole ($class)..."
    & $msbuild (Join-Path $source 'src/host/exe/Host.EXE.vcxproj') /m:2 /v:minimal /nologo "/p:SolutionDir=$source\" /p:Configuration=Release /p:Platform=x64 /p:PlatformToolset=v143 /p:WindowsTerminalBranding=Dev /p:MultiProcessorCompilation=false /p:VcpkgAdditionalInstallOptions= *> $buildLog
    if ($LASTEXITCODE -ne 0) { Get-Content $buildLog -Tail 40; throw "OpenConsole build failed. See $buildLog" }
    Copy-Item -LiteralPath (Join-Path $source 'bin/x64/Release/OpenConsole.exe') -Destination $artifact -Force
    [ordered]@{
        key = $key
        upstream = "https://github.com/microsoft/terminal/tree/v$version"
        sourceSha256 = $archiveHash
        modification = 'CConsoleHandoff.h: Edolterm console handoff CLSID; x64 triplet: VS 2022 v143'
        consoleClassId = $class
        sha256 = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash
    } | ConvertTo-Json | Set-Content -LiteralPath $receipt -Encoding UTF8
    Copy-Item -LiteralPath $artifact -Destination (Join-Path $destination 'Edolterm.OpenConsole.exe') -Force
    Copy-Item -LiteralPath $receipt -Destination (Join-Path $destination 'openconsole-build.json') -Force
} finally { $lock.Dispose() }

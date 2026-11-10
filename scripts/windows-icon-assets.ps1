param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
if (-not ('EdoltermPackaging.Icons' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace EdoltermPackaging {
    public static class Icons {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
        [DllImport("kernel32.dll")]
        static extern bool FreeLibrary(IntPtr module);
        [DllImport("user32.dll", SetLastError = true)]
        static extern IntPtr LoadImageW(IntPtr module, IntPtr name, uint type, int width, int height, uint flags);
        [DllImport("user32.dll")]
        public static extern bool DestroyIcon(IntPtr icon);
        public static IntPtr Load(string path, int size) {
            // Load resources without executing the application or its dependencies.
            IntPtr module = LoadLibraryExW(path, IntPtr.Zero, 0x22);
            if (module == IntPtr.Zero) throw new Exception("LoadLibraryExW failed: " + Marshal.GetLastWin32Error());
            try {
                IntPtr handle = LoadImageW(module, new IntPtr(1), 1, size, size, 0);
                if (handle == IntPtr.Zero) throw new Exception("LoadImageW failed: " + Marshal.GetLastWin32Error());
                return handle;
            } finally { FreeLibrary(module); }
        }
    }
}
'@
}
$Executable = (Get-Item -LiteralPath $Executable).FullName
[IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
function Save-Icon([int]$Size, [string[]]$Names) {
    # Request each embedded resolution instead of enlarging ExtractAssociatedIcon's 32px image.
    $handle = [EdoltermPackaging.Icons]::Load($Executable, $Size)
    $icon = [Drawing.Icon]::FromHandle($handle)
    try {
        $bitmap = $icon.ToBitmap()
        try {
            foreach ($name in $Names) {
                $bitmap.Save((Join-Path $OutputDirectory $name), [Drawing.Imaging.ImageFormat]::Png)
            }
        } finally { $bitmap.Dispose() }
    } finally {
        $icon.Dispose()
        [EdoltermPackaging.Icons]::DestroyIcon($handle) | Out-Null
    }
}
Save-Icon 44 @('Logo44.png')
Save-Icon 150 @('Logo150.png')
Save-Icon 256 @('Logo.png')
foreach ($size in @(16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256)) {
    # Both shell themes need explicit unplated variants, even when artwork is identical.
    Save-Icon $size @("Logo44.targetsize-$size.png",
        "Logo44.targetsize-${size}_altform-unplated.png",
        "Logo44.targetsize-${size}_altform-lightunplated.png")
}

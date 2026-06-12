# @author kongweiguang
# Verifies the real Tauri WebView window can launch, resize, render non-empty screenshots, and initialize SQLite with isolated app data.

param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\src-tauri\target\release\net-power.exe"),
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class NetPowerWindowSmokeWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    public static IntPtr FindVisibleWindowForProcess(int processId) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((hWnd, lParam) => {
            uint windowProcessId;
            GetWindowThreadProcessId(hWnd, out windowProcessId);
            if (windowProcessId == processId && IsWindowVisible(hWnd) && GetWindowTextLength(hWnd) > 0) {
                found = hWnd;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static string GetTitle(IntPtr hWnd) {
        int length = GetWindowTextLength(hWnd);
        StringBuilder builder = new StringBuilder(length + 1);
        GetWindowText(hWnd, builder, builder.Capacity);
        return builder.ToString();
    }
}
"@

function Test-SqliteHeader {
    param([string]$Path)

    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
    try {
        $buffer = New-Object byte[] 16
        $read = $stream.Read($buffer, 0, $buffer.Length)
        if ($read -lt 16) {
            return $false
        }
        $header = [System.Text.Encoding]::ASCII.GetString($buffer)
        return $header.StartsWith("SQLite format 3")
    } finally {
        $stream.Dispose()
    }
}

function Stop-SmokeProcess {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$ExePath
    )

    try {
        if ($null -ne $Process -and -not $Process.HasExited) {
            $Process.Kill()
            $Process.WaitForExit(5000) | Out-Null
        }
        Get-Process -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $ExePath } |
            ForEach-Object {
                try {
                    $_.Kill()
                    $_.WaitForExit(5000) | Out-Null
                } catch {
                    Write-Warning "Failed to clean matching WebView smoke process: $($_.Exception.Message)"
                }
            }
    } catch {
        Write-Warning "Failed to clean WebView smoke process: $($_.Exception.Message)"
    }
}

function Wait-ForWindow {
    param(
        [System.Diagnostics.Process]$Process,
        [DateTime]$Deadline
    )

    while ([DateTime]::UtcNow -lt $Deadline) {
        if ($Process.HasExited) {
            throw "Tauri process exited before a visible window was found, ExitCode=$($Process.ExitCode)"
        }
        $window = [NetPowerWindowSmokeWin32]::FindVisibleWindowForProcess($Process.Id)
        if ($window -ne [IntPtr]::Zero) {
            return $window
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out waiting for the Tauri WebView window"
}

function Get-WindowBounds {
    param([IntPtr]$Window)

    $rect = New-Object NetPowerWindowSmokeWin32+RECT
    if (-not [NetPowerWindowSmokeWin32]::GetWindowRect($Window, [ref]$rect)) {
        throw "Could not read Tauri window bounds"
    }

    return [pscustomobject]@{
        Width = $rect.Right - $rect.Left
        Height = $rect.Bottom - $rect.Top
    }
}

function Save-WindowScreenshot {
    param(
        [IntPtr]$Window,
        [string]$Path
    )

    $bounds = Get-WindowBounds -Window $Window
    $width = $bounds.Width
    $height = $bounds.Height
    if ($width -lt 300 -or $height -lt 300) {
        throw "Unexpected Tauri window bounds: ${width}x${height}"
    }

    $bitmap = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, [System.Drawing.Size]::new($width, $height))
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }

    return [pscustomobject]@{
        Width = $width
        Height = $height
        Path = $Path
    }
}

function Assert-ImageIsNotBlank {
    param([string]$Path)

    $bitmap = [System.Drawing.Bitmap]::new($Path)
    try {
        $stepX = [Math]::Max(1, [Math]::Floor($bitmap.Width / 48))
        $stepY = [Math]::Max(1, [Math]::Floor($bitmap.Height / 36))
        $samples = 0
        $colors = New-Object 'System.Collections.Generic.HashSet[int]'
        for ($y = 0; $y -lt $bitmap.Height; $y += $stepY) {
            for ($x = 0; $x -lt $bitmap.Width; $x += $stepX) {
                $color = $bitmap.GetPixel($x, $y).ToArgb()
                [void]$colors.Add($color)
                $samples += 1
            }
        }
        if ($samples -lt 16 -or $colors.Count -lt 8) {
            throw "Screenshot appears blank or too uniform: $Path colors=$($colors.Count) samples=$samples"
        }
    } finally {
        $bitmap.Dispose()
    }
}

$resolvedExe = Resolve-Path -LiteralPath $ExePath
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "net-power-webview-smoke-$([System.Guid]::NewGuid().ToString('N'))"
$appData = Join-Path $tempRoot "Roaming"
$localAppData = Join-Path $tempRoot "Local"
$tempDir = Join-Path $tempRoot "Temp"
$netPowerAppData = Join-Path $tempRoot "NetPowerAppData"
$screenshotsDir = Join-Path $tempRoot "Screenshots"
$process = $null

try {
    New-Item -ItemType Directory -Force -Path $appData, $localAppData, $tempDir, $netPowerAppData, $screenshotsDir | Out-Null

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $resolvedExe.Path
    $startInfo.WorkingDirectory = Split-Path -Parent $resolvedExe.Path
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $false
    $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Normal
    $startInfo.Environment["APPDATA"] = $appData
    $startInfo.Environment["LOCALAPPDATA"] = $localAppData
    $startInfo.Environment["TEMP"] = $tempDir
    $startInfo.Environment["TMP"] = $tempDir
    $startInfo.Environment["NET_POWER_APP_DATA_DIR"] = $netPowerAppData

    $process = [System.Diagnostics.Process]::Start($startInfo)
    if ($null -eq $process) {
        throw "Tauri release exe did not start"
    }

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $window = Wait-ForWindow -Process $process -Deadline $deadline
    $title = [NetPowerWindowSmokeWin32]::GetTitle($window)
    if ($title -notlike "*net-power*") {
        throw "Unexpected Tauri window title: $title"
    }
    $initialBounds = Get-WindowBounds -Window $window
    if ($initialBounds.Width -lt 1180 -or $initialBounds.Height -lt 700) {
        throw "Initial Tauri window is too small for the full workbench: $($initialBounds.Width)x$($initialBounds.Height)"
    }

    $sizes = @(
        @{ Name = "desktop"; Width = 1280; Height = 720 },
        @{ Name = "narrow"; Width = 390; Height = 640 }
    )
    foreach ($size in $sizes) {
        if (-not [NetPowerWindowSmokeWin32]::MoveWindow($window, 40, 40, $size.Width, $size.Height, $true)) {
            throw "Could not resize Tauri window to $($size.Width)x$($size.Height)"
        }
        Start-Sleep -Milliseconds 900
        $screenshot = Save-WindowScreenshot -Window $window -Path (Join-Path $screenshotsDir "$($size.Name).png")
        Assert-ImageIsNotBlank -Path $screenshot.Path
        if ($size.Name -eq "narrow" -and $screenshot.Width -gt 430) {
            throw "Narrow Tauri window did not honor the responsive minimum width: $($screenshot.Width)"
        }
    }

    $dbFile = Get-ChildItem -LiteralPath $netPowerAppData -Recurse -File -Filter "proxy-tool.db" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -eq $dbFile) {
        throw "proxy-tool.db was not found in isolated app data"
    }
    if (-not (Test-SqliteHeader -Path $dbFile.FullName)) {
        throw "proxy-tool.db is not a valid SQLite database: $($dbFile.FullName)"
    }

    Write-Host "webview smoke passed: title=$title initial=$($initialBounds.Width)x$($initialBounds.Height) screenshots=$screenshotsDir"
} finally {
    Stop-SmokeProcess -Process $process -ExePath $resolvedExe.Path
    if ((Test-Path -LiteralPath $tempRoot) -and ([System.IO.Path]::GetFileName($tempRoot).StartsWith("net-power-webview-smoke-"))) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

# @author kongweiguang
# Verifies the real Tauri release window hides instead of exiting when it receives a close request.

param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\src-tauri\target\release\net-power.exe"),
    [int]$TimeoutSeconds = 20
)

$ErrorActionPreference = "Stop"

Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class NetPowerTraySmokeWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    public const int WM_CLOSE = 0x0010;

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, int msg, IntPtr wParam, IntPtr lParam);

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
                    Write-Warning "Failed to clean matching tray smoke process: $($_.Exception.Message)"
                }
            }
    } catch {
        Write-Warning "Failed to clean tray smoke process: $($_.Exception.Message)"
    }
}

function Wait-ForVisibleWindow {
    param(
        [System.Diagnostics.Process]$Process,
        [DateTime]$Deadline
    )

    while ([DateTime]::UtcNow -lt $Deadline) {
        if ($Process.HasExited) {
            throw "Tauri process exited before a visible window was found, ExitCode=$($Process.ExitCode)"
        }
        $window = [NetPowerTraySmokeWin32]::FindVisibleWindowForProcess($Process.Id)
        if ($window -ne [IntPtr]::Zero) {
            return $window
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out waiting for the Tauri window"
}

function Wait-ForSqliteDatabase {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$AppDataDir,
        [DateTime]$Deadline
    )

    while ([DateTime]::UtcNow -lt $Deadline) {
        if ($Process.HasExited) {
            throw "Tauri process exited before SQLite initialization, ExitCode=$($Process.ExitCode)"
        }
        $dbFile = Get-ChildItem -LiteralPath $AppDataDir -Recurse -File -Filter "proxy-tool.db" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $dbFile -and (Test-SqliteHeader -Path $dbFile.FullName)) {
            return $dbFile.FullName
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out waiting for proxy-tool.db initialization"
}

function Wait-ForHiddenWindow {
    param(
        [System.Diagnostics.Process]$Process,
        [DateTime]$Deadline
    )

    while ([DateTime]::UtcNow -lt $Deadline) {
        if ($Process.HasExited) {
            throw "Tauri process exited after close request; expected hidden-to-tray behavior"
        }
        $window = [NetPowerTraySmokeWin32]::FindVisibleWindowForProcess($Process.Id)
        if ($window -eq [IntPtr]::Zero) {
            return
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Timed out waiting for the Tauri window to hide after close request"
}

$resolvedExe = Resolve-Path -LiteralPath $ExePath
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "net-power-tray-smoke-$([System.Guid]::NewGuid().ToString('N'))"
$appData = Join-Path $tempRoot "Roaming"
$localAppData = Join-Path $tempRoot "Local"
$tempDir = Join-Path $tempRoot "Temp"
$netPowerAppData = Join-Path $tempRoot "NetPowerAppData"
$process = $null

try {
    New-Item -ItemType Directory -Force -Path $appData, $localAppData, $tempDir, $netPowerAppData | Out-Null

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
    $window = Wait-ForVisibleWindow -Process $process -Deadline $deadline
    $title = [NetPowerTraySmokeWin32]::GetTitle($window)
    if ($title -notlike "*net-power*") {
        throw "Unexpected Tauri window title: $title"
    }
    $dbPath = Wait-ForSqliteDatabase -Process $process -AppDataDir $netPowerAppData -Deadline $deadline

    if (-not [NetPowerTraySmokeWin32]::PostMessage($window, [NetPowerTraySmokeWin32]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw "Could not send WM_CLOSE to the Tauri window"
    }

    Wait-ForHiddenWindow -Process $process -Deadline ([DateTime]::UtcNow.AddSeconds($TimeoutSeconds))
    if ($process.HasExited) {
        throw "Tauri process exited after window close request"
    }

    Write-Host "tray smoke passed: title=$title processId=$($process.Id) database=$dbPath"
} finally {
    Stop-SmokeProcess -Process $process -ExePath $resolvedExe.Path
    if ((Test-Path -LiteralPath $tempRoot) -and ([System.IO.Path]::GetFileName($tempRoot).StartsWith("net-power-tray-smoke-"))) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

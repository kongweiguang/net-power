# @author kongweiguang
# 验证 release exe 能启动并初始化 SQLite 数据库。脚本使用临时 APPDATA/LOCALAPPDATA，结束后清理进程和临时目录。

param(
    [string]$ExePath = (Join-Path $PSScriptRoot "..\src-tauri\target\release\net-power.exe"),
    [int]$TimeoutSeconds = 20
)

$ErrorActionPreference = "Stop"

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
                    Write-Warning "清理 release smoke 同源进程失败: $($_.Exception.Message)"
                }
            }
    } catch {
        Write-Warning "清理 release smoke 进程失败: $($_.Exception.Message)"
    }
}

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

$resolvedExe = Resolve-Path -LiteralPath $ExePath
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "net-power-release-smoke-$([System.Guid]::NewGuid().ToString('N'))"
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
    $startInfo.CreateNoWindow = $true
    $startInfo.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
    $startInfo.Environment["APPDATA"] = $appData
    $startInfo.Environment["LOCALAPPDATA"] = $localAppData
    $startInfo.Environment["TEMP"] = $tempDir
    $startInfo.Environment["TMP"] = $tempDir
    $startInfo.Environment["NET_POWER_APP_DATA_DIR"] = $netPowerAppData

    $process = [System.Diagnostics.Process]::Start($startInfo)
    if ($null -eq $process) {
        throw "release exe 未能启动"
    }

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $dbFile = $null
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($process.HasExited) {
            throw "release exe 提前退出，ExitCode=$($process.ExitCode)"
        }

        $dbFile = Get-ChildItem -LiteralPath $netPowerAppData -Recurse -File -Filter "proxy-tool.db" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $dbFile -and $dbFile.Length -gt 0) {
            break
        }
        Start-Sleep -Milliseconds 250
    }

    if ($null -eq $dbFile) {
        throw "未在隔离 app data 中找到 proxy-tool.db"
    }
    if (-not (Test-SqliteHeader -Path $dbFile.FullName)) {
        throw "proxy-tool.db 不是有效 SQLite 数据库: $($dbFile.FullName)"
    }

    Write-Host "release smoke passed: $($dbFile.FullName)"
} finally {
    Stop-SmokeProcess -Process $process -ExePath $resolvedExe.Path
    if ((Test-Path -LiteralPath $tempRoot) -and ([System.IO.Path]::GetFileName($tempRoot).StartsWith("net-power-release-smoke-"))) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

# @author kongweiguang
# Verifies that the NSIS installer can install, launch the installed exe with isolated app data, render WebView responsively, hide-to-tray on close, and uninstall from a temp directory.

param(
    [string]$InstallerPath = (Join-Path $PSScriptRoot "..\src-tauri\target\release\bundle\nsis\net-power_0.1.0_x64-setup.exe"),
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"

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

function Get-NetPowerInstallRecords {
    $keys = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\net-power",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\net-power",
        "HKCU:\Software\github\net-power",
        "HKLM:\Software\github\net-power"
    )

    foreach ($key in $keys) {
        if (Test-Path -LiteralPath $key) {
            $props = Get-ItemProperty -LiteralPath $key
            [pscustomobject]@{
                Key = $key
                DisplayName = $props.DisplayName
                InstallLocation = $props.InstallLocation
                UninstallString = $props.UninstallString
                DefaultValue = $props.'(default)'
            }
        }
    }
}

function Test-RecordValueUnderPath {
    param(
        [object]$Record,
        [string]$PathPrefix
    )

    $values = @($Record.InstallLocation, $Record.UninstallString, $Record.DefaultValue) |
        Where-Object { $null -ne $_ -and "$_".Trim() -ne "" }
    foreach ($value in $values) {
        if ("$value".IndexOf($PathPrefix, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            return $true
        }
    }
    return $false
}

function Remove-SmokeInstallRecords {
    param([string]$PathPrefix)

    $records = @(Get-NetPowerInstallRecords)
    foreach ($record in $records) {
        if ((Test-RecordValueUnderPath -Record $record -PathPrefix $PathPrefix) -and ($record.Key -like "HKCU:\Software\github\net-power")) {
            Remove-Item -LiteralPath $record.Key -Recurse -Force
        }
    }
}

function Assert-NoExistingInstall {
    Remove-SmokeInstallRecords -PathPrefix (Join-Path ([System.IO.Path]::GetTempPath()) "net-power-installer-smoke-")
    $records = @(Get-NetPowerInstallRecords)
    if ($records.Count -gt 0) {
        $summary = ($records | ForEach-Object { "$($_.Key) InstallLocation=$($_.InstallLocation) UninstallString=$($_.UninstallString)" }) -join "; "
        throw "Existing net-power install records found; refusing installer smoke to avoid modifying an existing install: $summary"
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
                    Write-Warning "Failed to clean matching installer smoke process: $($_.Exception.Message)"
                }
            }
    } catch {
        Write-Warning "Failed to clean installer smoke process: $($_.Exception.Message)"
    }
}

function Invoke-CheckedProcess {
    param(
        [string]$FilePath,
        [string[]]$ArgumentList,
        [string]$ActionName
    )

    $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        throw "$ActionName failed, ExitCode=$($process.ExitCode)"
    }
}

$resolvedInstaller = Resolve-Path -LiteralPath $InstallerPath
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "net-power-installer-smoke-$([System.Guid]::NewGuid().ToString('N'))"
$installDir = Join-Path $tempRoot "Install"
$appData = Join-Path $tempRoot "Roaming"
$localAppData = Join-Path $tempRoot "Local"
$tempDir = Join-Path $tempRoot "Temp"
$netPowerAppData = Join-Path $tempRoot "NetPowerAppData"
$installedExe = Join-Path $installDir "net-power.exe"
$uninstaller = Join-Path $installDir "uninstall.exe"
$process = $null

try {
    Assert-NoExistingInstall
    New-Item -ItemType Directory -Force -Path $installDir, $appData, $localAppData, $tempDir, $netPowerAppData | Out-Null

    Invoke-CheckedProcess -FilePath $resolvedInstaller.Path -ArgumentList @("/S", "/NS", "/D=$installDir") -ActionName "NSIS silent install"

    if (-not (Test-Path -LiteralPath $installedExe)) {
        throw "Installed net-power.exe was not found: $installedExe"
    }
    if (-not (Test-Path -LiteralPath $uninstaller)) {
        throw "Installed uninstall.exe was not found: $uninstaller"
    }

    $records = @(Get-NetPowerInstallRecords)
    if ($records.Count -eq 0) {
        throw "Registry install records were not found after install"
    }

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $installedExe
    $startInfo.WorkingDirectory = $installDir
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
        throw "Installed exe did not start"
    }

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    $dbFile = $null
    while ([DateTime]::UtcNow -lt $deadline) {
        if ($process.HasExited) {
            throw "Installed exe exited early, ExitCode=$($process.ExitCode)"
        }

        $dbFile = Get-ChildItem -LiteralPath $netPowerAppData -Recurse -File -Filter "proxy-tool.db" -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $dbFile -and $dbFile.Length -gt 0) {
            break
        }
        Start-Sleep -Milliseconds 250
    }

    if ($null -eq $dbFile) {
        throw "proxy-tool.db was not found in isolated app data"
    }
    if (-not (Test-SqliteHeader -Path $dbFile.FullName)) {
        throw "proxy-tool.db is not a valid SQLite database: $($dbFile.FullName)"
    }

    Stop-SmokeProcess -Process $process -ExePath $installedExe
    $process = $null
    & (Join-Path $PSScriptRoot "verify-webview-smoke.ps1") -ExePath $installedExe -TimeoutSeconds $TimeoutSeconds
    & (Join-Path $PSScriptRoot "verify-tray-smoke.ps1") -ExePath $installedExe -TimeoutSeconds $TimeoutSeconds

    Invoke-CheckedProcess -FilePath $uninstaller -ArgumentList @("/S") -ActionName "NSIS silent uninstall"
    Start-Sleep -Milliseconds 500

    if ((Test-Path -LiteralPath $installedExe) -or (Test-Path -LiteralPath $uninstaller)) {
        throw "Installed files remain after uninstall: $installDir"
    }
    $remainingUninstallRecords = @(Get-NetPowerInstallRecords | Where-Object { $_.Key -like "*\Windows\CurrentVersion\Uninstall\*" })
    if ($remainingUninstallRecords.Count -gt 0) {
        $summary = ($remainingUninstallRecords | ForEach-Object { "$($_.Key)" }) -join "; "
        throw "Registry install records remain after uninstall: $summary"
    }
    Remove-SmokeInstallRecords -PathPrefix $tempRoot
    $remainingRecords = @(Get-NetPowerInstallRecords)
    if ($remainingRecords.Count -gt 0) {
        $summary = ($remainingRecords | ForEach-Object { "$($_.Key)" }) -join "; "
        throw "Non-smoke registry records remain after uninstall: $summary"
    }

    Write-Host "installer smoke passed: $installedExe"
} finally {
    Stop-SmokeProcess -Process $process -ExePath $installedExe
    if (Test-Path -LiteralPath $uninstaller) {
        try {
            Invoke-CheckedProcess -FilePath $uninstaller -ArgumentList @("/S") -ActionName "installer smoke cleanup uninstall"
        } catch {
            Write-Warning "Installer smoke cleanup uninstall failed: $($_.Exception.Message)"
        }
    }
    Remove-SmokeInstallRecords -PathPrefix $tempRoot
    if ((Test-Path -LiteralPath $tempRoot) -and ([System.IO.Path]::GetFileName($tempRoot).StartsWith("net-power-installer-smoke-"))) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}

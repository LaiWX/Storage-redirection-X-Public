param(
    [string]$Version = "",
    [int]$VersionCode = 0,
    [string]$OutputDir = "build",

    [switch]$SkipBuild,
    [switch]$NoAdb
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Fail {
    param([string]$Message)
    throw $Message
}

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [string[]]$Arguments = @()
    )

    $displayCommand = "$FilePath $($Arguments -join ' ')".Trim()
    $startedAt = Get-Date
    Write-Host "开始执行：$displayCommand"
    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE
    $elapsed = (Get-Date) - $startedAt
    Write-Host ("命令完成：耗时 {0:N1} 秒，退出代码 {1}" -f $elapsed.TotalSeconds, $exitCode)
    if ($exitCode -ne 0) {
        Fail "命令执行失败，退出代码 ${exitCode}：$displayCommand"
    }
}

function Get-FirstExistingPath {
    param([string[]]$Paths)

    foreach ($path in $Paths) {
        if ([string]::IsNullOrWhiteSpace($path)) {
            continue
        }
        if (Test-Path -LiteralPath $path) {
            return (Resolve-Path -LiteralPath $path).Path
        }
    }

    return $null
}

function Add-PathPrefix {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) {
        return
    }

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $parts = $env:Path -split [System.IO.Path]::PathSeparator
    if ($parts -notcontains $resolved) {
        $env:Path = "$resolved$([System.IO.Path]::PathSeparator)$env:Path"
    }
}

function Initialize-AndroidEnvironment {
    $sdk = Get-FirstExistingPath @(
        $env:ANDROID_HOME,
        $env:ANDROID_SDK_ROOT,
        (Join-Path $env:LOCALAPPDATA "Android\Sdk")
    )

    if (-not $sdk) {
        Fail "未找到 Android SDK。请设置 ANDROID_HOME，或通过 Android Studio 安装 SDK。"
    }
    $env:ANDROID_HOME = $sdk
    $env:ANDROID_SDK_ROOT = $sdk
    Add-PathPrefix (Join-Path $sdk "platform-tools")

    if (-not (Get-Command java -ErrorAction SilentlyContinue)) {
        Fail "PATH 中未找到 Java。请安装 JDK 21 并设置 JAVA_HOME。"
    }

    return @{ Sdk = $sdk }
}

function Resolve-LocalVersion {
    $json = & python (Join-Path $RepoRoot ".github/scripts/resolve_fork_version.py") --format json
    if ($LASTEXITCODE -ne 0) { Fail "Cannot resolve fork version" }
    $resolved = ($json -join "`n") | ConvertFrom-Json
    return @{ Version = $resolved.version; VersionCode = [int]$resolved.version_code }
}

function Assert-UnderPath {
    param(
        [string]$Path,
        [string]$Parent
    )

    $parentFull = [System.IO.Path]::GetFullPath($Parent).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)
    $pathFull = [System.IO.Path]::GetFullPath($Path).TrimEnd([System.IO.Path]::DirectorySeparatorChar, [System.IO.Path]::AltDirectorySeparatorChar)

    if (-not $pathFull.StartsWith($parentFull + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        Fail "拒绝删除预期父目录以外的路径：$pathFull"
    }
}

function Remove-LocalFile {
    param(
        [string]$Path,
        [string]$ExpectedParent
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }

    Assert-UnderPath -Path $Path -Parent $ExpectedParent
    Remove-Item -LiteralPath $Path -Force
}

function Test-ReleaseApk {
    param([string]$ApkPath)

    if (-not (Test-Path -LiteralPath $ApkPath)) {
        Fail "缺少 release APK：$ApkPath"
    }

    $item = Get-Item -LiteralPath $ApkPath
    if ($item.Length -le 0) {
        Fail "Release APK 为空：$ApkPath"
    }
}

function Get-AdbDevices {
    param([string]$AdbPath)

    $output = & $AdbPath devices
    if ($LASTEXITCODE -ne 0) {
        return @()
    }

    $devices = @()
    foreach ($line in $output | Select-Object -Skip 1) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        $parts = $line.Trim() -split "\s+"
        if ($parts.Count -ge 2) {
            $devices += [pscustomobject]@{
                Serial = $parts[0]
                State = $parts[1]
            }
        }
    }

    return $devices
}

function Confirm-YesNo {
    param([string]$Prompt)

    $answer = Read-Host "$Prompt [y/N]"
    return $answer -match "^(y|yes)$"
}

function Invoke-AdbChecked {
    param(
        [string]$AdbPath,
        [string]$Serial,
        [string[]]$Arguments
    )

    & $AdbPath -s $Serial @Arguments
    if ($LASTEXITCODE -ne 0) {
        Fail "adb 命令执行失败，退出码 ${LASTEXITCODE}：adb -s $Serial $($Arguments -join ' ')"
    }
}

function Restore-EnvVar {
    param(
        [string]$Name,
        [bool]$HadValue,
        [string]$OldValue
    )

    if ($HadValue) {
        Set-Item -Path "Env:\$Name" -Value $OldValue
    } else {
        Remove-Item -Path "Env:\$Name" -ErrorAction SilentlyContinue
    }
}

$savedSigning = @{}
foreach ($name in @('SRX_APP_SIGNING_STORE_FILE', 'SRX_APP_SIGNING_KEY_ALIAS', 'SRX_APP_SIGNING_STORE_PASSWORD', 'SRX_APP_SIGNING_KEY_PASSWORD')) {
    $savedSigning[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
Push-Location $RepoRoot
try {
    if (-not $env:SRX_APP_SIGNING_STORE_FILE -and (Test-Path (Join-Path $env:USERPROFILE '.android/storage-redirect-x-signing/signing.json'))) {
        . (Join-Path $PSScriptRoot 'load-local-signing.ps1')
    }
    $buildEnvironment = Initialize-AndroidEnvironment

    $resolved = Resolve-LocalVersion
    if (($Version -and $Version -ne $resolved.Version) -or ($VersionCode -ne 0 -and $VersionCode -ne $resolved.VersionCode)) {
        Fail "Version arguments must match fork-version.properties and release state"
    }
    if ([string]::IsNullOrWhiteSpace($Version)) {
        $Version = $resolved.Version
    }
    if ($VersionCode -le 0) {
        $VersionCode = $resolved.VersionCode
    }

    $outputRoot = [System.IO.Path]::GetFullPath((Join-Path $RepoRoot $OutputDir))
    New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null

    $sourceApk = Join-Path $RepoRoot "app\build\outputs\apk\release\app-release.apk"
    $apkPath = Join-Path $outputRoot "storage-redirect-x-manager-v$Version.apk"

    Write-Step "构建设置"
    Write-Host "构建变体：    release"
    Write-Host "版本：        v$Version"
    Write-Host "版本代码：    $VersionCode"
    Write-Host "输出目录：    $outputRoot"
    Write-Host "Android SDK： $($buildEnvironment.Sdk)"
    Write-Host "Java：        $env:JAVA_HOME"

    if (-not $SkipBuild) {
        $gradlePath = Join-Path $RepoRoot "gradlew.bat"
        if (-not (Test-Path -LiteralPath $gradlePath)) {
            Fail "缺少 Gradle wrapper：$gradlePath"
        }

        Write-Step "构建 release 应用"
        $hadVersion = Test-Path Env:\VERSION
        $oldVersion = $env:VERSION
        $hadVersionCode = Test-Path Env:\VERSION_CODE
        $oldVersionCode = $env:VERSION_CODE

        $env:VERSION = $Version
        $env:VERSION_CODE = [string]$VersionCode
        try {
            Invoke-Checked -FilePath $gradlePath -Arguments @("--no-configuration-cache", "--console=plain", ":app:assembleRelease")
        } finally {
            Restore-EnvVar -Name "VERSION" -HadValue $hadVersion -OldValue $oldVersion
            Restore-EnvVar -Name "VERSION_CODE" -HadValue $hadVersionCode -OldValue $oldVersionCode
        }
    } else {
        Write-Step "跳过构建并使用现有 release APK"
    }

    Write-Step "复制 APK 产物"
    Test-ReleaseApk -ApkPath $sourceApk
    Remove-LocalFile -Path $apkPath -ExpectedParent $outputRoot
    Copy-Item -LiteralPath $sourceApk -Destination $apkPath -Force
    Test-ReleaseApk -ApkPath $apkPath
    Write-Host "Release APK 已就绪：$apkPath" -ForegroundColor Green

    if ($NoAdb) {
        Write-Host "已通过 -NoAdb 跳过 ADB 步骤。"
    } else {
        $adbCommand = Get-Command adb -ErrorAction SilentlyContinue
        if (-not $adbCommand) {
            Write-Host "PATH 中未找到 adb，仅完成 APK 构建。" -ForegroundColor Yellow
        } else {
            Write-Step "检查已连接的 ADB 设备"
            $devices = @(Get-AdbDevices -AdbPath $adbCommand.Source)
            $onlineDevices = @($devices | Where-Object { $_.State -eq "device" })
            if ($onlineDevices.Count -eq 0) {
                Write-Host "未找到在线 adb 设备，仅完成 APK 构建。" -ForegroundColor Yellow
                if ($devices.Count -gt 0) {
                    Write-Host "非在线设备："
                    $devices | ForEach-Object { Write-Host "  $($_.Serial)  $($_.State)" }
                }
            } else {
                $serial = $onlineDevices[0].Serial
                if ($onlineDevices.Count -gt 1) {
                    Write-Host "在线设备："
                    for ($i = 0; $i -lt $onlineDevices.Count; $i++) {
                        Write-Host "  [$($i + 1)] $($onlineDevices[$i].Serial)"
                    }
                    $choice = Read-Host "请选择设备编号，或按 Enter 使用第 1 个设备"
                    if ($choice -match "^\d+$") {
                        $index = [int]$choice - 1
                        if ($index -ge 0 -and $index -lt $onlineDevices.Count) {
                            $serial = $onlineDevices[$index].Serial
                        }
                    }
                }

                if ((Confirm-YesNo "已发现连接设备 $serial，是否安装此 release APK？")) {
                    Write-Step "安装 release APK"
                    Invoke-AdbChecked -AdbPath $adbCommand.Source -Serial $serial -Arguments @("install", "-r", $apkPath)
                    Write-Host "APK 安装完成。" -ForegroundColor Green
                } else {
                    Write-Host "已跳过安装，APK 保留于：$apkPath"
                }
            }
        }
    }
} finally {
    foreach ($name in $savedSigning.Keys) {
        [Environment]::SetEnvironmentVariable($name, $savedSigning[$name], 'Process')
    }
    Pop-Location
}

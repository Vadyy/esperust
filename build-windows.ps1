[CmdletBinding()]
param([switch]$DryRun, [switch]$Help)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed (exit $LASTEXITCODE)." }
}

function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
        [Environment]::GetEnvironmentVariable('Path', 'User') + ';' + $script:originalPath
}

function Find-Python {
    $candidates = [Collections.Generic.List[object]]::new()
    if ($env:ESPER_PYTHON) { $candidates.Add(@{Command=$env:ESPER_PYTHON; Prefix=@()}) }
    $candidates.Add(@{Command=(Join-Path $PSScriptRoot '.venv\Scripts\python.exe'); Prefix=@()})
    foreach ($version in @('-3', '-3.15', '-3.14', '-3.13', '-3.12', '-3.11')) {
        $candidates.Add(@{Command='py'; Prefix=@($version)})
    }
    foreach ($name in @('python', 'python3', 'python3.15', 'python3.14', 'python3.13', 'python3.12', 'python3.11')) {
        $candidates.Add(@{Command=$name; Prefix=@()})
    }
    foreach ($key in @('HKCU:\Software\Python\PythonCore\*\InstallPath', 'HKLM:\Software\Python\PythonCore\*\InstallPath')) {
        foreach ($entry in @(Get-Item -Path $key -ErrorAction SilentlyContinue)) {
            $installedPath = $entry.GetValue('')
            if ($installedPath) { $candidates.Add(@{Command=(Join-Path $installedPath 'python.exe'); Prefix=@()}) }
        }
    }
    foreach ($candidate in $candidates) {
        $command = Get-Command $candidate.Command -ErrorAction SilentlyContinue
        if (!$command) { continue }
        # Avoid the python.exe Store placeholder, which can open a Store window.
        if ($command.Source -match 'WindowsApps\\python[0-9.]*\.exe$') { continue }
        try {
            $prefix = $candidate.Prefix
            $output = & $candidate.Command @prefix -c "import sys,platform; sys.exit(1) if not ((3,11)<=sys.version_info<(4,0) and platform.machine().lower() in ('amd64','x86_64')) else print(sys.executable)" 2>$null
            if ($LASTEXITCODE -eq 0 -and $output -and (Test-Path -LiteralPath "$output")) { return "$output" }
        } catch { continue }
    }
    return $null
}

function Ensure-Winget {
    if (Get-Command winget.exe -ErrorAction SilentlyContinue) { return }
    Write-Host 'Installing/repairing WinGet from the Microsoft PowerShell Gallery module...'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Install-PackageProvider -Name NuGet -Scope CurrentUser -Force | Out-Null
    Install-Module Microsoft.WinGet.Client -Repository PSGallery -Scope CurrentUser -Force -AllowClobber
    Import-Module Microsoft.WinGet.Client
    Repair-WinGetPackageManager
    Refresh-Path
    if (!(Get-Command winget.exe -ErrorAction SilentlyContinue)) { throw 'WinGet is still unavailable. Install Microsoft App Installer and rerun.' }
}

function Install-WingetPackage {
    param([string]$Id, [string[]]$Extra=@())
    Ensure-Winget
    Invoke-Checked -Program 'winget.exe' -Arguments (@('install', '--exact', '--id', $Id, '--source', 'winget',
        '--accept-source-agreements', '--accept-package-agreements') + $Extra)
    Refresh-Path
}

function Find-VS {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (!(Test-Path $vswhere)) { return $null }
    $result = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($result) { return "$result" }
    return $null
}

function Import-VS {
    param([string]$Installation)
    $env:ESPER_VSDEVCMD = Join-Path $Installation 'Common7\Tools\VsDevCmd.bat'
    try {
        $lines = & $env:ComSpec /d /c 'call "%ESPER_VSDEVCMD%" -no_logo -arch=x64 -host_arch=x64 >nul && set'
        if ($LASTEXITCODE -ne 0) { throw 'Visual Studio developer environment initialization failed.' }
        foreach ($line in $lines) {
            $split = $line.IndexOf('=')
            if ($split -gt 0) { [Environment]::SetEnvironmentVariable($line.Substring(0,$split), $line.Substring($split+1), 'Process') }
        }
    } finally { Remove-Item Env:ESPER_VSDEVCMD -ErrorAction SilentlyContinue }
}

function Main {
    if ($Help) {
        Write-Host 'Usage: build-windows.bat [-DryRun]'
        Write-Host 'Checks prerequisites, asks before installing, then builds an x64 MSVC release.'
        return
    }
    if ($env:OS -ne 'Windows_NT') { throw 'This script is for Windows.' }
    Set-Location -LiteralPath $PSScriptRoot
    $script:originalPath = $env:Path
    $python = Find-Python
    $visualStudio = Find-VS
    Write-Host "Python: $(if ($python) {$python} else {'will install Python 3.12 x64'})"
    Write-Host "C++ tools: $(if ($visualStudio) {$visualStudio} else {'will install Visual Studio Build Tools with C++ and Windows SDK'})"
    Write-Host 'Plan: install missing WinGet/Git/Python/C++ tools; create .venv; install Python requirements;'
    Write-Host 'set up local Rust and vcpkg; build OpenSSL/LZO; verify importer; build a release.'
    Write-Host 'Downloads use Microsoft/WinGet, python.org, PyPI, rust-lang.org and vcpkg sources.'
    Write-Host 'Windows may request administrator approval or a restart for C++ tools.'
    if ($DryRun) { return }
    $answer = Read-Host 'Allow these installations and build the release? [y/N]'
    if ($answer -notmatch '^(y|yes)$') { Write-Host 'Cancelled. Nothing was installed.'; return }
    New-Item -ItemType Directory -Force '.build' | Out-Null
    if (!$python) {
        Install-WingetPackage -Id 'Python.Python.3.12' -Extra @('--architecture','x64')
        $python = Find-Python
        if (!$python) { throw 'Python 3.11+ x64 was not found after installation. Restart your terminal and rerun.' }
    }
    if ($visualStudio) { Import-VS $visualStudio }
    if (!$visualStudio -or !(Get-Command rc.exe -ErrorAction SilentlyContinue)) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        $buildTools = if (Test-Path $vswhere) { & $vswhere -latest -products Microsoft.VisualStudio.Product.BuildTools -property installationPath } else { $null }
        if ($buildTools) {
            $installer = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\setup.exe'
            $options = @('modify','--installPath',"`"$buildTools`"",'--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended','--passive','--norestart')
            $process = Start-Process -FilePath $installer -ArgumentList $options -Verb RunAs -Wait -PassThru
            if ($process.ExitCode -eq 3010) { throw 'C++ tools installed. Restart Windows, then rerun this script.' }
            if ($process.ExitCode -ne 0) { throw "Visual Studio setup failed: $($process.ExitCode)" }
        } else {
            Install-WingetPackage -Id 'Microsoft.VisualStudio.2022.BuildTools' -Extra @('--override','--wait --passive --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
        }
        $visualStudio = Find-VS
        if (!$visualStudio) { throw 'C++ tools are not ready. Restart Windows if requested, then rerun.' }
        Import-VS $visualStudio
    }
    foreach ($tool in @('cl.exe','link.exe','rc.exe')) {
        if (!(Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is missing. Repair Desktop development with C++ and the Windows SDK, then rerun." }
    }
    if (!(Get-Command git.exe -ErrorAction SilentlyContinue)) {
        Install-WingetPackage 'Git.Git'
        Import-VS $visualStudio
    }
    $venvPython = Join-Path $PSScriptRoot '.venv\Scripts\python.exe'
    if (!(Test-Path $venvPython)) { Invoke-Checked -Program $python -Arguments @('-m','venv','.venv') }
    Invoke-Checked -Program $venvPython -Arguments @('-m','ensurepip','--upgrade')
    Invoke-Checked -Program $venvPython -Arguments @('-m','pip','install','--upgrade','pip')
    Invoke-Checked -Program $venvPython -Arguments @('-m','pip','install','-r','tools/requirements.txt')

    $vcpkgRoot = Join-Path $PSScriptRoot '.build\vcpkg'
    if (!(Test-Path (Join-Path $vcpkgRoot '.git'))) {
        Invoke-Checked -Program 'git.exe' -Arguments @('clone','--depth','1','https://github.com/microsoft/vcpkg.git',$vcpkgRoot)
    }
    $vcpkg = Join-Path $vcpkgRoot 'vcpkg.exe'
    if (!(Test-Path $vcpkg)) { Invoke-Checked -Program (Join-Path $vcpkgRoot 'bootstrap-vcpkg.bat') -Arguments @('-disableMetrics') }
    Invoke-Checked -Program $vcpkg -Arguments @('install','openssl:x64-windows','lzo:x64-windows','--disable-metrics')
    Invoke-Checked -Program $venvPython -Arguments @('tools/setup_importer.py','--library-dir',(Join-Path $vcpkgRoot 'installed\x64-windows\bin'))

    if (!$env:CARGO_HOME) { $env:CARGO_HOME = Join-Path $PSScriptRoot '.cargo' }
    if (!$env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $PSScriptRoot '.rustup' }
    $env:Path = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:Path
    $rustup = Get-Command rustup.exe -ErrorAction SilentlyContinue
    if (!$rustup) {
        $bootstrap = Join-Path $PSScriptRoot '.build\rustup-init.exe'
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $bootstrap -UseBasicParsing
        Invoke-Checked -Program $bootstrap -Arguments @('-y','--no-modify-path','--profile','minimal','--default-toolchain','none','--default-host','x86_64-pc-windows-msvc')
    }
    $version = [regex]::Match((Get-Content rust-toolchain.toml -Raw), 'channel\s*=\s*"([^"]+)"').Groups[1].Value
    if (!$version) { throw 'Cannot read rust-toolchain.toml.' }
    $toolchain = "$version-x86_64-pc-windows-msvc"
    Invoke-Checked -Program 'rustup.exe' -Arguments @('toolchain','install',$toolchain,'--profile','minimal')
    Invoke-Checked -Program 'rustup.exe' -Arguments @('run',$toolchain,'cargo','build','--release','--locked')
    Write-Host "Release built: $PSScriptRoot\target\release\esperust.exe"
    Write-Host 'Run it from this folder, or pass --resources C:\path\to\client.'
}

try { Main; exit 0 } catch { Write-Error $_ -ErrorAction Continue; exit 1 }

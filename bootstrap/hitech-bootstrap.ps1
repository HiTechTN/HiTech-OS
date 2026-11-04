# HiTech-OS universal bootstrap — Windows / WSL2 companion.
# Engineering and documentation: Mohamed Azmi Kaaniche (AZMI), HiTechTN, HiTechLab.
[CmdletBinding()]
param(
    [ValidateSet("orion", "edge", "server", "minimal")]
    [string]$Profile = "orion",
    [switch]$DryRun,
    [switch]$Verify,
    [string]$Distro = "",
    [string]$Source = ""
)

$ErrorActionPreference = "Stop"

function Write-Hitech([string]$Message, [string]$Color = "Cyan") {
    Write-Host "[hitech-bootstrap] $Message" -ForegroundColor $Color
}

function Fail([string]$Message) {
    Write-Host "[fail] $Message" -ForegroundColor Red
    exit 1
}

function Get-WindowsHardware {
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
    $gpu = Get-CimInstance Win32_VideoController | Select-Object -First 1
    $ram = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1MB)
    [pscustomobject]@{
        CPU = if ($cpu) { $cpu.Name.Trim() } else { "unknown" }
        Cores = if ($cpu) { $cpu.NumberOfCores } else { 0 }
        RAMMiB = $ram
        GPU = if ($gpu) { $gpu.Name.Trim() } else { "unknown" }
        VRAMMiB = if ($gpu) { [math]::Round($gpu.AdapterRAM / 1MB) } else { 0 }
    }
}

function Get-WslDistros {
    $raw = & wsl.exe -l -q 2>$null
    if ($LASTEXITCODE -ne 0) { return @() }
    return @($raw | ForEach-Object { ($_ -replace "`0", "").Trim() } | Where-Object { $_ })
}

Write-Hitech "HiTech-OS Windows/WSL2 bootstrap"
$hw = Get-WindowsHardware
Write-Hitech ("Windows CPU: {0} ({1} cores), RAM: {2} MiB, GPU: {3} ({4} MiB)" -f $hw.CPU, $hw.Cores, $hw.RAMMiB, $hw.GPU, $hw.VRAMMiB)

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
    Fail "WSL is not installed. Run 'wsl --install' as Administrator, reboot, then rerun this script."
}

$distros = Get-WslDistros
if ($distros.Count -eq 0) {
    if ($DryRun -or $Verify) {
        Write-Hitech "No WSL distribution found (dry-run/verify only)." Yellow
        exit 0
    }
    Fail "No WSL distribution found. Run 'wsl --install -d Ubuntu' as Administrator, reboot, then rerun."
}

if (-not $Distro) {
    $Distro = ($distros | Where-Object { $_ -match "Ubuntu" } | Select-Object -First 1)
    if (-not $Distro) { $Distro = $distros[0] }
}
if ($distros -notcontains $Distro) {
    Fail "WSL distribution '$Distro' not found. Available: $($distros -join ', ')"
}

$versionLine = (& wsl.exe -l -v 2>$null | Select-String -Pattern "\s$([regex]::Escape($Distro))\s" | Select-Object -First 1)
if ($versionLine -and ($versionLine.Line -notmatch "\s2\s*$")) {
    if ($DryRun -or $Verify) {
        Write-Hitech "Distribution '$Distro' is not WSL2." Yellow
    } else {
        Write-Hitech "Converting '$Distro' to WSL2..." Yellow
        & wsl.exe --set-version $Distro 2
        if ($LASTEXITCODE -ne 0) { Fail "Could not convert '$Distro' to WSL2." }
    }
}

$scriptPath = Join-Path $PSScriptRoot "hitech-bootstrap.sh"
if (-not (Test-Path $scriptPath)) { Fail "Missing companion script: $scriptPath" }
$linuxScript = (& wsl.exe -d $Distro -- wslpath -a $scriptPath 2>$null | Out-String).Trim()
if (-not $linuxScript) { Fail "Could not translate $scriptPath inside WSL." }

$argsList = @("--profile", $Profile)
if ($DryRun) { $argsList += "--dry-run" }
if ($Verify) { $argsList += "--verify" }
if ($Source) {
    $linuxSource = (& wsl.exe -d $Distro -- wslpath -a $Source 2>$null | Out-String).Trim()
    $argsList += @("--source", $linuxSource)
}

Write-Hitech "Running inside WSL2 distribution '$Distro': $linuxScript $($argsList -join ' ')"
& wsl.exe -d $Distro -- sudo -- bash $linuxScript @argsList
if ($LASTEXITCODE -ne 0) { Fail "Bootstrap failed inside WSL2 (exit $LASTEXITCODE)." }
Write-Hitech "Bootstrap completed. Reboot Windows if the script installed Nix or changed WSL systemd settings." Green

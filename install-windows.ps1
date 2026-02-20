#
# Chronova CLI Windows Installer
# Drop-in replacement for wakatime-cli
# https://chronova.dev
#

param(
    [string]$ApiKey = ""
)

# Configuration
$Repo = "nx-solutions-ug/chronova-cli"
$ApiUrl = "https://api.github.com/repos/$Repo"
$DefaultApiUrl = "https://chronova.dev/api/v1"
$ScriptPath = $MyInvocation.MyCommand.Path

# Colors for output (Windows 10+ supports ANSI)
$HostSupportsAnsi = $Host.UI.SupportsVirtualTerminal -or ($env:WT_SESSION -or $env:TERM_PROGRAM)

function Write-ColorOutput {
    param(
        [string]$Text,
        [string]$Color = "White"
    )
    
    if ($HostSupportsAnsi) {
        switch ($Color) {
            "Red" { Write-Host "`e[0;31m$Text`e[0m" }
            "Green" { Write-Host "`e[0;32m$Text`e[0m" }
            "Yellow" { Write-Host "`e[1;33m$Text`e[0m" }
            "Blue" { Write-Host "`e[0;34m$Text`e[0m" }
            default { Write-Host $Text }
        }
    } else {
        Write-Host $Text
    }
}

Write-ColorOutput "##########################" "Green"
Write-ColorOutput "# CHRONOVA CLI INSTALLER #" "Green"
Write-ColorOutput "#      for Windows       #" "Green"
Write-ColorOutput "##########################" "Green"
Write-Host ""

# ============================================
# REQUIREMENTS CHECK
# ============================================
Write-ColorOutput "Checking requirements..." "Blue"

# Check PowerShell version
if ($PSVersionTable.PSVersion.Major -lt 5) {
    Write-ColorOutput "Error: PowerShell 5.0 or higher is required. You have version $($PSVersionTable.PSVersion)" "Red"
    exit 1
}

# Check for required cmdlets
$requiredCmdlets = @('Invoke-WebRequest', 'Expand-Archive')
foreach ($cmdlet in $requiredCmdlets) {
    if (-not (Get-Command $cmdlet -ErrorAction SilentlyContinue)) {
        Write-ColorOutput "Error: Required cmdlet '$cmdlet' is not available" "Red"
        exit 1
    }
}

# Check for Execution Policy
$executionPolicy = Get-ExecutionPolicy
if ($executionPolicy -eq 'Restricted') {
    Write-ColorOutput "Warning: Your Execution Policy is set to 'Restricted'" "Yellow"
    Write-ColorOutput "You may need to run: Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser" "Yellow"
}

Write-ColorOutput "All requirements met!" "Green"
Write-Host ""

# ============================================
# PLATFORM DETECTION
# ============================================
Write-ColorOutput "Detecting architecture..." "Blue"

$Architecture = $env:PROCESSOR_ARCHITECTURE
$Arch = switch ($Architecture) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    "x86" { "i686" }
    default { "unknown" }
}

if ($Arch -eq "unknown") {
    Write-ColorOutput "Error: Unsupported architecture: $Architecture" "Red"
    exit 1
}

Write-ColorOutput "  Architecture: $Arch" "Green"
Write-ColorOutput "  Platform: Windows" "Green"
Write-Host ""

# Get target triple
$Target = "$Arch-pc-windows-msvc"
Write-ColorOutput "  Target: $Target" "Green"
Write-Host ""

# ============================================
# FETCH LATEST VERSION
# ============================================
Write-ColorOutput "Fetching latest release version..." "Blue"

if ($env:CHRONOVA_CLI_VERSION) {
    $Version = $env:CHRONOVA_CLI_VERSION
    Write-ColorOutput "  Using specified version: $Version" "Green"
} else {
    try {
        $response = Invoke-WebRequest -Uri "$ApiUrl/releases/latest" -UseBasicParsing
        $content = $response.Content
        if ($content -match '"tag_name":\s*"([^"]+)"') {
            $Version = $matches[1]
            Write-ColorOutput "  Latest version: $Version" "Green"
        } else {
            throw "Could not parse version"
        }
    } catch {
        Write-ColorOutput "Error: Could not determine latest version" "Red"
        Write-ColorOutput "  $_" "Red"
        exit 1
    }
}
Write-Host ""

# ============================================
# BACKUP EXISTING WAKATIME DATA
# ============================================
Write-ColorOutput "Checking for existing WakaTime installation..." "Blue"

$Timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$WakaTimeDir = "$env:USERPROFILE\.wakatime"
$WakaTimeCfg = "$env:USERPROFILE\.wakatime.cfg"

if (Test-Path $WakaTimeDir) {
    $BackupDir = "$env:USERPROFILE\.wakatime-backup-$Timestamp"
    Write-ColorOutput "  Backing up ~/.wakatime to $BackupDir" "Yellow"
    try {
        Copy-Item -Path $WakaTimeDir -Destination $BackupDir -Recurse -Force
    } catch {
        Write-ColorOutput "  Warning: Could not backup .wakatime directory" "Yellow"
    }
} else {
    Write-ColorOutput "  No existing ~/.wakatime directory found" "Green"
}

if (Test-Path $WakaTimeCfg) {
    $BackupCfg = "$env:USERPROFILE\.wakatime.cfg.backup-$Timestamp"
    Write-ColorOutput "  Backing up ~/.wakatime.cfg to $BackupCfg" "Yellow"
    try {
        Copy-Item -Path $WakaTimeCfg -Destination $BackupCfg -Force
    } catch {
        Write-ColorOutput "  Warning: Could not backup .wakatime.cfg" "Yellow"
    }
} else {
    Write-ColorOutput "  No existing ~/.wakatime.cfg found" "Green"
}
Write-Host ""

# ============================================
# CREATE DIRECTORIES
# ============================================
Write-ColorOutput "Creating directories..." "Blue"

$ChronovaDir = "$env:USERPROFILE\.chronova"
$LocalBinDir = "$env:USERPROFILE\.local\bin"

New-Item -ItemType Directory -Force -Path $ChronovaDir | Out-Null
New-Item -ItemType Directory -Force -Path $WakaTimeDir | Out-Null
New-Item -ItemType Directory -Force -Path $LocalBinDir | Out-Null

Write-ColorOutput "  Created: $ChronovaDir" "Green"
Write-ColorOutput "  Created: $WakaTimeDir" "Green"
Write-ColorOutput "  Created: $LocalBinDir" "Green"
Write-Host ""

# ============================================
# DOWNLOAD BINARY
# ============================================
Write-ColorOutput "Downloading Chronova CLI binary..." "Blue"

$ArchiveName = "chronova-cli-$Version-$Target.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$Version/$ArchiveName"

Write-ColorOutput "  URL: $DownloadUrl" "Yellow"

$TempDir = [System.IO.Path]::GetTempPath() + [System.Guid]::NewGuid().ToString()
New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile "$TempDir\$ArchiveName" -UseBasicParsing
    Write-ColorOutput "  Downloaded successfully" "Green"
} catch {
    Write-ColorOutput "Error: Failed to download binary" "Red"
    Write-ColorOutput "  URL: $DownloadUrl" "Red"
    Write-ColorOutput "  $_" "Red"
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

# Extract archive
Write-ColorOutput "Extracting archive..." "Blue"
Expand-Archive -Path "$TempDir\$ArchiveName" -DestinationPath $TempDir -Force

# Find and move binary
$LocalBinary = "$ChronovaDir\chronova-cli.exe"
$ExtractedBinary = Get-ChildItem -Path $TempDir -Recurse -Filter "chronova-cli.exe" | Select-Object -First 1

if ($ExtractedBinary) {
    Copy-Item -Path $ExtractedBinary.FullName -Destination $LocalBinary -Force
} else {
    Write-ColorOutput "Error: Could not find chronova-cli.exe in archive" "Red"
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
    exit 1
}

Write-ColorOutput "  Binary installed to: $LocalBinary" "Green"
Write-Host ""

# ============================================
# CREATE COPIES (Windows doesn't handle symlinks well without admin)
# ============================================
Write-ColorOutput "Creating copies (Windows compatibility)..." "Blue"

# Copy for local bin
Copy-Item -Path $LocalBinary -Destination "$LocalBinDir\chronova-cli.exe" -Force
Copy-Item -Path $LocalBinary -Destination "$LocalBinDir\wakatime-cli.exe" -Force
Write-ColorOutput "  Created: $LocalBinDir\chronova-cli.exe" "Green"
Write-ColorOutput "  Created: $LocalBinDir\wakatime-cli.exe" "Green"

# Copy to .wakatime for VSCode extension compatibility
$WakaTimeCliName = "wakatime-cli-windows-$Arch.exe"
Copy-Item -Path $LocalBinary -Destination "$WakaTimeDir\$WakaTimeCliName" -Force
Copy-Item -Path $LocalBinary -Destination "$WakaTimeDir\wakatime-cli.exe" -Force
Write-ColorOutput "  Created: $WakaTimeDir\$WakaTimeCliName" "Green"
Write-ColorOutput "  Created: $WakaTimeDir\wakatime-cli.exe" "Green"
Write-Host ""

# Cleanup temp directory
Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue

# ============================================
# CONFIGURATION
# ============================================
Write-ColorOutput "Setting up configuration..." "Blue"

# Extract existing API key from WakaTime config
function Extract-ApiKey {
    param([string]$ConfigFile)
    
    if (Test-Path $ConfigFile) {
        $content = Get-Content $ConfigFile -Raw
        if ($content -match '(?im)^api_key\s*=\s*(.+)$') {
            $key = $matches[1].Trim()
            if ($key -and $key -ne "your_api_key_here") {
                return $key
            }
        }
    }
    return $null
}

$ApiKeyValue = $ApiKey

# Try to get API key from existing configs
Write-ColorOutput "  Looking for existing API key..." "Blue"

if (-not $ApiKeyValue -and (Test-Path $WakaTimeCfg)) {
    $ApiKeyValue = Extract-ApiKey -ConfigFile $WakaTimeCfg
    if ($ApiKeyValue) {
        Write-ColorOutput "    Found API key in ~/.wakatime.cfg" "Green"
    }
}

if (-not $ApiKeyValue -and $BackupCfg -and (Test-Path $BackupCfg)) {
    $ApiKeyValue = Extract-ApiKey -ConfigFile $BackupCfg
    if ($ApiKeyValue) {
        Write-ColorOutput "    Found API key in backup config" "Green"
    }
}

$ChronovaCfg = "$env:USERPROFILE\.chronova.cfg"
if (-not $ApiKeyValue -and (Test-Path $ChronovaCfg)) {
    $ApiKeyValue = Extract-ApiKey -ConfigFile $ChronovaCfg
    if ($ApiKeyValue) {
        Write-ColorOutput "    Found API key in existing ~/.chronova.cfg" "Green"
    }
}

# Create Chronova config
if (-not (Test-Path $ChronovaCfg)) {
    Write-ColorOutput "  Creating default configuration..." "Green"
    @"
[settings]
api_url = $DefaultApiUrl
api_key = your_api_key_here
debug = false
hidefilenames = false
include_only_with_project_file = false
status_bar_enabled = true
"@ | Set-Content -Path $ChronovaCfg -Encoding UTF8
}

# Update API URL
Write-ColorOutput "  Setting API URL to $DefaultApiUrl" "Green"
$content = Get-Content $ChronovaCfg -Raw
$content = $content -replace '(?im)^api_url\s*=.*$', "api_url = $DefaultApiUrl"
$content | Set-Content -Path $ChronovaCfg -Encoding UTF8

# Update API key if found
if ($ApiKeyValue) {
    $content = Get-Content $ChronovaCfg -Raw
    $content = $content -replace '(?im)^api_key\s*=.*$', "api_key = $ApiKeyValue"
    $content | Set-Content -Path $ChronovaCfg -Encoding UTF8
    Write-ColorOutput "  API key configured from existing config" "Green"
}

# Copy config to WakaTime location (Windows doesn't support symlinks well without admin)
Copy-Item -Path $ChronovaCfg -Destination $WakaTimeCfg -Force
Write-ColorOutput "  Config copied to: $WakaTimeCfg" "Green"

# Set permissions (Windows handles this differently, just ensure it's not read-only)
Set-ItemProperty -Path $ChronovaCfg -Name IsReadOnly -Value $false -ErrorAction SilentlyContinue
Write-Host ""

# ============================================
# INTERACTIVE API KEY PROMPT
# ============================================
if (-not $ApiKeyValue -and $Host.Name -eq 'ConsoleHost') {
    Write-ColorOutput "========================================" "Yellow"
    Write-ColorOutput "API Key Configuration" "Yellow"
    Write-ColorOutput "========================================" "Yellow"
    Write-Host ""
    Write-ColorOutput "You can get your API key from: https://chronova.dev/settings" "Blue"
    Write-ColorOutput "(Copy the link and paste it into your browser)" "Yellow"
    Write-Host ""
    
    $response = Read-Host "Would you like to enter your API key now? (y/n)"
    
    if ($response -match '^[Yy]$') {
        Write-Host ""
        Write-ColorOutput "Please enter your API key from https://chronova.dev/settings:" "Yellow"
        $apiKeyInput = Read-Host -AsSecureString "API Key"
        
        # Convert secure string to plain text for config file
        $BSTR = [System.Runtime.InteropServices.Marshal]::SecureStringToBSTR($apiKeyInput)
        $plainApiKey = [System.Runtime.InteropServices.Marshal]::PtrToStringAuto($BSTR)
        [System.Runtime.InteropServices.Marshal]::ZeroFreeBSTR($BSTR)
        
        if ($plainApiKey) {
            $content = Get-Content $ChronovaCfg -Raw
            $content = $content -replace '(?im)^api_key\s*=.*$', "api_key = $plainApiKey"
            $content | Set-Content -Path $ChronovaCfg -Encoding UTF8
            
            # Also update the WakaTime copy
            Copy-Item -Path $ChronovaCfg -Destination $WakaTimeCfg -Force
            
            Write-ColorOutput "API key saved to ~/.chronova.cfg" "Green"
            $ApiKeyValue = $plainApiKey
        } else {
            Write-ColorOutput "No API key entered. You can configure it later by editing ~/.chronova.cfg" "Yellow"
        }
    } else {
        Write-ColorOutput "Skipping API key configuration." "Yellow"
        Write-ColorOutput "You can add it later by editing ~/.chronova.cfg" "Yellow"
    }
    Write-Host ""
}

# ============================================
# UPDATE PATH
# ============================================
Write-ColorOutput "Checking PATH configuration..." "Blue"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$LocalBinDir*") {
    Write-ColorOutput "  Adding $LocalBinDir to your PATH..." "Yellow"
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$LocalBinDir", "User")
    Write-ColorOutput "  Added to PATH. You may need to restart your terminal for changes to take effect." "Green"
} else {
    Write-ColorOutput "  $LocalBinDir is already in your PATH" "Green"
}
Write-Host ""

# ============================================
# INSTALLATION COMPLETE
# ============================================
Write-ColorOutput "========================================" "Green"
Write-ColorOutput "Installation Complete!" "Green"
Write-ColorOutput "========================================" "Green"
Write-Host ""
Write-ColorOutput "Installation Summary:" "Blue"
Write-ColorOutput "  Binary:           $LocalBinary" "Green"
Write-ColorOutput "  Version:          $Version" "Green"
Write-ColorOutput "  Architecture:     $Arch" "Green"
Write-ColorOutput "  Config file:      $ChronovaCfg" "Green"
Write-ColorOutput "  Copies:           $LocalBinDir\chronova-cli.exe" "Green"
Write-ColorOutput "                    $LocalBinDir\wakatime-cli.exe" "Green"
Write-ColorOutput "                    $WakaTimeDir\$WakaTimeCliName" "Green"
Write-Host ""

if ($ApiKeyValue) {
    Write-ColorOutput "  API Key:          Configured ✓" "Green"
} else {
    Write-ColorOutput "  API Key:          Not configured" "Yellow"
    Write-ColorOutput "                    Visit https://chronova.dev/settings to get your key" "Yellow"
    Write-ColorOutput "                    Then run: chronova-cli --config to verify" "Yellow"
}
Write-Host ""

# Test installation
Write-ColorOutput "Testing installation..." "Blue"
try {
    $versionOutput = & "$LocalBinary" --version 2>&1
    Write-ColorOutput "  ✓ Binary working: $versionOutput" "Green"
} catch {
    Write-ColorOutput "  Warning: Could not verify binary" "Yellow"
}
Write-Host ""

Write-ColorOutput "The WakaTime VSCode extension will now use Chronova CLI automatically!" "Green"
Write-Host ""
Write-ColorOutput "Next steps:" "Blue"
Write-ColorOutput "  1. Restart your terminal to apply PATH changes" "Blue"
Write-ColorOutput "  2. Restart VSCode for changes to take effect" "Blue"
Write-ColorOutput "  3. Visit https://chronova.dev/docs for more information" "Blue"
Write-Host ""

# ============================================
# CLEANUP - Delete the script
# ============================================
Write-ColorOutput "Cleaning up installer..." "Blue"
if ($ScriptPath -and (Test-Path $ScriptPath)) {
    Remove-Item -Path $ScriptPath -Force
    Write-ColorOutput "  Installer script removed" "Green"
}
Write-Host ""

Write-ColorOutput "Done!" "Green"
exit 0

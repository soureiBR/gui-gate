# SoureiGate Installer — Windows (PowerShell)
# Uso: irm https://github.com/SEU_ORG/soureigate/raw/main/install.ps1 | iex

$ErrorActionPreference = "Stop"

Write-Host "=== SoureiGate Installer ===" -ForegroundColor Cyan
Write-Host ""

# Diretório de instalação
$InstallDir = "$env:LOCALAPPDATA\SoureiGate"
$ConfigDir = "$env:APPDATA\soureigate"

# Cria diretórios
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null

# Detecta arquitetura
$Arch = if ([Environment]::Is64BitOperatingSystem) { "amd64" } else { "x86" }
$FileName = "gate-windows-$Arch.exe"

# Baixa o binário do GitHub Releases (latest)
$Repo = "soureiBR/gui-gate"
$ReleaseUrl = "https://github.com/$Repo/releases/latest/download/$FileName"

Write-Host "Baixando $FileName..."
Invoke-WebRequest -Uri $ReleaseUrl -OutFile "$InstallDir\gate.exe"

Write-Host "Instalado em $InstallDir\gate.exe"

# Adiciona ao PATH do usuário (permanente)
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host ""
    Write-Host "Adicionado ao PATH do usuario." -ForegroundColor Green
    Write-Host "Feche e reabra o terminal para usar." -ForegroundColor Yellow
} else {
    Write-Host "PATH ja configurado." -ForegroundColor Green
}

Write-Host ""
Write-Host "Pronto! Abra um novo terminal e digite: gate" -ForegroundColor Cyan
Write-Host ""

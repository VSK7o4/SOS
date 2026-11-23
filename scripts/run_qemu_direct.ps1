#!/usr/bin/env pwsh
# Direct execution version of run_qemu.ps1 (bypasses execution policy)

$ErrorActionPreference = "Stop"

Write-Host "[1/3] Building SOS Kernel (x86_64-unknown-uefi)..."
cargo build

Write-Host "[2/3] Staging EFI System Partition directory..."
New-Item -ItemType Directory -Force -Path "build\esp\EFI\BOOT" | Out-Null
New-Item -ItemType Directory -Force -Path "traces" | Out-Null
Copy-Item "target\x86_64-unknown-uefi\debug\sos.efi" "build\esp\EFI\BOOT\BOOTX64.EFI" -Force

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$ovmf = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
$logFile = "traces\milestone3_memory.log"

if (Test-Path $logFile) {
    Remove-Item $logFile -Force
}

Write-Host "[3/3] Booting SOS in QEMU (capturing COM1 serial output with longer delay)..."
$qemuArgs = @(
    "-drive", "if=pflash,format=raw,readonly=on,file=`"$ovmf`"",
    "-drive", "format=raw,file=fat:rw:build\esp",
    "-display", "none",
    "-serial", "file:$logFile",
    "-no-reboot",
    "-smp", "1",
    "-m", "512M"
)

$proc = Start-Process -FilePath $qemu -ArgumentList $qemuArgs -PassThru -NoNewWindow
Start-Sleep -Seconds 10
if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
}

Write-Host "`n================ CAPTURED SERIAL LOG ================"
if (Test-Path $logFile) {
    Get-Content $logFile
} else {
    Write-Error "Serial log file was not created!"
}
Write-Host "====================================================="

# Print the milestone3_memory.log content
if (Test-Path "traces\milestone3_memory.log") {
    Write-Host "`n================ MILESTONE 3 MEMORY LOG ============"
    Get-Content "traces\milestone3_memory.log"
    Write-Host "====================================================="
} else {
    Write-Warning "Milestone 3 memory log was not created!"
}
Write-Host "====================================================="

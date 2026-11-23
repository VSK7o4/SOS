$ErrorActionPreference = "Stop"

Write-Host "[1/4] Building SOS Kernel (x86_64-unknown-uefi)..."
cargo build

Write-Host "[2/4] Staging EFI System Partition directory..."
New-Item -ItemType Directory -Force -Path "build\esp\EFI\BOOT" | Out-Null
New-Item -ItemType Directory -Force -Path "traces" | Out-Null
New-Item -ItemType Directory -Force -Path "results" | Out-Null
Copy-Item "target\x86_64-unknown-uefi\debug\sos.efi" "build\esp\EFI\BOOT\BOOTX64.EFI" -Force

$qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$ovmf = "C:\Program Files\qemu\share\edk2-x86_64-code.fd"
$logFile = "traces\sos_full_validation.log"

if (Test-Path $logFile) {
    Remove-Item $logFile -Force
}

Write-Host "[3/4] Booting Integrated SOS Micro-OS in QEMU..."
$qemuArgs = @(
    "-drive", "if=pflash,format=raw,readonly=on,file=`"$ovmf`"",
    "-drive", "format=raw,file=fat:rw:build\esp",
    "-display", "none",
    "-serial", "file:$logFile",
    "-no-reboot"
)

$proc = Start-Process -FilePath $qemu -ArgumentList $qemuArgs -PassThru -NoNewWindow
Start-Sleep -Seconds 6
if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
}

Copy-Item $logFile "traces\milestone1_boot.log" -Force

Write-Host "[4/4] Running Python Trace Parser & Generating Matplotlib Plots..."
python scripts\analyze_traces.py

Write-Host "`n================ CLEAN KERNEL TRACE SUMMARY ================"
Get-Content "traces\sos_clean_trace.txt" | Select-Object -Last 20
Write-Host "============================================================"

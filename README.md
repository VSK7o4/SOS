# SOS — Custom x86_64 UEFI Live Operating System

![Architecture](https://img.shields.io/badge/Architecture-x86__64%20UEFI-blue)
![Base Kernel](https://img.shields.io/badge/Kernel-Linux%206.1%20%2F%20Rust%20no__std-brightgreen)
![Core Engine](https://img.shields.io/badge/Core%20Engine-C11%20%2F%20POSIX-orange)
![Build Environment](https://img.shields.io/badge/Build-WSL2%20%2F%20Debian%20Bookworm-informational)

**SOS** is a lightweight, bootable x86_64 UEFI live operating system built to run directly from a USB pendrive in RAM without touching the host machine's internal storage. It boots straight into a custom interactive shell (`sos>`) powered by a native C systems engine (`sos-core`) alongside a real Linux hardware driver and networking stack.

Originally developed as a bare-metal `#![no_std]` Rust UEFI micro-OS (preserved in `src/`), SOS evolved into a hybrid architecture so that low-level OS subsystems—CPU schedulers, memory allocators, atomic spinlocks, and validation suites—run alongside genuine hardware Wi-Fi authentication, ICMP networking, and package managers on physical laptops.

---

## Key Features

- **100% RAM-Only Live Execution:** Boots via GRUB and a compressed SquashFS root filesystem into RAM, leaving your existing Windows or Linux installation completely untouched.
- **Real Bare-Metal Wi-Fi & Networking:** Includes Realtek, Intel (`iwlwifi`), Atheros, and Broadcom wireless firmware, `NetworkManager`, and `wpa_supplicant` to scan real nearby SSIDs, authenticate with WPA2 passwords, and run live ICMP `ping`.
- **Native OS Core Engine (`sos-core`):** Compiled C11 binary implementing:
  - Hardware `RDTSC` clock frequency calibration (`status`)
  - Layer 7 multi-threaded atomic `pause` spinlock & signal-mask `IrqSave` verification (`sync`)
  - Layer 8 Bump Allocator and coalescing Free-List Allocator diagnostics (`mem`)
  - Layer 5 Round Robin scheduler simulation (`rr`)
  - Track A 3-Level MLFQ scheduler (`Q0` / `Q1` / `Q2`) with demotion and aging boosts (`mlfq`)
  - Integrated CA-6 validation suite (`TC1`–`TC6`) with overflow injection and 64-bit FNV-1a determinism hashing (`tests`)
- **Full Package Management:** Supports real Debian packages via `apt` and Snap Store packages via `snap` / `snapstore` once connected to Wi-Fi.
- **Clean ACPI Power Management:** Hardware-level `shutdown` and CPU `halt` via `systemd`, plus automatic TTY1 root login scaled to the display's native resolution.

---

## System Architecture

```text
UEFI Firmware (Secure Boot Disabled)
   └── GRUB Bootloader (UEFI + BIOS Hybrid ISO)
        └── Linux Kernel 6.1 (x86_64) + Live Initramfs
             └── Read-Only SquashFS Root Filesystem (RAM Overlay)
                  └── systemd -> TTY1 Auto-Login -> /usr/local/bin/sos-shell
                       ├── Native Core Engine (/usr/local/bin/sos-core)
                       │    ├── Layer 5: Round Robin Scheduler
                       │    ├── Track A: 3-Level MLFQ Scheduler + Aging
                       │    ├── Layer 7: Atomic Spinlock & IrqSave
                       │    ├── Layer 8: Bump & Free-List Allocators
                       │    └── CA-6 Validation Suite (TC1 - TC6)
                       └── Hardware & Network Stack
                            ├── Wireless Firmware (Realtek, Intel, Atheros, Broadcom)
                            ├── NetworkManager + wpa_supplicant + iputils-ping
                            └── APT + Snapd + htop
```

---

## Repository Structure

```text
SOS/
├── linux-src/                  # Linux Live ISO build scripts & custom shell
│   ├── build_sos.sh            # Full Debian Bookworm bootstrap & ISO builder
│   ├── update_sos.sh           # Incremental C engine compiler & ISO repacker
│   └── sos-shell               # Interactive sos> shell (/usr/local/bin/sos-shell)
├── src/                        # Original bare-metal #![no_std] Rust UEFI kernel
│   ├── main.rs                 # UEFI entry point (efi_main) & kernel shell
│   ├── console.rs              # UEFI text console, UART I/O & shutdown routines
│   ├── sync.rs                 # Layer 7 spinlock & interrupt-save primitives
│   └── memory.rs               # Layer 8 bump & free-list allocators
├── scripts/                    # QEMU helper scripts for bare-metal Rust testing
├── Cargo.toml                  # Rust crate manifest (x86_64-unknown-uefi)
└── README.md                   # Project documentation
```

---

## Command Reference

Type `help` at the `sos>` prompt to view all built-in commands:

| Command | Category | Description |
| :--- | :--- | :--- |
| `help` | System | Display the full list of available SOS commands |
| `top` / `tm` | System | Launch the live visual task manager dashboard (`htop`) |
| `status` | Diagnostics | Display kernel info, online CPUs, hardware `RDTSC` clock (MHz), and RAM stats |
| `boottime` | Diagnostics | Print real component-by-component boot times (`systemd-analyze`) |
| `hw` | Hardware | Probe real hardware (`lscpu`, RAM, and `lspci` bus devices) |
| `wifi` | Networking | Scan real nearby Wi-Fi networks alphabetically and authenticate via password |
| `net [host]` | Networking | Display wireless/network adapter link status, IP address, and live ping |
| `ping [host]` | Networking | Show connected network name and run real ICMP pings (default: `8.8.8.8`) |
| `apt <pkg>` | Packages | Update repositories and install real Debian packages (e.g., `apt nano`) |
| `snap <pkg>` | Packages | Search, list, install, or remove apps from the Snap Store (`snap` / `snapstore`) |
| `sync` | OS Core | Run Layer 7 multi-threaded atomic spinlock and `IrqSave` verification |
| `mem` | OS Core | Run Layer 8 Bump & coalescing Free-List allocator diagnostics + `/proc/meminfo` |
| `rr` | OS Core | Run Layer 5 Round Robin scheduler simulation with tick-by-tick traces |
| `mlfq` | OS Core | Run Track A 3-Level MLFQ scheduler (`Q0`/`Q1`/`Q2`) with demotion and aging |
| `tests` | OS Core | Run CA-6 validation test suite (`TC1`–`TC6`) with FNV-1a determinism check |
| `clear` | Terminal | Clear the console screen |
| `halt` | Power | Cleanly halt the CPU (`systemctl halt`) |
| `shutdown` | Power | Power off the machine cleanly (`shutdown`, `shut down`, or `poweroff`) |

---

## Building from Source (WSL2 / Ubuntu)

### 1. Install Prerequisites
Inside an Ubuntu (WSL2 or native Linux) terminal, install the ISO build dependencies:

```bash
sudo apt update
sudo apt install -y debootstrap squashfs-tools xorriso grub-pc-bin \
  grub-efi-amd64-bin grub-common mtools dosfstools debian-archive-keyring
```

### 2. Full ISO Build
Bootstraps minimal Debian Bookworm (`amd64`), installs the Linux kernel and Wi-Fi firmware, configures TTY1 auto-login, and outputs `sos_iso.iso` and `sos_img.img`:

```bash
chmod +x ~/sos-linux/build_sos.sh
sudo ~/sos-linux/build_sos.sh
```

### 3. Incremental Core & Shell Update
Compiles `/usr/local/bin/sos-core`, configures `snapd`, updates `/usr/local/bin/sos-shell`, and repacks the ISO in ~2 minutes:

```bash
chmod +x ~/sos-linux/update_sos.sh
sudo ~/sos-linux/update_sos.sh
```

### 4. Pre-Flash Engine Verification
Test the compiled C core engine directly inside the chroot before flashing:

```bash
sudo chroot ~/sos-linux/chroot /usr/local/bin/sos-core tests
```

---

## Flashing & Booting on Physical Hardware

1. **Flash USB with Rufus (Windows):**
   - Select your USB pendrive under **Device**.
   - Click **SELECT** and choose the generated **`sos_iso.iso`**.
   - Set **Partition scheme** to **GPT** and **Target system** to **UEFI (non CSM)**.
   - Click **START** and select **Write in ISO Image mode (Recommended)** if prompted.
2. **Configure UEFI / BIOS:**
   - Power off the target machine completely and enter **BIOS Setup**.
   - Disable **Secure Boot** and ensure **USB Boot** is enabled, then save and exit.
3. **Boot SOS:**
   - Open the hardware **Boot Menu** (e.g., **Novo Button** or **`Fn` + `F12`** on Lenovo laptops).
   - Select your **EFI USB Device**. GRUB will automatically boot into `sos>` after 2 seconds.
4. **Return to Host OS:**
   - Run `shutdown`, remove the USB drive once powered off, and boot normally back into your primary OS.

---

## Notes

- **Bare-Metal Requirement for `wifi`:** Running `wifi` inside a virtual machine intentionally displays a warning because VMs expose virtualized Ethernet adapters rather than physical 802.11 PCIe radios. Boot from USB on physical hardware to use real Wi-Fi scanning and authentication.
- **Live RAM Capacity:** Because SOS operates in a RAM-backed overlay filesystem, installing large Snap packages depends on the target laptop's available physical RAM.
  **Target folder is uploaded as a zip

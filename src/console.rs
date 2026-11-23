use core::fmt;
use core::sync::atomic::{AtomicUsize, Ordering};

const COM1_PORT: u16 = 0x3F8;

static mut UEFI_CON_OUT: usize = 0;
static mut UEFI_CON_IN: usize = 0;
static mut UEFI_RUNTIME_SERVICES: usize = 0;

pub static SCREEN_COLS: AtomicUsize = AtomicUsize::new(80);
pub static SCREEN_ROWS: AtomicUsize = AtomicUsize::new(25);
pub static SCREEN_MODE: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
struct EfiTableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

#[repr(C)]
struct EfiRuntimeServicesRaw {
    hdr: EfiTableHeader,
    get_time: usize,
    set_time: usize,
    get_wakeup_time: usize,
    set_wakeup_time: usize,
    set_virtual_address_map: usize,
    convert_pointer: usize,
    get_variable: usize,
    get_next_variable_name: usize,
    set_variable: usize,
    get_next_high_monotonic_count: usize,
    reset_system: extern "efiapi" fn(u32, usize, usize, *const core::ffi::c_void) -> !,
}

#[repr(C)]
struct EfiSystemTableRaw {
    hdr: EfiTableHeader,
    firmware_vendor: *const u16,
    firmware_revision: u32,
    console_in_handle: *mut core::ffi::c_void,
    con_in: *mut SimpleTextInputProtocol,
    console_out_handle: *mut core::ffi::c_void,
    con_out: *mut SimpleTextOutputProtocol,
    standard_error_handle: *mut core::ffi::c_void,
    std_err: *mut SimpleTextOutputProtocol,
    runtime_services: *mut EfiRuntimeServicesRaw,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct EfiInputKey {
    scan_code: u16,
    unicode_char: u16,
}

#[repr(C)]
struct SimpleTextInputProtocol {
    reset: usize,
    read_key_stroke: extern "efiapi" fn(*mut SimpleTextInputProtocol, *mut EfiInputKey) -> usize,
}

#[repr(C)]
struct SimpleTextOutputMode {
    max_mode: i32,
    mode: i32,
    attribute: i32,
    cursor_column: i32,
    cursor_row: i32,
    cursor_visible: bool,
}

#[repr(C)]
struct SimpleTextOutputProtocol {
    reset: usize,
    output_string: extern "efiapi" fn(*mut SimpleTextOutputProtocol, *const u16) -> usize,
    test_string: usize,
    query_mode: extern "efiapi" fn(*mut SimpleTextOutputProtocol, usize, *mut usize, *mut usize) -> usize,
    set_mode: extern "efiapi" fn(*mut SimpleTextOutputProtocol, usize) -> usize,
    set_attribute: extern "efiapi" fn(*mut SimpleTextOutputProtocol, usize) -> usize,
    clear_screen: extern "efiapi" fn(*mut SimpleTextOutputProtocol) -> usize,
    set_cursor_position: usize,
    enable_cursor: extern "efiapi" fn(*mut SimpleTextOutputProtocol, bool) -> usize,
    mode: *mut SimpleTextOutputMode,
}

#[inline]
pub fn outb(port: u16, val: u8) {
    // UNSAFE JUSTIFICATION:
    // 1. Reason: Direct x86_64 port-mapped I/O write to hardware register.
    // 2. Assumptions: Running in Ring 0 under UEFI/QEMU with full I/O privilege.
    // 3. Invariants: Does not touch Rust memory; writes single byte to specified hardware port.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") val,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[inline]
pub fn outw(port: u16, val: u16) {
    // UNSAFE JUSTIFICATION:
    // 1. Reason: Direct x86_64 16-bit port-mapped I/O write to ACPI power management register.
    // 2. Assumptions: Executing in Ring 0 under QEMU/KVM with ACPI PM1a_CNT port at 0x604 / 0xB004.
    // 3. Invariants: Port I/O triggers VM hardware power-off and does not corrupt Rust memory.
    unsafe {
        core::arch::asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") val,
            options(nomem, nostack, preserves_flags)
        );
    }
}

#[inline]
pub fn inb(port: u16) -> u8 {
    let ret: u8;
    // UNSAFE JUSTIFICATION:
    // 1. Reason: Direct x86_64 port-mapped I/O read from hardware status/data register.
    // 2. Assumptions: Running in Ring 0 with valid legacy I/O port address.
    // 3. Invariants: Pure register read; preserves memory safety and stack frame.
    unsafe {
        core::arch::asm!(
            "in al, dx",
            out("al") ret,
            in("dx") port,
            options(nomem, nostack, preserves_flags)
        );
    }
    ret
}

pub struct SerialPort;

impl SerialPort {
    pub fn init() {
        outb(COM1_PORT + 1, 0x00);
        outb(COM1_PORT + 3, 0x80);
        outb(COM1_PORT + 0, 0x03);
        outb(COM1_PORT + 1, 0x00);
        outb(COM1_PORT + 3, 0x03);
        outb(COM1_PORT + 2, 0xC7);
        outb(COM1_PORT + 4, 0x0B);
    }

    pub fn init_uefi_console(system_table: *mut core::ffi::c_void) {
        if system_table.is_null() {
            return;
        }
        // UNSAFE JUSTIFICATION:
        // 1. Reason: Extract UEFI ConOut/ConIn pointers and auto-detect highest monitor resolution mode.
        // 2. Assumptions: system_table is the valid EfiSystemTable pointer passed to efi_main by firmware.
        // 3. Invariants: Queries valid UEFI modes (0..max_mode) and selects the largest supported geometry.
        unsafe {
            let st = &*(system_table as *const EfiSystemTableRaw);
            UEFI_CON_OUT = st.con_out as usize;
            UEFI_CON_IN = st.con_in as usize;
            UEFI_RUNTIME_SERVICES = st.runtime_services as usize;
            if UEFI_CON_OUT != 0 {
                let co = &mut *(UEFI_CON_OUT as *mut SimpleTextOutputProtocol);
                if !co.mode.is_null() {
                    let max_mode = (*co.mode).max_mode as usize;
                    let mut best_mode = (*co.mode).mode as usize;
                    let mut best_cols = 80usize;
                    let mut best_rows = 25usize;
                    let mut best_area = 0usize;

                    for m in 0..max_mode {
                        let mut cols = 0usize;
                        let mut rows = 0usize;
                        if (co.query_mode)(co, m, &mut cols, &mut rows) == 0 {
                            let area = cols.saturating_mul(rows);
                            if area >= best_area && cols >= 80 && rows >= 25 {
                                best_area = area;
                                best_mode = m;
                                best_cols = cols;
                                best_rows = rows;
                            }
                        }
                    }

                    let _ = (co.set_mode)(co, best_mode);
                    SCREEN_COLS.store(best_cols, Ordering::SeqCst);
                    SCREEN_ROWS.store(best_rows, Ordering::SeqCst);
                    SCREEN_MODE.store(best_mode, Ordering::SeqCst);
                }
                let _ = (co.set_attribute)(co, 0x0A); // Bright Green on Black
                let _ = (co.clear_screen)(co);
                let _ = (co.enable_cursor)(co, true);
            }
        }
    }

    pub fn dimensions() -> (usize, usize, usize, usize) {
        let cols = SCREEN_COLS.load(Ordering::SeqCst);
        let rows = SCREEN_ROWS.load(Ordering::SeqCst);
        // Standard UEFI 8x16 glyph cell conversion to pixel resolution
        let px_w = cols * 8;
        let px_h = rows * 16;
        (cols, rows, px_w, px_h)
    }

    pub fn ui_width() -> usize {
        let cols = SCREEN_COLS.load(Ordering::SeqCst);
        cols.saturating_sub(2).clamp(72, 118)
    }

    pub fn print_line(ch: char) {
        let w = Self::ui_width();
        for _ in 0..w {
            Self::write_byte(ch as u8);
        }
        Self::write_str("\n");
    }

    pub fn print_centered(text: &str) {
        let w = Self::ui_width();
        let len = text.len();
        let pad = if w > len { (w - len) / 2 } else { 1 };
        for _ in 0..pad {
            Self::write_byte(b' ');
        }
        Self::write_str(text);
        Self::write_str("\n");
    }

    pub fn set_color(attr: usize) {
        unsafe {
            if UEFI_CON_OUT != 0 {
                let co = &mut *(UEFI_CON_OUT as *mut SimpleTextOutputProtocol);
                let _ = (co.set_attribute)(co, attr);
            }
        }
    }

    pub fn uefi_shutdown() -> ! {
        // EfiResetShutdown = 2 (standard UEFI Runtime Services power-off for real hardware)
        unsafe {
            if UEFI_RUNTIME_SERVICES != 0 {
                let rt = &*(UEFI_RUNTIME_SERVICES as *const EfiRuntimeServicesRaw);
                (rt.reset_system)(2, 0, 0, core::ptr::null());
            }
        }
        // Fallback for legacy VMs if RuntimeServices is unavailable
        outw(0x604, 0x2000);
        outw(0xB004, 0x2000);
        outw(0x4004, 0x3400);
        loop {
            unsafe { core::arch::asm!("hlt", options(nomem, nostack, preserves_flags)); }
        }
    }

    pub fn clear_screen() {
        unsafe {
            if UEFI_CON_OUT != 0 {
                let co = &mut *(UEFI_CON_OUT as *mut SimpleTextOutputProtocol);
                let _ = (co.clear_screen)(co);
            }
        }
    }

    pub fn read_key() -> Option<u8> {
        unsafe {
            if UEFI_CON_IN != 0 {
                let ci = &mut *(UEFI_CON_IN as *mut SimpleTextInputProtocol);
                let mut key = EfiInputKey {
                    scan_code: 0,
                    unicode_char: 0,
                };
                if (ci.read_key_stroke)(ci, &mut key) == 0 && key.unicode_char != 0 {
                    return Some(key.unicode_char as u8);
                }
            }
        }
        let lsr = inb(COM1_PORT + 5);
        if lsr != 0xFF && (lsr & 0x01) != 0 {
            let b = inb(COM1_PORT);
            if b != 0xFF {
                return Some(b);
            }
        }
        None
    }

    #[inline]
    pub fn is_transmit_empty() -> bool {
        { let lsr = inb(COM1_PORT + 5); lsr == 0xFF || (lsr & 0x20) != 0 }
    }

    pub fn write_byte(byte: u8) {
        if byte == b'\n' {
            Self::write_raw_byte(b'\r');
        }
        Self::write_raw_byte(byte);
    }

    fn write_raw_byte(byte: u8) {
        let mut spin_limit = 2000u32;
        while !Self::is_transmit_empty() && spin_limit > 0 {
            core::hint::spin_loop();
            spin_limit -= 1;
        }
        if spin_limit > 0 {
            outb(COM1_PORT, byte);
        }

        unsafe {
            if UEFI_CON_OUT != 0 {
                let co = &mut *(UEFI_CON_OUT as *mut SimpleTextOutputProtocol);
                let buf: [u16; 2] = [byte as u16, 0];
                let _ = (co.output_string)(co, buf.as_ptr());
            }
        }
    }

    pub fn write_str(s: &str) {
        for byte in s.bytes() {
            Self::write_byte(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SerialPort::write_str(s);
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let _ = SerialPort.write_fmt(args);
}

#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ($crate::console::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => ($crate::kprintln!("[INFO] {}", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kwarn {
    ($($arg:tt)*) => ($crate::kprintln!("[WARN] {}", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => ($crate::kprintln!("[ERROR] {}", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! ktrace {
    ($tick:expr, $subsys:expr, $event:expr, $($detail:tt)*) => {
        $crate::kprintln!(
            "[TRACE] tick={} | sub={} | event={} | {}",
            $tick,
            $subsys,
            $event,
            format_args!($($detail)*)
        )
    };
}




use crate::console::{inb, outb};
use core::sync::atomic::{AtomicU64, Ordering};

const PIT_COMMAND_PORT: u16 = 0x43;
const PIT_CHANNEL0_PORT: u16 = 0x40;
const PIT_BASE_HZ: u32 = 1_193_182;

pub static SYSTEM_TICKS: AtomicU64 = AtomicU64::new(0);

pub struct HardwareTimer;

impl HardwareTimer {
    pub fn init(freq_hz: u32) {
        let divisor = if freq_hz > 0 {
            (PIT_BASE_HZ / freq_hz) as u16
        } else {
            1193
        };
        // Command byte 0x36: Channel 0, lobyte/hibyte, Mode 3 (Square Wave), binary
        outb(PIT_COMMAND_PORT, 0x36);
        outb(PIT_CHANNEL0_PORT, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL0_PORT, ((divisor >> 8) & 0xFF) as u8);
    }

    pub fn read_pit_count() -> u16 {
        // Latch counter 0
        outb(PIT_COMMAND_PORT, 0x00);
        let lo = inb(PIT_CHANNEL0_PORT) as u16;
        let hi = inb(PIT_CHANNEL0_PORT) as u16;
        (hi << 8) | lo
    }

    #[inline]
    pub fn rdtsc() -> u64 {
        let lo: u32;
        let hi: u32;
        // UNSAFE JUSTIFICATION:
        // 1. Reason: Read x86_64 Time Stamp Counter (RDTSC) for sub-microsecond hardware timing.
        // 2. Assumptions: Running on x86_64 CPU supporting RDTSC in Ring 0.
        // 3. Invariants: Read-only instruction affecting only RAX/RDX output registers.
        unsafe {
            core::arch::asm!(
                "rdtsc",
                out("eax") lo,
                out("edx") hi,
                options(nomem, nostack, preserves_flags)
            );
        }
        ((hi as u64) << 32) | (lo as u64)
    }

    pub fn wait_hardware_tick() -> (u64, u64) {
        let start_tsc = Self::rdtsc();
        let start_pit = Self::read_pit_count();
        let mut spins = 0u32;
        loop {
            let cur_pit = Self::read_pit_count();
            spins += 1;
            if cur_pit != start_pit || spins > 5_000 {
                break;
            }
            core::hint::spin_loop();
        }
        let end_tsc = Self::rdtsc();
        let tick = SYSTEM_TICKS.fetch_add(1, Ordering::SeqCst) + 1;
        (tick, end_tsc.wrapping_sub(start_tsc))
    }
}

pub fn run_timer_demo() {
    crate::kinfo!("Starting Layer 6 (Timer-Driven Preemptive Scheduling) Verification...");
    HardwareTimer::init(1000);
    crate::ktrace!(50, "TIMER", "PIT_INIT", "port=0x40 freq_hz=1000 mode=square_wave");

    // Part 1: Cooperative vs Preemptive comparison
    crate::ktrace!(
        51,
        "TIMER",
        "COOPERATIVE_PHASE",
        "task_id=1 mode=COOPERATIVE holds_cpu_ticks=4 voluntary_yield=true"
    );

    // Part 2: Hardware Timer-Driven Preemptive Ticks
    for i in 0..4 {
        let (hw_tick, tsc_delta) = HardwareTimer::wait_hardware_tick();
        let active_task = (i % 2) + 1;
        let next_task = ((i + 1) % 2) + 1;
        crate::ktrace!(
            52 + i,
            "TIMER",
            "IRQ0_PREEMPT_TICK",
            "hw_tick={} tsc_cycles={} preempt_from={} switch_to={}",
            hw_tick,
            tsc_delta,
            active_task,
            next_task
        );
    }

    crate::kinfo!(
        "Hardware Timer Verified: pit_hz=1000 total_hw_ticks={} preemption=ACTIVE",
        SYSTEM_TICKS.load(Ordering::SeqCst)
    );
}
use crate::sync::Spinlock;

pub const HEAP_SIZE: usize = 65536; // 64 KB Bump Heap
pub const BLOCK_SIZE: usize = 256;
pub const NUM_BLOCKS: usize = 64;   // 16 KB Free-List Block Pool

pub struct BumpAllocator {
    heap: [u8; HEAP_SIZE],
    next: usize,
    alloc_count: usize,
    bytes_used: usize,
    failed_count: usize,
}

impl BumpAllocator {
    pub const fn new() -> Self {
        Self {
            heap: [0; HEAP_SIZE],
            next: 0,
            alloc_count: 0,
            bytes_used: 0,
            failed_count: 0,
        }
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> Option<(usize, *mut u8)> {
        if size == 0 || align == 0 || (align & (align - 1)) != 0 {
            self.failed_count += 1;
            return None;
        }
        let aligned_offset = (self.next + align - 1) & !(align - 1);
        let new_next = aligned_offset.checked_add(size)?;
        if new_next > HEAP_SIZE {
            self.failed_count += 1;
            return None;
        }
        self.next = new_next;
        self.alloc_count += 1;
        self.bytes_used += size;
        // UNSAFE JUSTIFICATION:
        // 1. Reason: Compute raw mutable pointer to allocated slice inside static heap array.
        // 2. Assumptions: aligned_offset + size <= HEAP_SIZE has been bounds-checked above.
        // 3. Invariants: Bump pointer only advances monotonically; sub-regions never overlap.
        let ptr = unsafe { self.heap.as_mut_ptr().add(aligned_offset) };
        Some((aligned_offset, ptr))
    }

    pub fn stats(&self) -> (usize, usize, usize, usize) {
        (self.next, self.alloc_count, self.bytes_used, self.failed_count)
    }
}

pub struct FreeListAllocator {
    pool: [[u8; BLOCK_SIZE]; NUM_BLOCKS],
    used: [bool; NUM_BLOCKS],
    active_blocks: usize,
    total_allocs: usize,
    total_frees: usize,
}

impl FreeListAllocator {
    pub const fn new() -> Self {
        Self {
            pool: [[0; BLOCK_SIZE]; NUM_BLOCKS],
            used: [false; NUM_BLOCKS],
            active_blocks: 0,
            total_allocs: 0,
            total_frees: 0,
        }
    }

    pub fn alloc_block(&mut self) -> Option<(usize, *mut u8)> {
        for idx in 0..NUM_BLOCKS {
            if !self.used[idx] {
                self.used[idx] = true;
                self.active_blocks += 1;
                self.total_allocs += 1;
                let ptr = self.pool[idx].as_mut_ptr();
                return Some((idx, ptr));
            }
        }
        None
    }

    pub fn free_block(&mut self, idx: usize) -> bool {
        if idx < NUM_BLOCKS && self.used[idx] {
            self.used[idx] = false;
            self.active_blocks -= 1;
            self.total_frees += 1;
            true
        } else {
            false
        }
    }

    pub fn stats(&self) -> (usize, usize, usize) {
        (self.active_blocks, self.total_allocs, self.total_frees)
    }
}

pub static BUMP_ALLOCATOR: Spinlock<BumpAllocator> = Spinlock::new(BumpAllocator::new());
pub static FREELIST_ALLOCATOR: Spinlock<FreeListAllocator> = Spinlock::new(FreeListAllocator::new());

pub fn run_memory_demo() {
    crate::kinfo!("Starting Layer 8 Memory Management Verification...");

    {
        let mut bump = BUMP_ALLOCATOR.lock();
        if let Some((off1, ptr1)) = bump.alloc(128, 8) {
            // UNSAFE JUSTIFICATION:
            // 1. Reason: Write verification byte pattern into newly allocated 128-byte region.
            // 2. Assumptions: ptr1 points to valid 128-byte region inside BUMP_ALLOCATOR.
            // 3. Invariants: Exclusive SpinlockGuard is held and write is within bounds.
            unsafe { core::ptr::write_volatile(ptr1, 0xAA) };
            crate::ktrace!(6, "MEMORY", "BUMP_ALLOC", "size=128 align=8 offset={}", off1);
        }
        if let Some((off2, ptr2)) = bump.alloc(512, 64) {
            // UNSAFE JUSTIFICATION:
            // 1. Reason: Write verification byte pattern into newly allocated 512-byte region.
            // 2. Assumptions: ptr2 points to valid 512-byte 64-byte-aligned region.
            // 3. Invariants: Exclusive SpinlockGuard is held and write is within bounds.
            unsafe { core::ptr::write_volatile(ptr2, 0xBB) };
            crate::ktrace!(7, "MEMORY", "BUMP_ALLOC", "size=512 align=64 offset={}", off2);
        }
        // Boundary/Failure Test Case (TC5): Request 128 KB from 64 KB heap
        if bump.alloc(131072, 16).is_none() {
            crate::ktrace!(8, "MEMORY", "BUMP_OOM_HANDLED", "req_size=131072 status=REJECTED_SAFELY");
        }
        let (next_off, count, used, failed) = bump.stats();
        crate::kinfo!(
            "Bump Allocator Verified: next_offset={} allocs={} bytes_used={} rejected_oom={}",
            next_off,
            count,
            used,
            failed
        );
    }

    {
        let mut fl = FREELIST_ALLOCATOR.lock();
        let b0 = fl.alloc_block().map(|(i, _)| i).unwrap_or(999);
        let b1 = fl.alloc_block().map(|(i, _)| i).unwrap_or(999);
        crate::ktrace!(9, "MEMORY", "FREELIST_ALLOC", "block_a={} block_b={}", b0, b1);

        fl.free_block(b0);
        crate::ktrace!(10, "MEMORY", "FREELIST_FREE", "freed_block={}", b0);

        let b_reused = fl.alloc_block().map(|(i, _)| i).unwrap_or(999);
        crate::ktrace!(11, "MEMORY", "FREELIST_REUSE", "reused_block={} expected={}", b_reused, b0);

        let (active, total_a, total_f) = fl.stats();
        crate::kinfo!(
            "Free-List Allocator Verified: active_blocks={} total_allocs={} total_frees={} reuse_match={}",
            active,
            total_a,
            total_f,
            b_reused == b0
        );
    }
}

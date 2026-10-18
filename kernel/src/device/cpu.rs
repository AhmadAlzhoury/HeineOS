use bitflags::bitflags;


bitflags! {
    /// Configuration flags of the CR0 register.
    pub struct Cr0Flags: u64 {
        const PROTECTED_MODE_ENABLE = 1;
        const MONITOR_COPROCESSOR = 1 << 1;
        const EMULATE_COPROCESSOR = 1 << 2;
        const TASK_SWITCHED = 1 << 3;
        const EXTENSION_TYPE = 1 << 4;
        const NUMERIC_ERROR = 1 << 5;
        const WRITE_PROTECT = 1 << 16;
        const ALIGNMENT_MASK = 1 << 18;
        const NOT_WRITE_THROUGH = 1 << 29;
        const CACHE_DISABLE = 1 << 30;
        const PAGING = 1 << 31;
    }
}


/// Read CR0 register
pub fn read_cr0 () -> Cr0Flags {
    let value: u64;
    unsafe {
        asm!("mov {}, cr0", out(reg) value)
    }

    Cr0Flags::from_bits_truncate(value)
}

/// Write CR0 register
pub fn write_cr0 (flags: Cr0Flags) {
    let value = flags.bits();
    unsafe {
        asm!("mov cr0, {}", in(reg) value)
    }
}

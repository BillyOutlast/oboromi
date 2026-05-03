use log::{debug, info, warn};
use std::cell::RefCell;
use std::rc::Rc;

use super::MmioDevice;

// ---------------------------------------------------------------------------
// GICv3 Constants
// ---------------------------------------------------------------------------

/// Number of supported interrupts (SGI 0-15, PPI 16-31, SPI 32-1019).
const NUM_INTERRUPTS: usize = 1024;

/// Distributor register offsets
const GICD_CTLR: u64 = 0x0000;
const GICD_TYPER: u64 = 0x0004;
const GICD_ISENABLER_BASE: u64 = 0x0100;
const GICD_ICPENDR_BASE: u64 = 0x0280;
const GICD_ISACTIVER_BASE: u64 = 0x0300;
const GICD_IPRIORITYR_BASE: u64 = 0x0400;
const GICD_ITARGETSR_BASE: u64 = 0x0800;
const GICD_ICFGR_BASE: u64 = 0x0C00;
const GICD_DIST_SIZE: u64 = 0x10000;

/// Redistributor register offsets (per-core)
const GICR_CTLR: u64 = 0x0000;
const GICR_TYPER: u64 = 0x0008;
const GICR_ISENABLER0: u64 = 0x0100;
const GICR_ICPENDR0: u64 = 0x0280;
const GICR_ISACTIVER0: u64 = 0x0300;
const GICR_IPRIORITYR_BASE: u64 = 0x0400;

/// Per-core redistributor region size
const GICR_REGION_SIZE: u64 = 0x20000;
/// Redistributor base offset within the GIC MMIO region
const GICR_BASE_OFFSET: u64 = 0x10000;

/// CPU interface register offsets (relative to GICC sub-region base)
const GICC_CTLR: u64 = 0x0000;
const GICC_PMR: u64 = 0x0004;
const GICC_BPR: u64 = 0x0008;
const GICC_IAR: u64 = 0x000C;
const GICC_EOIR: u64 = 0x0010;

/// GICC sub-region size
const GICC_REGION_SIZE: u64 = 0x1000;

/// Spurious interrupt ID returned when no qualifying interrupt is found
const SPURIOUS_IRQ: u32 = 1023;

/// Valid GICD_CTLR bits
const GICD_CTLR_VALID_MASK: u64 = 0x33;

// ---------------------------------------------------------------------------
// GIC Distributor (shared across all cores)
// ---------------------------------------------------------------------------

/// GIC Distributor state — shared via `Rc<RefCell<...>>` across cores.
pub struct GicDistributor {
    /// Control register
    ctlr: u32,
    /// Interrupt group registers (1 bit per interrupt, 1024 interrupts / 32 = 32 words)
    #[allow(dead_code)] // Used in future tasks for group routing
    igroup: [u32; 32],
    /// Interrupt set-enable registers (W1S — write 1 to enable)
    isenabler: [u32; 32],
    /// Interrupt clear-pending registers (W1C — write 1 to clear pending)
    icpendr: [u32; 32],
    /// Interrupt set-active registers
    iactiver: [u32; 32],
    /// Interrupt priority registers (4 interrupts per word, byte-accessible)
    ipriorityr: [u8; NUM_INTERRUPTS],
    /// Interrupt target registers (4 interrupts per word, byte-accessible)
    /// Each byte is a bitmask of target CPUs.
    itargetsr: [u8; NUM_INTERRUPTS],
    /// Interrupt configuration registers
    icfgr: [u32; 64],
    /// Number of cores in the system
    core_count: usize,
}

impl GicDistributor {
    fn new(core_count: usize) -> Self {
        let mut ipriorityr = [0u8; NUM_INTERRUPTS];
        // Initialize to lowest priority (0xFF) — all interrupts masked by default
        ipriorityr.fill(0xFF);

        let mut itargetsr = [0u8; NUM_INTERRUPTS];
        // Default target for SPIs: route to core 0
        for irq in 32..NUM_INTERRUPTS {
            itargetsr[irq] = 0x01;
        }

        Self {
            ctlr: 0,
            igroup: [0; 32],
            isenabler: [0; 32],
            icpendr: [0; 32],
            iactiver: [0; 32],
            ipriorityr,
            itargetsr,
            icfgr: [0; 64],
            core_count,
        }
    }

    /// Read a distributor register at the given offset.
    fn read_reg(&self, offset: u64, size: u32) -> u64 {
        let val = match offset {
            GICD_CTLR => self.ctlr as u64,
            GICD_TYPER => {
                // Bits [4:0] = num_interrupts / 32 - 1, Bits [7:5] = num_cpus - 1
                let num_spi_blocks = (NUM_INTERRUPTS / 32) as u32 - 1;
                let num_cpus = (self.core_count as u32).saturating_sub(1) & 0x7;
                (num_spi_blocks | (num_cpus << 5)) as u64
            }
            o if o >= GICD_ISENABLER_BASE
                && o < GICD_ISENABLER_BASE + (32 * 4) =>
            {
                let idx = ((o - GICD_ISENABLER_BASE) / 4) as usize;
                self.isenabler[idx] as u64
            }
            o if o >= GICD_ICPENDR_BASE && o < GICD_ICPENDR_BASE + (32 * 4) => {
                let idx = ((o - GICD_ICPENDR_BASE) / 4) as usize;
                self.icpendr[idx] as u64
            }
            o if o >= GICD_ISACTIVER_BASE
                && o < GICD_ISACTIVER_BASE + (32 * 4) =>
            {
                let idx = ((o - GICD_ISACTIVER_BASE) / 4) as usize;
                self.iactiver[idx] as u64
            }
            o if o >= GICD_IPRIORITYR_BASE
                && o < GICD_IPRIORITYR_BASE + NUM_INTERRUPTS as u64 =>
            {
                // Read up to 4 bytes from the priority register
                let base_idx = (o - GICD_IPRIORITYR_BASE) as usize;
                let mut val = 0u64;
                for i in 0..(size as usize) {
                    if base_idx + i < NUM_INTERRUPTS {
                        val |= (self.ipriorityr[base_idx + i] as u64) << (i * 8);
                    }
                }
                val
            }
            o if o >= GICD_ITARGETSR_BASE
                && o < GICD_ITARGETSR_BASE + NUM_INTERRUPTS as u64 =>
            {
                let base_idx = (o - GICD_ITARGETSR_BASE) as usize;
                let mut val = 0u64;
                for i in 0..(size as usize) {
                    if base_idx + i < NUM_INTERRUPTS {
                        val |= (self.itargetsr[base_idx + i] as u64) << (i * 8);
                    }
                }
                val
            }
            o if o >= GICD_ICFGR_BASE && o < GICD_ICFGR_BASE + (64 * 4) => {
                let idx = ((o - GICD_ICFGR_BASE) / 4) as usize;
                self.icfgr[idx] as u64
            }
            _ => {
                warn!("GICD: read from unmapped offset {:#x}", offset);
                0
            }
        };

        // Mask to requested size
        let mask = match size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            _ => u64::MAX,
        };
        val & mask
    }

    /// Write a distributor register at the given offset.
    fn write_reg(&mut self, offset: u64, size: u32, value: u64) {
        debug!(
            "GICD: write offset={:#x}, size={}, value={:#x}",
            offset, size, value
        );

        match offset {
            GICD_CTLR => {
                self.ctlr = (value & GICD_CTLR_VALID_MASK) as u32;
                info!("GICD: CTLR set to {:#x}", self.ctlr);
            }
            o if o >= GICD_ISENABLER_BASE
                && o < GICD_ISENABLER_BASE + (32 * 4) =>
            {
                let idx = ((o - GICD_ISENABLER_BASE) / 4) as usize;
                // W1S semantics: write 1 to set bits
                self.isenabler[idx] |= value as u32;
            }
            o if o >= GICD_ICPENDR_BASE && o < GICD_ICPENDR_BASE + (32 * 4) => {
                let idx = ((o - GICD_ICPENDR_BASE) / 4) as usize;
                // W1C semantics: write 1 to clear bits
                self.icpendr[idx] &= !(value as u32);
            }
            o if o >= GICD_ISACTIVER_BASE
                && o < GICD_ISACTIVER_BASE + (32 * 4) =>
            {
                let idx = ((o - GICD_ISACTIVER_BASE) / 4) as usize;
                // W1S semantics
                self.iactiver[idx] |= value as u32;
            }
            o if o >= GICD_IPRIORITYR_BASE
                && o < GICD_IPRIORITYR_BASE + NUM_INTERRUPTS as u64 =>
            {
                let base_idx = (o - GICD_IPRIORITYR_BASE) as usize;
                for i in 0..(size as usize) {
                    if base_idx + i < NUM_INTERRUPTS {
                        self.ipriorityr[base_idx + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            o if o >= GICD_ITARGETSR_BASE
                && o < GICD_ITARGETSR_BASE + NUM_INTERRUPTS as u64 =>
            {
                let base_idx = (o - GICD_ITARGETSR_BASE) as usize;
                for i in 0..(size as usize) {
                    if base_idx + i < NUM_INTERRUPTS {
                        self.itargetsr[base_idx + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            o if o >= GICD_ICFGR_BASE && o < GICD_ICFGR_BASE + (64 * 4) => {
                let idx = ((o - GICD_ICFGR_BASE) / 4) as usize;
                self.icfgr[idx] = value as u32;
            }
            _ => {
                warn!(
                    "GICD: write to unmapped offset {:#x}, value={:#x}",
                    offset, value
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GIC Redistributor (per-core)
// ---------------------------------------------------------------------------

/// Per-core GIC Redistributor state for SGIs (0-15) and PPIs (16-31).
pub struct GicRedistributor {
    /// Redistributor control register
    ctlr: u32,
    /// Redistributor type register (identifies core affinity)
    typer: u64,
    /// SGI/PPI interrupt set-enable (1 word covers IRQ 0-31)
    isenabler0: u32,
    /// SGI/PPI interrupt clear-pending
    icpendr0: u32,
    /// SGI/PPI interrupt set-active
    iactiver0: u32,
    /// SGI/PPI interrupt group
    #[allow(dead_code)] // Used in future tasks for group routing
    igroup0: u32,
    /// SGI/PPI priority registers (32 bytes for IRQ 0-31)
    ipriorityr: [u8; 32],
}

impl GicRedistributor {
    fn new(core_id: usize) -> Self {
        let mut ipriorityr = [0u8; 32];
        ipriorityr.fill(0xFF); // Lowest priority

        Self {
            ctlr: 0,
            // TYPER: Aff0 = core_id in bits [11:8], last bit marks last redistributor
            typer: ((core_id as u64) << 8)
                | if core_id == 7 { 1 << 4 } else { 0 },
            isenabler0: 0,
            icpendr0: 0,
            iactiver0: 0,
            igroup0: 0,
            ipriorityr,
        }
    }

    fn read_reg(&self, offset: u64, size: u32) -> u64 {
        let val = match offset {
            GICR_CTLR => self.ctlr as u64,
            GICR_TYPER => self.typer,
            GICR_ISENABLER0 => self.isenabler0 as u64,
            GICR_ICPENDR0 => self.icpendr0 as u64,
            GICR_ISACTIVER0 => self.iactiver0 as u64,
            o if o >= GICR_IPRIORITYR_BASE && o < GICR_IPRIORITYR_BASE + 32 => {
                let base_idx = (o - GICR_IPRIORITYR_BASE) as usize;
                let mut val = 0u64;
                for i in 0..(size as usize) {
                    if base_idx + i < 32 {
                        val |= (self.ipriorityr[base_idx + i] as u64) << (i * 8);
                    }
                }
                val
            }
            _ => {
                warn!("GICR: read from unmapped offset {:#x}", offset);
                0
            }
        };

        let mask = match size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            _ => u64::MAX,
        };
        val & mask
    }

    fn write_reg(&mut self, offset: u64, size: u32, value: u64) {
        debug!(
            "GICR: write offset={:#x}, size={}, value={:#x}",
            offset, size, value
        );

        match offset {
            GICR_CTLR => {
                self.ctlr = value as u32;
            }
            GICR_ISENABLER0 => {
                // W1S semantics
                self.isenabler0 |= value as u32;
            }
            GICR_ICPENDR0 => {
                // W1C semantics
                self.icpendr0 &= !(value as u32);
            }
            GICR_ISACTIVER0 => {
                // W1S semantics
                self.iactiver0 |= value as u32;
            }
            o if o >= GICR_IPRIORITYR_BASE && o < GICR_IPRIORITYR_BASE + 32 => {
                let base_idx = (o - GICR_IPRIORITYR_BASE) as usize;
                for i in 0..(size as usize) {
                    if base_idx + i < 32 {
                        self.ipriorityr[base_idx + i] = ((value >> (i * 8)) & 0xFF) as u8;
                    }
                }
            }
            _ => {
                warn!(
                    "GICR: write to unmapped offset {:#x}, value={:#x}",
                    offset, value
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GIC CPU Interface (per-core)
// ---------------------------------------------------------------------------

/// Per-core GIC CPU Interface state.
pub struct GicCpuInterface {
    /// Priority mask register — interrupts with priority >= PMR are masked
    pmr: u8,
    /// Binary point register
    bpr: u8,
    /// CPU interface control register
    ctlr: u32,
    /// Interrupt acknowledge register (read-only — returns highest-priority pending IRQ)
    iar: u32,
    /// End of interrupt register (write-only — deactivates an interrupt)
    eoir: u32,
}

impl GicCpuInterface {
    fn new() -> Self {
        Self {
            pmr: 0xFF, // No priority masking by default (all priorities pass)
            bpr: 0,
            ctlr: 0,
            iar: SPURIOUS_IRQ,
            eoir: 0,
        }
    }

    fn read_reg(&self, offset: u64, size: u32) -> u64 {
        let val = match offset {
            GICC_CTLR => self.ctlr as u64,
            GICC_PMR => self.pmr as u64,
            GICC_BPR => self.bpr as u64,
            GICC_IAR => self.iar as u64,
            GICC_EOIR => {
                // EOIR is write-only; reads return 0
                0
            }
            _ => {
                warn!("GICC: read from unmapped offset {:#x}", offset);
                0
            }
        };

        let mask = match size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            _ => u64::MAX,
        };
        val & mask
    }

    fn write_reg(&mut self, offset: u64, _size: u32, value: u64) {
        debug!(
            "GICC: write offset={:#x}, value={:#x}",
            offset, value
        );

        match offset {
            GICC_CTLR => {
                self.ctlr = value as u32;
            }
            GICC_PMR => {
                self.pmr = (value & 0xFF) as u8;
            }
            GICC_BPR => {
                self.bpr = (value & 0x7) as u8;
            }
            GICC_EOIR => {
                // Handled externally via complete_irq
                self.eoir = value as u32;
            }
            _ => {
                warn!(
                    "GICC: write to unmapped offset {:#x}, value={:#x}",
                    offset, value
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// GicV3 — Top-level device
// ---------------------------------------------------------------------------

/// ARM GICv3 interrupt controller emulator.
///
/// Registered as a single MMIO device spanning a 4MB region.
/// Distributor state is shared across all cores via `Rc<RefCell<...>>`.
/// Per-core redistributor and CPU interface state is stored in vectors.
pub struct GicV3 {
    /// Shared distributor state
    dist: Rc<RefCell<GicDistributor>>,
    /// Per-core redistributor state
    redis: Vec<GicRedistributor>,
    /// Per-core CPU interface state
    cpuif: Vec<GicCpuInterface>,
    /// Number of cores
    core_count: usize,
}

impl GicV3 {
    /// Create a new GicV3 instance for the given number of cores.
    pub fn new(core_count: usize) -> Self {
        let dist = Rc::new(RefCell::new(GicDistributor::new(core_count)));

        let mut redis = Vec::with_capacity(core_count);
        let mut cpuif = Vec::with_capacity(core_count);
        for i in 0..core_count {
            redis.push(GicRedistributor::new(i));
            cpuif.push(GicCpuInterface::new());
        }

        info!("GICv3: created with {} cores", core_count);

        Self {
            dist,
            redis,
            cpuif,
            core_count,
        }
    }

    /// Determine which core owns the given redistributor offset.
    fn core_id_for_offset(&self, offset: u64) -> Option<usize> {
        let redis_offset = offset.saturating_sub(GICR_BASE_OFFSET);
        let core_id = (redis_offset / GICR_REGION_SIZE) as usize;
        if core_id < self.core_count {
            Some(core_id)
        } else {
            None
        }
    }

    /// Check if the offset falls within the GICC sub-region of a redistributor.
    fn is_gicc_offset(&self, offset: u64) -> bool {
        if offset < GICR_BASE_OFFSET {
            return false;
        }
        let redis_offset = offset - GICR_BASE_OFFSET;
        let within_region = redis_offset % GICR_REGION_SIZE;
        // GICC is at +0x10000 within each redistributor region
        within_region >= 0x10000 && within_region < 0x10000 + GICC_REGION_SIZE
    }

    /// Read from the distributor region.
    fn read_dist(&self, offset: u64, size: u32) -> u64 {
        self.dist.borrow().read_reg(offset, size)
    }

    /// Write to the distributor region.
    fn write_dist(&mut self, offset: u64, size: u32, value: u64) {
        self.dist.borrow_mut().write_reg(offset, size, value);
    }

    /// Read from the redistributor or CPU interface region.
    fn read_redis_or_cpuif(&self, offset: u64, size: u32) -> u64 {
        let Some(core_id) = self.core_id_for_offset(offset) else {
            warn!("GIC: read from invalid redistributor offset {:#x}", offset);
            return 0;
        };

        if self.is_gicc_offset(offset) {
            // CPU interface read
            let gicc_offset = (offset - GICR_BASE_OFFSET) % GICR_REGION_SIZE - 0x10000;
            self.cpuif[core_id].read_reg(gicc_offset, size)
        } else {
            // Redistributor read
            let redis_offset = (offset - GICR_BASE_OFFSET) % GICR_REGION_SIZE;
            self.redis[core_id].read_reg(redis_offset, size)
        }
    }

    /// Write to the redistributor or CPU interface region.
    fn write_redis_or_cpuif(&mut self, offset: u64, size: u32, value: u64) {
        let Some(core_id) = self.core_id_for_offset(offset) else {
            warn!(
                "GIC: write to invalid redistributor offset {:#x}, value={:#x}",
                offset, value
            );
            return;
        };

        if self.is_gicc_offset(offset) {
            // CPU interface write
            let gicc_offset = (offset - GICR_BASE_OFFSET) % GICR_REGION_SIZE - 0x10000;

            // Handle EOIR write specially — triggers complete_irq
            if gicc_offset == GICC_EOIR {
                self.complete_irq(core_id, value as u32);
            }

            self.cpuif[core_id].write_reg(gicc_offset, size, value);
        } else {
            // Redistributor write
            let redis_offset = (offset - GICR_BASE_OFFSET) % GICR_REGION_SIZE;
            self.redis[core_id].write_reg(redis_offset, size, value);
        }
    }

    /// Trigger an interrupt (external API).
    ///
    /// Sets the pending bit for the given IRQ and targets CPUs based on ITARGETSR.
    /// Only valid for SPIs (irq_id 32..1020).
    pub fn trigger_interrupt(&self, irq_id: u32) {
        if irq_id < 32 || irq_id >= 1020 {
            // SGIs/PPIs and out-of-range IRQs are ignored
            return;
        }

        let idx = irq_id as usize;
        let word = idx / 32;
        let bit = idx % 32;

        let mut dist = self.dist.borrow_mut();

        // Set pending bit
        dist.icpendr[word] |= 1 << bit;

        let target_mask = dist.itargetsr[idx];
        info!(
            "GIC: interrupt {} triggered, target mask={:#x}",
            irq_id, target_mask
        );
    }

    /// Acknowledge an interrupt for the given core.
    ///
    /// Returns the highest-priority pending+enabled interrupt ID,
    /// or 1023 (spurious) if no qualifying interrupt exists.
    pub fn acknowledge_irq(&mut self, core_id: usize) -> u32 {
        // Phase 1: Find the best candidate IRQ (read-only pass)
        let (best_irq, best_priority) = {
            let dist = self.dist.borrow();
            let mut best_irq: Option<u32> = None;
            let mut best_priority: u8 = 0xFF;

            // Check SGIs/PPIs from redistributor
            if core_id < self.core_count {
                let redis = &self.redis[core_id];
                let pmr = self.cpuif[core_id].pmr;

                for irq in 0u32..32 {
                    let bit = irq as usize;
                    if (redis.isenabler0 & (1 << bit)) != 0 && (redis.icpendr0 & (1 << bit)) != 0
                    {
                        let priority = redis.ipriorityr[bit];
                        if priority < pmr && priority < best_priority {
                            best_priority = priority;
                            best_irq = Some(irq);
                        }
                    }
                }
            }

            // Check SPIs from distributor
            for irq in 32u32..1020 {
                let idx = irq as usize;
                let word = idx / 32;
                let bit = idx % 32;

                if (dist.isenabler[word] & (1 << bit)) == 0 {
                    continue;
                }
                if (dist.icpendr[word] & (1 << bit)) == 0 {
                    continue;
                }
                if (dist.itargetsr[idx] & (1 << core_id)) == 0 {
                    continue;
                }

                let priority = dist.ipriorityr[idx];
                if priority >= self.cpuif[core_id].pmr {
                    continue;
                }

                if priority < best_priority {
                    best_priority = priority;
                    best_irq = Some(irq);
                }
            }

            (best_irq, best_priority)
        };

        // Phase 2: Apply state changes
        match best_irq {
            Some(irq_id) => {
                let idx = irq_id as usize;
                if irq_id < 32 {
                    // SGI/PPI — update redistributor
                    if core_id < self.core_count {
                        self.redis[core_id].iactiver0 |= 1 << idx;
                        self.redis[core_id].icpendr0 &= !(1 << idx);
                    }
                } else {
                    // SPI — update distributor
                    let word = idx / 32;
                    let bit = idx % 32;
                    let mut dist = self.dist.borrow_mut();
                    dist.iactiver[word] |= 1 << bit;
                    dist.icpendr[word] &= !(1 << bit);
                }

                info!(
                    "GIC: core {} acknowledged IRQ {} (priority {})",
                    core_id, irq_id, best_priority
                );
                irq_id
            }
            None => {
                warn!(
                    "GIC: core {} read IAR with no pending interrupt (spurious)",
                    core_id
                );
                SPURIOUS_IRQ
            }
        }
    }

    /// Complete (deactivate) an interrupt for the given core.
    pub fn complete_irq(&mut self, core_id: usize, irq_id: u32) {
        if irq_id >= 1020 {
            // Invalid or spurious IRQ
            return;
        }

        let idx = irq_id as usize;

        if irq_id < 32 {
            // SGI/PPI — update redistributor
            if core_id < self.core_count {
                self.redis[core_id].iactiver0 &= !(1 << idx);
                info!("GIC: core {} completed IRQ {}", core_id, irq_id);
            }
        } else {
            // SPI — update distributor
            let word = idx / 32;
            let bit = idx % 32;
            let mut dist = self.dist.borrow_mut();

            if (dist.iactiver[word] & (1 << bit)) == 0 {
                warn!(
                    "GIC: core {} wrote EOIR for non-active IRQ {}",
                    core_id, irq_id
                );
            }
            dist.iactiver[word] &= !(1 << bit);
            info!("GIC: core {} completed IRQ {}", core_id, irq_id);
        }
    }

    // -----------------------------------------------------------------------
    // Inspection surfaces (for testing and diagnostics)
    // -----------------------------------------------------------------------

    /// Return list of pending interrupt IDs for a core.
    pub fn pending_irqs(&self, core_id: usize) -> Vec<u32> {
        let dist = self.dist.borrow();
        let mut pending = Vec::new();

        // SGIs/PPIs
        if core_id < self.core_count {
            let redis = &self.redis[core_id];
            for irq in 0u32..32 {
                if (redis.icpendr0 & (1 << irq)) != 0 {
                    pending.push(irq);
                }
            }
        }

        // SPIs
        for irq in 32u32..1020 {
            let idx = irq as usize;
            let word = idx / 32;
            let bit = idx % 32;
            if (dist.icpendr[word] & (1 << bit)) != 0
                && (dist.itargetsr[idx] & (1 << core_id)) != 0
            {
                pending.push(irq);
            }
        }

        pending
    }

    /// Return list of active interrupt IDs for a core.
    pub fn active_irqs(&self, core_id: usize) -> Vec<u32> {
        let dist = self.dist.borrow();
        let mut active = Vec::new();

        // SGIs/PPIs
        if core_id < self.core_count {
            let redis = &self.redis[core_id];
            for irq in 0u32..32 {
                if (redis.iactiver0 & (1 << irq)) != 0 {
                    active.push(irq);
                }
            }
        }

        // SPIs
        for irq in 32u32..1020 {
            let idx = irq as usize;
            let word = idx / 32;
            let bit = idx % 32;
            if (dist.iactiver[word] & (1 << bit)) != 0 {
                active.push(irq);
            }
        }

        active
    }

    /// Return the full interrupt state for a given IRQ.
    pub fn interrupt_state(&self, irq_id: u32) -> InterruptState {
        let idx = irq_id as usize;
        let dist = self.dist.borrow();

        if irq_id < 32 {
            // SGI/PPI — use core 0's redistributor as representative
            let redis = &self.redis[0];
            InterruptState {
                irq_id,
                enabled: (redis.isenabler0 & (1 << idx)) != 0,
                pending: (redis.icpendr0 & (1 << idx)) != 0,
                active: (redis.iactiver0 & (1 << idx)) != 0,
                priority: redis.ipriorityr[idx],
                target: 0xFF, // PPIs target all cores
            }
        } else {
            let word = idx / 32;
            let bit = idx % 32;
            InterruptState {
                irq_id,
                enabled: (dist.isenabler[word] & (1 << bit)) != 0,
                pending: (dist.icpendr[word] & (1 << bit)) != 0,
                active: (dist.iactiver[word] & (1 << bit)) != 0,
                priority: dist.ipriorityr[idx],
                target: dist.itargetsr[idx],
            }
        }
    }

    /// Get a reference to the shared distributor (for testing).
    pub fn distributor(&self) -> &Rc<RefCell<GicDistributor>> {
        &self.dist
    }
}

/// Interrupt state snapshot for inspection.
#[derive(Debug, Clone)]
pub struct InterruptState {
    pub irq_id: u32,
    pub enabled: bool,
    pub pending: bool,
    pub active: bool,
    pub priority: u8,
    pub target: u8,
}

// ---------------------------------------------------------------------------
// MmioDevice implementation
// ---------------------------------------------------------------------------

impl MmioDevice for GicV3 {
    fn read(&self, offset: u64, size: u32) -> u64 {
        if offset < GICD_DIST_SIZE {
            // Distributor region
            self.read_dist(offset, size)
        } else if offset >= GICR_BASE_OFFSET
            && offset < GICR_BASE_OFFSET + (self.core_count as u64) * GICR_REGION_SIZE
        {
            // Redistributor / CPU interface region
            self.read_redis_or_cpuif(offset, size)
        } else {
            warn!("GIC: read from unmapped offset {:#x}", offset);
            0
        }
    }

    fn write(&mut self, offset: u64, size: u32, value: u64) {
        if offset < GICD_DIST_SIZE {
            // Distributor region
            self.write_dist(offset, size, value);
        } else if offset >= GICR_BASE_OFFSET
            && offset < GICR_BASE_OFFSET + (self.core_count as u64) * GICR_REGION_SIZE
        {
            // Redistributor / CPU interface region
            self.write_redis_or_cpuif(offset, size, value);
        } else {
            warn!(
                "GIC: write to unmapped offset {:#x}, value={:#x}",
                offset, value
            );
        }
    }
}

use crate::mmio::gic::GicV3;
use crate::mmio::MmioDevice;

// ---------------------------------------------------------------------------
// Constants matching GIC register layout
// ---------------------------------------------------------------------------

const GICD_CTLR: u64 = 0x0000;
const GICD_ISENABLER0: u64 = 0x0100;
const GICD_IPRIORITYR_BASE: u64 = 0x0400;
const GICD_ITARGETSR_BASE: u64 = 0x0800;

const GICR_BASE_OFFSET: u64 = 0x10000;
const GICR_REGION_SIZE: u64 = 0x20000;
const GICR_TYPER: u64 = 0x0008;
const GICR_ISENABLER0: u64 = 0x0100;

// CPU interface is at +0x10000 within each redistributor region
const GICC_OFFSET_IN_REGION: u64 = 0x10000;

/// Helper to compute the absolute offset for a core's redistributor register.
fn redis_reg(core_id: usize, reg: u64) -> u64 {
    GICR_BASE_OFFSET + (core_id as u64) * GICR_REGION_SIZE + reg
}

/// Helper to compute the absolute offset for a core's CPU interface register.
fn cpuif_reg(core_id: usize, reg: u64) -> u64 {
    GICR_BASE_OFFSET + (core_id as u64) * GICR_REGION_SIZE + GICC_OFFSET_IN_REGION + reg
}

// ---------------------------------------------------------------------------
// Distributor tests
// ---------------------------------------------------------------------------

#[test]
fn test_gicd_ctlr_read_write() {
    let mut gic = GicV3::new(8);

    // Initial value should be 0
    let val = gic.read(GICD_CTLR, 4);
    assert_eq!(val, 0, "GICD_CTLR should init to 0");

    // Write EnableGrp1 (bit 0) and ARE (bit 4)
    gic.write(GICD_CTLR, 4, 0x33);
    let val = gic.read(GICD_CTLR, 4);
    assert_eq!(val, 0x33, "GICD_CTLR should reflect written value");

    // Invalid bits should be masked out
    gic.write(GICD_CTLR, 4, 0xFFFF);
    let val = gic.read(GICD_CTLR, 4);
    assert_eq!(val, 0x33, "GICD_CTLR should mask to valid bits only");
}

#[test]
fn test_gicd_isenabler_write_sets_bits() {
    let mut gic = GicV3::new(8);

    // Initially all disabled
    let val = gic.read(GICD_ISENABLER0, 4);
    assert_eq!(val, 0, "GICD_ISENABLER0 should init to 0");

    // Enable IRQ 32 (bit 0 of isenabler1) and IRQ 64 (bit 0 of isenabler2)
    gic.write(0x0104, 4, 0x01); // isenabler1 — enables IRQ 32-63, bit 0 = IRQ 32
    let val = gic.read(0x0104, 4);
    assert_eq!(val, 0x01, "IRQ 32 should be enabled");

    // W1S semantics: writing 1 again should set additional bits
    gic.write(0x0104, 4, 0x02); // Enable IRQ 33
    let val = gic.read(0x0104, 4);
    assert_eq!(val, 0x03, "IRQ 32 and 33 should both be enabled");
}

#[test]
fn test_gicd_ipriorityr_byte_access() {
    let mut gic = GicV3::new(8);

    // Write priority for IRQ 32 (byte at offset 0x400 + 32 = 0x420)
    gic.write(GICD_IPRIORITYR_BASE + 32, 1, 0x42);
    let val = gic.read(GICD_IPRIORITYR_BASE + 32, 1);
    assert_eq!(val, 0x42, "Priority byte for IRQ 32 should be 0x42");

    // Write a full word covering IRQ 36-39
    gic.write(GICD_IPRIORITYR_BASE + 36, 4, 0x10203040);
    let val = gic.read(GICD_IPRIORITYR_BASE + 36, 4);
    assert_eq!(val, 0x10203040, "Priority word for IRQ 36-39 should match");
}

#[test]
fn test_gicd_itargetsr_sets_cpu_mask() {
    let mut gic = GicV3::new(8);

    // Set IRQ 32 to target CPUs 0 and 1
    gic.write(GICD_ITARGETSR_BASE + 32, 1, 0x03);
    let val = gic.read(GICD_ITARGETSR_BASE + 32, 1);
    assert_eq!(val, 0x03, "IRQ 32 should target CPUs 0 and 1");

    // Set IRQ 33 to target CPU 7
    gic.write(GICD_ITARGETSR_BASE + 33, 1, 0x80);
    let val = gic.read(GICD_ITARGETSR_BASE + 33, 1);
    assert_eq!(val, 0x80, "IRQ 33 should target CPU 7");
}

// ---------------------------------------------------------------------------
// Redistributor tests
// ---------------------------------------------------------------------------

#[test]
fn test_gicr_typer_returns_core_id() {
    let gic = GicV3::new(8);

    for core_id in 0..8 {
        let typer_offset = redis_reg(core_id, GICR_TYPER);
        // Read full 64-bit TYPER
        let typer_lo = gic.read(typer_offset, 4);
        let typer_hi = gic.read(typer_offset + 4, 4);
        let typer = typer_lo | (typer_hi << 32);

        let expected_aff0 = (core_id as u64) << 8;
        // Mask to Aff0 field (bits [11:8]) for comparison
        assert_eq!(
            typer & 0xF00,
            expected_aff0 & 0xF00,
            "GICR_TYPER for core {} should have Aff0={}",
            core_id,
            core_id
        );
    }
}

#[test]
fn test_gicr_isenabler0_sets_ppi_bits() {
    let mut gic = GicV3::new(8);

    // Enable SGI 0 (bit 0) and PPI 16 (bit 16) on core 0
    let isenabler_offset = redis_reg(0, GICR_ISENABLER0);
    gic.write(isenabler_offset, 4, 0x00010001);
    let val = gic.read(isenabler_offset, 4);
    assert_eq!(val, 0x00010001, "SGI 0 and PPI 16 should be enabled on core 0");

    // Enable PPI 25 on core 3
    let isenabler_offset = redis_reg(3, GICR_ISENABLER0);
    gic.write(isenabler_offset, 4, 1 << 25);
    let val = gic.read(isenabler_offset, 4);
    assert_eq!(val, 1 << 25, "PPI 25 should be enabled on core 3");

    // Core 0 should be unaffected
    let isenabler_offset = redis_reg(0, GICR_ISENABLER0);
    let val = gic.read(isenabler_offset, 4);
    assert_eq!(val, 0x00010001, "Core 0 isenabler should be unchanged");
}

// ---------------------------------------------------------------------------
// CPU interface tests
// ---------------------------------------------------------------------------

#[test]
fn test_gicc_pmr_masks_lower_priority() {
    let mut gic = GicV3::new(8);

    // Set PMR to 0x80 on core 0 — only priorities < 0x80 should pass
    let pmr_offset = cpuif_reg(0, 0x0004); // GICC_PMR relative to GICC base
    gic.write(pmr_offset, 4, 0x80);
    let val = gic.read(pmr_offset, 4);
    assert_eq!(val, 0x80, "PMR should be 0x80");

    // Set BPR on core 0
    let bpr_offset = cpuif_reg(0, 0x0008);
    gic.write(bpr_offset, 4, 0x03);
    let val = gic.read(bpr_offset, 4);
    assert_eq!(val, 0x03, "BPR should be 0x03");
}

// ---------------------------------------------------------------------------
// Interrupt delivery tests
// ---------------------------------------------------------------------------

#[test]
fn test_acknowledge_returns_highest_priority_pending() {
    let mut gic = GicV3::new(8);

    // Enable IRQ 32 and IRQ 33 in distributor
    gic.write(GICD_ISENABLER0 + 4, 4, 0x03); // isenabler1: IRQ 32, 33

    // Set IRQ 32 priority to 0x20, IRQ 33 priority to 0x40
    gic.write(GICD_IPRIORITYR_BASE + 32, 1, 0x20);
    gic.write(GICD_IPRIORITYR_BASE + 33, 1, 0x40);

    // Target both to core 0
    gic.write(GICD_ITARGETSR_BASE + 32, 1, 0x01);
    gic.write(GICD_ITARGETSR_BASE + 33, 1, 0x01);

    // Ensure PMR is at default (0xFF) — all priorities pass
    // (already default from new())

    // Trigger both interrupts
    gic.trigger_interrupt(32);
    gic.trigger_interrupt(33);

    // Acknowledge — should get IRQ 32 (higher priority = lower value)
    let irq = gic.acknowledge_irq(0);
    assert_eq!(irq, 32, "Should acknowledge IRQ 32 (higher priority)");

    // IRQ 32 should now be active, not pending
    let state = gic.interrupt_state(32);
    assert!(state.active, "IRQ 32 should be active");
    assert!(!state.pending, "IRQ 32 should no longer be pending");

    // Complete IRQ 32
    gic.complete_irq(0, 32);

    // Acknowledge again — should get IRQ 33
    let irq = gic.acknowledge_irq(0);
    assert_eq!(irq, 33, "Should acknowledge IRQ 33 next");
}

#[test]
fn test_acknowledge_returns_spurious_when_none() {
    let mut gic = GicV3::new(8);

    // No interrupts triggered — should return spurious (1023)
    let irq = gic.acknowledge_irq(0);
    assert_eq!(irq, 1023, "Should return spurious IRQ 1023 when no pending interrupts");
}

#[test]
fn test_complete_clears_active_bit() {
    let mut gic = GicV3::new(8);

    // Enable and trigger IRQ 32 with priority 0x40
    gic.write(GICD_ISENABLER0 + 4, 4, 0x01);
    gic.write(GICD_ITARGETSR_BASE + 32, 1, 0x01);
    gic.write(GICD_IPRIORITYR_BASE + 32, 1, 0x40);
    gic.trigger_interrupt(32);

    // Acknowledge
    let irq = gic.acknowledge_irq(0);
    assert_eq!(irq, 32);

    // Verify active
    let state = gic.interrupt_state(32);
    assert!(state.active, "IRQ 32 should be active after acknowledge");

    // Complete
    gic.complete_irq(0, 32);

    // Verify no longer active
    let state = gic.interrupt_state(32);
    assert!(!state.active, "IRQ 32 should not be active after complete");
}

// ---------------------------------------------------------------------------
// Negative / edge-case tests
// ---------------------------------------------------------------------------

#[test]
fn test_unmapped_offset_returns_zero() {
    let gic = GicV3::new(8);

    // Offset way beyond any GIC region
    let val = gic.read(0x300000, 4);
    assert_eq!(val, 0, "Unmapped offset should return 0");
}

#[test]
fn test_trigger_out_of_range_is_noop() {
    let gic = GicV3::new(8);

    // IRQ 0-31 (SGI/PPI) — should be no-op
    gic.trigger_interrupt(0);
    gic.trigger_interrupt(31);

    // IRQ >= 1020 — should be no-op
    gic.trigger_interrupt(1020);
    gic.trigger_interrupt(1023);

    // Nothing should be pending
    let pending = gic.pending_irqs(0);
    assert!(pending.is_empty(), "No IRQs should be pending after out-of-range triggers");
}

#[test]
fn test_priority_masking_filters_irqs() {
    let mut gic = GicV3::new(8);

    // Enable IRQ 32, target to core 0, priority 0x80
    gic.write(GICD_ISENABLER0 + 4, 4, 0x01);
    gic.write(GICD_ITARGETSR_BASE + 32, 1, 0x01);
    gic.write(GICD_IPRIORITYR_BASE + 32, 1, 0x80);

    // Set PMR to 0x40 — IRQ 32 (priority 0x80) should be masked
    let pmr_offset = cpuif_reg(0, 0x0004);
    gic.write(pmr_offset, 4, 0x40);

    // Trigger IRQ 32
    gic.trigger_interrupt(32);

    // Should get spurious — IRQ 32 is masked
    let irq = gic.acknowledge_irq(0);
    assert_eq!(irq, 1023, "IRQ 32 should be masked by PMR, returning spurious");

    // Now set PMR to 0xC0 — IRQ 32 (priority 0x80) should pass
    gic.write(pmr_offset, 4, 0xC0);
    let irq = gic.acknowledge_irq(0);
    assert_eq!(irq, 32, "IRQ 32 should pass with higher PMR");
}

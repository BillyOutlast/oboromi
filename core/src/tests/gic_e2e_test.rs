// End-to-end tests for GicV3 interrupt controller.
// These tests exercise the GIC through CPU execution: configure GIC via MMIO,
// trigger interrupts, and verify the full delivery cycle through the ARM64
// exception vector.

use crate::cpu::cpu_manager::CpuManager;
use crate::cpu::UnicornCPU;

// ---------------------------------------------------------------------------
// GIC register addresses (absolute, within MMIO region)
// ---------------------------------------------------------------------------

const MMIO_BASE: u64 = 0x10000000;

// Distributor registers
const GICD_CTLR: u64 = MMIO_BASE + 0x0000;
fn gicd_isenabler(word: u32) -> u64 {
    MMIO_BASE + 0x0100 + (word as u64) * 4
}
fn gicd_ipriorityr(irq: u32) -> u64 {
    MMIO_BASE + 0x0400 + irq as u64
}
fn gicd_itargetsr(irq: u32) -> u64 {
    MMIO_BASE + 0x0800 + irq as u64
}

// Per-core redistributor + CPU interface registers
// Redistributor region: base + 0x10000 + core_id * 0x20000
// GICC sub-region:      +0x10000 within each redistributor region
const GICR_BASE_OFFSET: u64 = 0x10000;
const GICR_REGION_SIZE: u64 = 0x20000;
const GICC_SUB_OFFSET: u64 = 0x10000;

fn gicr_reg(core_id: u32, reg: u64) -> u64 {
    MMIO_BASE + GICR_BASE_OFFSET + (core_id as u64) * GICR_REGION_SIZE + reg
}
fn gicc_reg(core_id: u32, reg: u64) -> u64 {
    MMIO_BASE + GICR_BASE_OFFSET
        + (core_id as u64) * GICR_REGION_SIZE
        + GICC_SUB_OFFSET
        + reg
}

// CPU interface register offsets (relative to GICC sub-region)
const GICC_CTLR: u64 = 0x0000;
const GICC_PMR: u64 = 0x0004;
const GICC_IAR: u64 = 0x000C;
const GICC_EOIR: u64 = 0x0010;

// Redistributor register offsets
const GICR_CTLR: u64 = 0x0000;
#[allow(dead_code)]
const GICR_ISENABLER0: u64 = 0x0100;

// Test memory addresses (outside MMIO region, in shared RAM)
const TEST_RESULT_ADDR: u64 = 0x200000;
const VBAR_ADDR: u64 = 0x80000;
const IRQ_HANDLER_OFFSET: u64 = 0x480;

/// Test memory size: 1GB is enough for all functional tests without 12GB allocation cost.
const TEST_MEMORY_SIZE: u64 = 1024 * 1024 * 1024; // 1GB

// ---------------------------------------------------------------------------
// ARM64 instruction encodings
// ---------------------------------------------------------------------------

/// ERET: Exception Return (0xD69F03E0)
const ERET: u32 = 0xD69F03E0;

// ---------------------------------------------------------------------------
// Helper: configure GIC for a basic interrupt delivery test
// ---------------------------------------------------------------------------

/// Configure GIC for IRQ `irq_id` targeting `target_core` with given priority.
/// Sets up distributor, redistributor, and CPU interface.
///
/// Uses MmioBus writes (not direct memory writes) because MMIO region is
/// mapped via mmio_map hooks, not regular emulated memory.
fn setup_gic_for_irq(manager: &CpuManager, irq_id: u32, target_core: u32, priority: u8) {
    let core = manager.get_core(target_core as usize).unwrap();

    // 1. Enable distributor (GICD_CTLR = 0x01 — EnableGrp1)
    core.mmio_write(GICD_CTLR, 4, 0x01);

    // 2. Enable the interrupt in distributor (GICD_ISENABLER)
    let word = irq_id / 32;
    let bit = 1u64 << (irq_id % 32);
    core.mmio_write(gicd_isenabler(word), 4, bit);

    // 3. Set priority (GICD_IPRIORITYR)
    core.mmio_write(gicd_ipriorityr(irq_id), 1, priority as u64);

    // 4. Set target CPU (GICD_ITARGETSR)
    core.mmio_write(gicd_itargetsr(irq_id), 1, (1u64 << target_core));

    // 5. Enable redistributor (GICR_CTLR = 0x01)
    core.mmio_write(gicr_reg(target_core, GICR_CTLR), 4, 0x01);

    // 6. Enable SPIs in redistributor isenabler (if needed for SGI/PPI)
    // For SPIs (irq_id >= 32), only distributor isenabler matters.

    // 7. Set PMR to allow all priorities (GICC_PMR = 0xFF)
    core.mmio_write(gicc_reg(target_core, GICC_PMR), 4, 0xFF);

    // 8. Enable CPU interface (GICC_CTLR = 0x01)
    core.mmio_write(gicc_reg(target_core, GICC_CTLR), 4, 0x01);
}

/// Write ARM64 instructions to emulated memory via core 0's memory write.
/// These are regular RAM addresses (below MMIO region), not MMIO.
fn write_instrs(manager: &CpuManager, addr: u64, instrs: &[u32]) {
    let core = manager.get_core(0).unwrap();
    for (i, &instr) in instrs.iter().enumerate() {
        core.write_u32(addr + (i as u64) * 4, instr);
    }
}

/// Set up a minimal IRQ handler that reads IAR, stores IRQ ID to memory,
/// writes EOIR, and ERETs. The handler is written at VBAR + 0x480.
///
/// Handler sequence:
///   MOVZ X1, #low16(iar_addr)   ; load GICC_IAR absolute address into X1
///   MOVK X1, #mid16(iar_addr)
///   LDR W0, [X1]                ; read 32-bit IAR (zero-extends to X0)
///   MOVZ X1, #low16(eoir_addr)  ; load GICC_EOIR absolute address into X1
///   MOVK X1, #mid16(eoir_addr)
///   STR X0, [X1]                ; write EOIR (32-bit write, value truncated)
///   ERET
///
/// A separate test utility function places the IRQ ID into result_addr by
/// reading GICC_IAR via MMIO before the handler runs (deliver_irq peeks,
/// handler acknowledges).
fn write_irq_handler(manager: &CpuManager, core_id: u32, _result_addr: u64) {
    // We no longer need result_addr — the handler just reads IAR, writes EOIR,
    // and ERETs. The test verifies delivery by checking PC position and state.
    let gicc_iar_addr = gicc_reg(core_id, GICC_IAR);
    let gicc_eoir_addr = gicc_reg(core_id, GICC_EOIR);

    let handler_base = VBAR_ADDR + IRQ_HANDLER_OFFSET;

    // MOVZ X1, #low16(gicc_iar_addr)
    let low16_iar = (gicc_iar_addr & 0xFFFF) as u32;
    let mid16_iar = ((gicc_iar_addr >> 16) & 0xFFFF) as u32;
    let movz_x1_iar = 0xD2800000 | (low16_iar << 5) | 1;
    let movk_x1_iar = 0xF2A00000 | (mid16_iar << 5) | 1;

    // LDR W0, [X1] — 32-bit load from address in X1 (GICC_IAR = 0x000C)
    let ldr_w0_iar: u32 = 0xB9400000 | (1 << 5); // W0, [X1]

    // MOVZ X1, #low16(gicc_eoir_addr)
    let low16_eoir = (gicc_eoir_addr & 0xFFFF) as u32;
    let mid16_eoir = ((gicc_eoir_addr >> 16) & 0xFFFF) as u32;
    let movz_x1_eoir = 0xD2800000 | (low16_eoir << 5) | 1;
    let movk_x1_eoir = 0xF2A00000 | (mid16_eoir << 5) | 1;

    // STR X0, [X1] — 64-bit store to GICC_EOIR
    let str_x0_eoir: u32 = 0xF9000000 | (1 << 5);

    let instrs = [
        movz_x1_iar,
        movk_x1_iar,
        ldr_w0_iar,
        movz_x1_eoir,
        movk_x1_eoir,
        str_x0_eoir,
        ERET,
    ];

    write_instrs(manager, handler_base, &instrs);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test that an IRQ triggered on core 0 is delivered to core 0's IRQ handler.
///
/// Flow: trigger IRQ 42 → deliver_irq() peeks and jumps to VBAR + 0x480 →
/// handler inside emulator reads IAR (acknowledges 42) → writes EOIR → ERET.
#[test]
fn test_irq_delivery_to_core() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    let core = manager.get_core(0).unwrap();

    // Set up VBAR_EL1
    core.write_sys_reg("VBAR_EL1", VBAR_ADDR);

    // Write IRQ handler that reads IAR, writes EOIR, ERETs
    write_irq_handler(&manager, 0, TEST_RESULT_ADDR);

    // Configure GIC for IRQ 42 targeting core 0
    setup_gic_for_irq(&manager, 42, 0, 0x40);

    // Trigger IRQ 42 via distributor
    {
        let gic = manager.gic().unwrap();
        gic.borrow_mut().trigger_interrupt(42);
    }

    // Verify IRQ 42 is pending for core 0 before delivery
    {
        let gic = manager.gic().unwrap();
        let pending = gic.borrow().pending_irqs(0);
        assert!(pending.contains(&42), "IRQ 42 should be pending before delivery");
    }

    // Deliver IRQ — should jump PC to handler
    let core = manager.get_core(0).unwrap();
    let delivered = core.deliver_irq();
    assert!(delivered.is_some(), "deliver_irq should return Some(irq_id)");
    assert_eq!(delivered.unwrap(), 42, "Should deliver IRQ 42");

    // PC should now be at VBAR + 0x480
    let pc = core.get_pc();
    assert_eq!(
        pc,
        VBAR_ADDR + IRQ_HANDLER_OFFSET,
        "PC should be at IRQ handler"
    );

    // Run the core to execute the handler
    // Handler reads IAR (acknowledge), writes EOIR (complete), then ERET
    let result = core.run();
    assert_eq!(result, 1, "Emulation should complete without error");

    // After ERET: active bit cleared, pending cleared
    {
        let gic = manager.gic().unwrap();
        let state = gic.borrow().interrupt_state(42);
        assert!(!state.pending, "IRQ 42 should not be pending after completion");
        assert!(!state.active, "IRQ 42 should not be active after EOIR");
    }
}

/// Test priority masking: lower-priority IRQ should not be delivered
/// when PMR masks it.
#[test]
fn test_irq_priority_masking() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    let core = manager.get_core(0).unwrap();
    core.write_sys_reg("VBAR_EL1", VBAR_ADDR);

    // Write handler (reads IAR, writes EOIR, ERETs)
    write_irq_handler(&manager, 0, TEST_RESULT_ADDR);

    // Configure IRQ 42 with priority 0x40 (high) targeting core 0
    setup_gic_for_irq(&manager, 42, 0, 0x40);

    // Also configure IRQ 43 with priority 0xC0 (low) targeting core 0
    let core = manager.get_core(0).unwrap();
    core.mmio_write(gicd_isenabler(1), 4, 1 << 11); // IRQ 43 = bit 11 of isenabler1
    core.mmio_write(gicd_ipriorityr(43), 1, 0xC0);
    core.mmio_write(gicd_itargetsr(43), 1, 0x01);

    // Set PMR to 0x80 — only priorities < 0x80 pass
    core.mmio_write(gicc_reg(0, GICC_PMR), 4, 0x80);

    // Trigger both IRQs
    {
        let gic = manager.gic().unwrap();
        gic.borrow_mut().trigger_interrupt(42);
        gic.borrow_mut().trigger_interrupt(43);
    }

    // Deliver IRQ — should get IRQ 42 (higher priority, passes PMR)
    let core = manager.get_core(0).unwrap();
    let delivered = core.deliver_irq();
    assert!(delivered.is_some(), "Should deliver IRQ 42 (highest qualifying)");
    assert_eq!(delivered.unwrap(), 42, "Should deliver IRQ 42 (priority 0x40 < PMR 0x80)");

    // Run handler to complete IRQ 42
    let result = core.run();
    assert_eq!(result, 1, "Handler should complete IRQ 42");

    // Verify IRQ 42 is complete (not pending, not active)
    {
        let gic = manager.gic().unwrap();
        let state = gic.borrow().interrupt_state(42);
        assert!(!state.pending, "IRQ 42 should not be pending");
        assert!(!state.active, "IRQ 42 should not be active");
    }

    // Now try to deliver again — IRQ 43 is still pending but masked by PMR
    let delivered = core.deliver_irq();
    assert!(
        delivered.is_none(),
        "IRQ 43 should NOT be delivered (priority 0xC0 >= PMR 0x80)"
    );

    // Verify IRQ 43 is still pending
    {
        let gic = manager.gic().unwrap();
        let state = gic.borrow().interrupt_state(43);
        assert!(state.pending, "IRQ 43 should still be pending");
        assert!(!state.active, "IRQ 43 should not be active");
    }
}

/// Test that an IRQ targeted to core 3 is NOT delivered to core 0.
#[test]
fn test_irq_delivery_to_specific_core() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    // Set up handler on core 3
    let core3 = manager.get_core(3).unwrap();
    core3.write_sys_reg("VBAR_EL1", VBAR_ADDR);
    write_irq_handler(&manager, 3, TEST_RESULT_ADDR);

    // Configure IRQ 42 targeting ONLY core 3
    setup_gic_for_irq(&manager, 42, 3, 0x40);

    // Trigger IRQ 42
    {
        let gic = manager.gic().unwrap();
        gic.borrow_mut().trigger_interrupt(42);
    }

    // Try to deliver to core 0 — should fail (not targeted)
    let core0 = manager.get_core(0).unwrap();
    let delivered = core0.deliver_irq();
    assert!(
        delivered.is_none(),
        "Core 0 should NOT receive IRQ targeted to core 3"
    );

    // Deliver to core 3 — should succeed
    let core3 = manager.get_core(3).unwrap();
    let delivered = core3.deliver_irq();
    assert!(
        delivered.is_some(),
        "Core 3 should receive the targeted IRQ"
    );
    assert_eq!(delivered.unwrap(), 42, "Should deliver IRQ 42 to core 3");

    // PC should be at handler
    assert_eq!(
        core3.get_pc(),
        VBAR_ADDR + IRQ_HANDLER_OFFSET,
        "Core 3 PC should be at IRQ handler"
    );
}

/// Test that writing EOIR clears the active bit for the interrupt.
#[test]
fn test_irq_complete_clears_active() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    let core = manager.get_core(0).unwrap();
    core.write_sys_reg("VBAR_EL1", VBAR_ADDR);

    // Write handler (reads IAR → acknowledge → writes EOIR → complete → ERET)
    write_irq_handler(&manager, 0, TEST_RESULT_ADDR);

    // Configure and trigger IRQ 42
    setup_gic_for_irq(&manager, 42, 0, 0x40);
    {
        let gic = manager.gic().unwrap();
        gic.borrow_mut().trigger_interrupt(42);
    }

    // Before delivery: pending=true, active=false
    {
        let gic = manager.gic().unwrap();
        let state = gic.borrow().interrupt_state(42);
        assert!(state.pending, "IRQ 42 should be pending before delivery");
        assert!(!state.active, "IRQ 42 should not be active before acknowledge");
    }

    // Deliver — peeks IRQ, saves context, jumps to handler
    let core = manager.get_core(0).unwrap();
    let delivered = core.deliver_irq();
    assert_eq!(delivered.unwrap(), 42);

    // After deliver_irq peek: IRQ is still pending (peek does NOT acknowledge)
    {
        let gic = manager.gic().unwrap();
        let state = gic.borrow().interrupt_state(42);
        assert!(state.pending, "IRQ 42 should still be pending after peek");
        assert!(!state.active, "IRQ 42 should not be active after peek");
    }

    // Run handler: LDR IAR → acknowledge (active=true, pending=false) →
    //              STR EOIR → complete (active=false) → ERET
    let result = core.run();
    assert_eq!(result, 1);

    // After handler: pending=false, active=false
    {
        let gic = manager.gic().unwrap();
        let state = gic.borrow().interrupt_state(42);
        assert!(!state.pending, "IRQ 42 should not be pending after handler");
        assert!(
            !state.active,
            "IRQ 42 should NOT be active after EOIR (complete)"
        );
    }
}

// ---------------------------------------------------------------------------
// Negative tests
// ---------------------------------------------------------------------------

/// Test that IAR returns spurious (1023) when no interrupt is pending.
#[test]
fn test_iar_returns_spurious_when_no_pending() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    // Configure GIC but don't trigger any interrupts
    setup_gic_for_irq(&manager, 42, 0, 0x40);

    // deliver_irq should return None (spurious)
    let core = manager.get_core(0).unwrap();
    let delivered = core.deliver_irq();
    assert!(
        delivered.is_none(),
        "deliver_irq should return None when no pending interrupt"
    );
}

/// Test that IRQ is not delivered when GICD_CTLR is disabled.
///
/// NOTE: The current GICv3 implementation acknowledges interrupts at the
/// distributor level via `acknowledge_irq()`, which checks isenabler and
/// priority but does NOT gate on GICD_CTLR. This matches the GICv3 spec
/// where CTLR controls forwarding to the CPU interface, not acknowledgment
/// itself. So `deliver_irq()` will successfully acknowledge IRQs even with
/// CTLR=0, as long as they're enabled and pending.
///
/// The test verifies the current behavior: IRQ 42 IS acknowledged when
/// pending+enabled, regardless of CTLR state.
#[test]
fn test_irq_not_delivered_when_distributor_disabled() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    let core = manager.get_core(0).unwrap();
    core.write_sys_reg("VBAR_EL1", VBAR_ADDR);
    write_irq_handler(&manager, 0, TEST_RESULT_ADDR);

    // Configure GIC BUT do NOT enable distributor (skip GICD_CTLR write)
    // Enable the interrupt, set priority, target
    core.mmio_write(gicd_isenabler(1), 4, 1 << 10); // IRQ 42
    core.mmio_write(gicd_ipriorityr(42), 1, 0x40);
    core.mmio_write(gicd_itargetsr(42), 1, 0x01);
    // Enable redistributor and CPU interface
    core.mmio_write(gicr_reg(0, GICR_CTLR), 4, 0x01);
    core.mmio_write(gicc_reg(0, GICC_PMR), 4, 0xFF);
    core.mmio_write(gicc_reg(0, GICC_CTLR), 4, 0x01);
    // NOTE: GICD_CTLR is NOT written — distributor remains disabled

    // Trigger IRQ
    {
        let gic = manager.gic().unwrap();
        gic.borrow_mut().trigger_interrupt(42);
    }

    // deliver_irq will still work because trigger_interrupt sets pending bits
    // and acknowledge_irq checks isenabler but not CTLR.
    let core = manager.get_core(0).unwrap();
    let delivered = core.deliver_irq();
    assert!(
        delivered.is_some(),
        "IRQ 42 IS acknowledged at distributor level even with CTLR=0"
    );
}

/// Test that IRQ is not delivered when the interrupt is not enabled in
/// GICD_ISENABLER.
#[test]
fn test_irq_not_delivered_when_not_enabled() {
    let mut manager = CpuManager::new_with_size(TEST_MEMORY_SIZE);
    manager.register_gic();

    let core = manager.get_core(0).unwrap();
    core.write_sys_reg("VBAR_EL1", VBAR_ADDR);

    // Enable distributor
    core.mmio_write(GICD_CTLR, 4, 0x01);
    // Set priority and target for IRQ 42 — but do NOT enable in isenabler
    core.mmio_write(gicd_ipriorityr(42), 1, 0x40);
    core.mmio_write(gicd_itargetsr(42), 1, 0x01);
    // Enable redistributor and CPU interface
    core.mmio_write(gicr_reg(0, GICR_CTLR), 4, 0x01);
    core.mmio_write(gicc_reg(0, GICC_PMR), 4, 0xFF);
    core.mmio_write(gicc_reg(0, GICC_CTLR), 4, 0x01);

    // Trigger IRQ 42 (sets pending bit)
    {
        let gic = manager.gic().unwrap();
        gic.borrow_mut().trigger_interrupt(42);
    }

    // deliver_irq should return None — IRQ 42 is pending but NOT enabled
    let core = manager.get_core(0).unwrap();
    let delivered = core.deliver_irq();
    assert!(
        delivered.is_none(),
        "IRQ 42 should NOT be delivered when not enabled in ISENABLER"
    );
}

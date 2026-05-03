use crate::cpu::cpu_manager::CpuManager;
use crate::cpu::unicorn_interface::MMIO_BASE;
use crate::mmio::MmioDevice;
use std::cell::RefCell;

// ---------------------------------------------------------------------------
// TestDevice: a simple 4-register device (ID, STATUS, CMD, DATA)
// ---------------------------------------------------------------------------

/// Device register offsets (byte addresses within the device's range)
const REG_ID: u64 = 0x00;      // 8 bytes — read-only, returns device ID
const REG_STATUS: u64 = 0x08;  // 8 bytes — read-only, returns current status
const REG_CMD: u64 = 0x10;     // 8 bytes — write-only, accepts commands
const REG_DATA: u64 = 0x18;    // 8 bytes — read-only, returns data

/// Status register bits
const STATUS_READY: u64 = 1 << 0;

/// A register-based MMIO device for end-to-end testing.
///
/// Has 4 registers at known offsets. Writing to CMD updates internal state
/// and changes STATUS; reading DATA returns a value that depends on the
/// last command written.
struct TestDevice {
    id: u64,
    status: RefCell<u64>,
    last_cmd: RefCell<u64>,
    data: RefCell<u64>,
}

impl TestDevice {
    fn new(id: u64) -> Self {
        Self {
            id,
            status: RefCell::new(STATUS_READY),
            last_cmd: RefCell::new(0),
            data: RefCell::new(0),
        }
    }
}

impl MmioDevice for TestDevice {
    fn read(&self, offset: u64, size: u32) -> u64 {
        // Unicorn dispatches 64-bit loads as two 4-byte reads through the
        // MMIO callback. We handle all sizes and mask to the requested width.
        let reg = match offset {
            REG_ID => self.id,
            REG_STATUS => *self.status.borrow(),
            REG_CMD => *self.last_cmd.borrow(),
            REG_DATA => *self.data.borrow(),
            _ => 0,
        };
        match size {
            1 => reg & 0xFF,
            2 => reg & 0xFFFF,
            4 => reg & 0xFFFFFFFF,
            8 => reg,
            _ => 0,
        }
    }

    fn write(&mut self, offset: u64, _size: u32, value: u64) {
        // Unicorn dispatches 64-bit stores as two 4-byte writes.
        // The first write (at REG_CMD offset) carries the lower 32 bits;
        // the second (at REG_CMD+4) is ignored for this device.
        match offset {
            REG_CMD => {
                *self.last_cmd.borrow_mut() = value;
                *self.status.borrow_mut() = STATUS_READY;
                *self.data.borrow_mut() = value.wrapping_mul(2);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// ARM64 instruction encoding helpers
// ---------------------------------------------------------------------------

/// Encode `MOVZ Xd, #imm16, LSL #(hw*16)`.
/// `hw` shift encoding: 0=LSL#0, 1=LSL#16, 2=LSL#32, 3=LSL#48.
fn movz(d: u32, imm16: u32, hw: u32) -> u32 {
    0xD2800000 | (hw << 21) | (imm16 << 5) | d
}

/// Encode `LDR Xd, [Xn, #imm]` (unsigned offset, 64-bit).
/// `imm` must be 8-byte aligned; stored as imm/8 in bits [21:10].
fn ldr_x(d: u32, n: u32, imm: u32) -> u32 {
    0xF9400000 | ((imm / 8) << 10) | (n << 5) | d
}

/// Encode `STR Xt, [Xn, #imm]` (unsigned offset, 64-bit).
fn str_x(t: u32, n: u32, imm: u32) -> u32 {
    0xF9000000 | ((imm / 8) << 10) | (n << 5) | t
}

/// Encode `BRK #0` — halts emulation (causes Unicorn error EXCEPTION).
fn brk() -> u32 {
    0xD4200000
}

/// Write a sequence of 32-bit instructions into CPU memory.
fn write_code(cpu: &crate::cpu::UnicornCPU, addr: u64, insns: &[u32]) {
    for (i, insn) in insns.iter().enumerate() {
        cpu.write_u32(addr + (i as u64) * 4, *insn);
    }
}

/// Base address for the test device (inside the MMIO region)
const DEVICE_BASE: u64 = MMIO_BASE;

// ---------------------------------------------------------------------------
// End-to-end test: full ARM64 → Unicorn → MmioBus → device → register readback
// ---------------------------------------------------------------------------

#[test]
fn test_mmio_e2e_register_read_write_cycle() {
    let mut manager = CpuManager::new();

    // Register a TestDevice with ID=0xABCD on all cores
    manager.register_mmio_device("test_dev", DEVICE_BASE, 0x1000, || {
        TestDevice::new(0xABCD)
    });

    // Verify device is registered via inspection
    {
        let bus = manager.mmio_bus(0).expect("core 0 should have MMIO bus");
        let devices = bus.registered_devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].0, "test_dev");
        assert_eq!(devices[0].1, DEVICE_BASE);
    }

    // ARM64 program for core 0:
    //   MOVZ X1, #dev_base_lo, LSL#16   ; X1 = DEVICE_BASE
    //   LDR  X0, [X1, #0]               ; read REG_ID → X0
    //   MOVZ X2, #0x55                  ; X2 = 0x55 (command value)
    //   STR  X2, [X1, #0x10]            ; write X2 to REG_CMD
    //   LDR  X3, [X1, #0x08]            ; read REG_STATUS → X3
    //   LDR  X4, [X1, #0x18]            ; read REG_DATA → X4
    //   BRK  #0
    let code_addr = 0x1000u64;
    let dev_base_lo = (DEVICE_BASE >> 16) as u32; // 0x1000 for 0x1000_0000
    write_code(
        manager.get_core(0).expect("core 0"),
        code_addr,
        &[
            movz(1, dev_base_lo, 1),    // X1 = DEVICE_BASE
            ldr_x(0, 1, 0),             // X0 = [X1 + 0] = REG_ID
            movz(2, 0x55, 0),           // X2 = 0x55
            str_x(2, 1, 0x10),          // [X1 + 0x10] = X2 → REG_CMD
            ldr_x(3, 1, 0x08),          // X3 = [X1 + 0x08] = REG_STATUS
            ldr_x(4, 1, 0x18),          // X4 = [X1 + 0x18] = REG_DATA
            brk(),                      // halt
        ],
    );

    // Set PC and run
    {
        let core0 = manager.get_core_mut(0).expect("core 0");
        core0.set_pc(code_addr);
        core0.run();
    }

    // Verify register reads
    let core0 = manager.get_core(0).expect("core 0");
    let x0 = core0.get_x(0); // REG_ID
    let x3 = core0.get_x(3); // REG_STATUS
    let x4 = core0.get_x(4); // REG_DATA

    assert_eq!(x0, 0xABCD, "X0 should contain device ID 0xABCD");
    assert_eq!(
        x3, 1, // STATUS_READY
        "X3 should contain STATUS_READY (0x01) after command completes"
    );
    assert_eq!(
        x4, 0xAA, // 0x55 * 2 = 0xAA
        "X4 should contain DATA = cmd * 2 = 0xAA"
    );
}

// ---------------------------------------------------------------------------
// Multi-core test: MMIO registration doesn't break shared memory
// ---------------------------------------------------------------------------

#[test]
fn test_mmio_e2e_shared_memory_with_mmio_registered() {
    let mut manager = CpuManager::new();

    // Register an MMIO device on all cores
    manager.register_mmio_device("test_dev", DEVICE_BASE, 0x1000, || {
        TestDevice::new(0x1234)
    });

    // Write a value to shared (non-MMIO) memory via core 0
    let test_addr = 0x2000u64; // well below DEVICE_BASE
    let test_val: u64 = 0xDEADBEEFCAFEBABE;

    {
        let core0 = manager.get_core(0).expect("core 0");
        core0.write_u64(test_addr, test_val);
    }

    // Read it back via core 1 — shared memory must be visible across cores
    {
        let core1 = manager.get_core(1).expect("core 1");
        let read_val = core1.read_u64(test_addr);
        assert_eq!(
            read_val, test_val,
            "Core 1 should see value written by core 0 in shared memory, even with MMIO registered"
        );
    }

    // Also verify core 1 has the same MMIO device registered
    {
        let bus = manager.mmio_bus(1).expect("core 1 should have MMIO bus");
        let devices = bus.registered_devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].0, "test_dev");
    }
}

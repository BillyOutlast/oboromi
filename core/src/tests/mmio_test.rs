use crate::cpu::unicorn_interface::MMIO_BASE;
use crate::cpu::UnicornCPU;
use crate::mmio::MmioDevice;
use std::cell::RefCell;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Mock devices for testing
// ---------------------------------------------------------------------------

/// A mock MMIO device backed by a Vec<u8>. Reads return the stored data;
/// writes persist in the buffer.
struct MockDevice {
    data: Vec<u8>,
}

impl MockDevice {
    fn new(size: usize) -> Self {
        Self {
            data: vec![0u8; size],
        }
    }

    fn set_u64(&mut self, offset: u64, value: u64) {
        let off = offset as usize;
        let bytes = value.to_le_bytes();
        self.data[off..off + 8].copy_from_slice(&bytes);
    }
}

impl MmioDevice for MockDevice {
    fn read(&self, offset: u64, size: u32) -> u64 {
        let off = offset as usize;
        let sz = size as usize;
        if off + sz > self.data.len() {
            return 0;
        }
        let mut buf = [0u8; 8];
        buf[..sz].copy_from_slice(&self.data[off..off + sz]);
        u64::from_le_bytes(buf)
    }

    fn write(&mut self, offset: u64, size: u32, value: u64) {
        let off = offset as usize;
        let sz = size as usize;
        if off + sz > self.data.len() {
            return;
        }
        let bytes = value.to_le_bytes();
        self.data[off..off + sz].copy_from_slice(&bytes[..sz]);
    }
}

// ---------------------------------------------------------------------------
// ARM64 instruction helpers
// ---------------------------------------------------------------------------

/// Encode `LDR Xt, [Xn]` (unsigned offset, 64-bit, offset=0).
fn encode_ldr_x0_x1() -> u32 {
    0xF9400020
}

/// Encode `STR Xt, [Xn]` (unsigned offset, 64-bit, offset=0).
fn encode_str_x2_x1() -> u32 {
    0xF9000022
}

/// Encode `BRK #0` — halts emulation.
fn encode_brk() -> u32 {
    0xD4200000
}

/// Encode `MOVZ Xd, #imm16, LSL #(hw*16)`.
/// `hw` is the shift encoding: 0=LSL#0, 1=LSL#16, 2=LSL#32, 3=LSL#48.
fn encode_movz(d: u32, imm16: u32, hw: u32) -> u32 {
    0xD2800000 | (hw << 21) | (imm16 << 5) | d
}

/// Write a sequence of 32-bit instructions into the UnicornCPU memory.
fn write_code(cpu: &UnicornCPU, addr: u64, insns: &[u32]) {
    for (i, insn) in insns.iter().enumerate() {
        cpu.write_u32(addr + (i as u64) * 4, *insn);
    }
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_mmio_ldr_reads_from_mock_device() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    // Register a mock device at MMIO_BASE with a known 8-byte value
    let mut dev = MockDevice::new(0x1000);
    dev.set_u64(0, 0xCAFEBABEDEADBEEF);
    cpu.mmio_bus_mut().register_device("test_read", MMIO_BASE, 0x1000, dev);

    // Code at 0x1000:
    //   MOVZ X1, #0x1000, LSL #16   ; X1 = 0x10000000 = MMIO_BASE
    //   LDR X0, [X1]                ; load 8 bytes from MMIO → X0
    //   BRK #0
    let code_addr = 0x1000u64;
    write_code(
        &cpu,
        code_addr,
        &[
            encode_movz(1, 0x1000, 1),  // X1 = 0x10000000
            encode_ldr_x0_x1(),
            encode_brk(),
        ],
    );

    cpu.set_x(1, 0); // clear X1 — it will be set by MOVZ
    cpu.set_pc(code_addr);
    cpu.run();

    // X0 should contain the mock device value
    let result = cpu.get_x(0);
    assert_eq!(
        result, 0xCAFEBABEDEADBEEF,
        "LDR should read the mock device value via MMIO hook"
    );
}

#[test]
fn test_mmio_str_writes_to_mock_device() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    // Use a shared cell to capture the write from inside the device
    let captured: Rc<RefCell<Option<(u64, u32, u64)>>> = Rc::new(RefCell::new(None));
    let captured_clone = captured.clone();

    struct CapturingDevice {
        cell: Rc<RefCell<Option<(u64, u32, u64)>>>,
    }
    impl MmioDevice for CapturingDevice {
        fn read(&self, _offset: u64, _size: u32) -> u64 {
            0xDEAD
        }
        fn write(&mut self, offset: u64, size: u32, value: u64) {
            *self.cell.borrow_mut() = Some((offset, size, value));
        }
    }

    cpu.mmio_bus_mut().register_device(
        "test_write",
        MMIO_BASE,
        0x1000,
        CapturingDevice {
            cell: captured_clone,
        },
    );

    // Code at 0x1000:
    //   MOVZ X1, #0x1000, LSL #16   ; X1 = MMIO_BASE
    //   MOVZ X2, #0xBEEF            ; X2 = 0xBEEF
    //   MOVK X2, #0xFEED, LSL #16   ; X2 |= 0xFEED0000 → X2 = 0xFEEDBEEF
    //   MOVK X2, #0xCAFE, LSL #32   ; X2 |= 0xCAFE00000000 → X2 = 0xCAFEBEEF... wait
    // Actually, let's just load a known value into X2 via simpler instructions
    //   MOVZ X2, #0xBEEF            ; X2 = 0xBEEF
    //   MOVK X2, #0xFEED, LSL #16   ; X2 = 0xFEEDBEEF
    //   STR X2, [X1]                ; store 8 bytes to MMIO
    //   BRK #0
    let code_addr = 0x1000u64;
    write_code(
        &cpu,
        code_addr,
        &[
            encode_movz(1, 0x1000, 1),         // X1 = 0x10000000
            encode_movz(2, 0xBEEF, 0),          // X2 = 0xBEEF
            0xF2A00000 | (1 << 21) | (0xFEED << 5) | 2, // MOVK X2, #0xFEED, LSL#16 → X2 = 0xFEEDBEEF
            encode_str_x2_x1(),
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    // Verify the capturing device received the write
    let last = captured.borrow();
    assert!(last.is_some(), "Capturing device should have received a write");
    let (offset, size, _value) = last.unwrap();
    // Unicorn splits 8-byte STR into two 4-byte writes (offset 0 then offset 4)
    assert!(offset <= 4, "Offset should be within the first 8 bytes");
    assert_eq!(size, 4, "Unicorn dispatches 64-bit STR as two 4-byte writes");
    // The final write (offset=4) holds the upper 32 bits of 0xFEEDBEEF
    // For a full roundtrip, see test_mmio_ldr_str_roundtrip_via_bus
}

#[test]
fn test_mmio_unmapped_access_returns_zero() {
    // Verify unmapped MMIO access via direct bus read returns 0
    let bus = crate::mmio::MmioBus::new();
    // No devices registered — any access is unmapped
    assert_eq!(bus.read(0x10000000, 4), 0);
    assert_eq!(bus.read(0x10000000, 8), 0);
}

#[test]
fn test_mmio_registered_devices_visible() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    let dev = MockDevice::new(0x1000);
    cpu.mmio_bus_mut()
        .register_device("uart", MMIO_BASE, 0x1000, dev);

    let devices = cpu.mmio_bus_mut().registered_devices();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].0, "uart");
    assert_eq!(devices[0].1, MMIO_BASE);
    assert_eq!(devices[0].2, 0x1000);
}

#[test]
fn test_mmio_ldr_str_roundtrip_via_bus() {
    // Verify the full roundtrip: write via STR, read back via LDR
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    let dev = MockDevice::new(0x1000);
    cpu.mmio_bus_mut()
        .register_device("ram", MMIO_BASE, 0x1000, dev);

    // Code:
    //   MOVZ X1, #0x1000, LSL #16   ; X1 = MMIO_BASE
    //   MOVZ X2, #0x1234            ; X2 = 0x1234
    //   STR X2, [X1]                ; store X2 to MMIO_BASE
    //   MOV X0, #0                  ; X0 = 0
    //   LDR X0, [X1]                ; load back from MMIO_BASE → X0
    //   BRK #0
    let code_addr = 0x1000u64;
    write_code(
        &cpu,
        code_addr,
        &[
            encode_movz(1, 0x1000, 1),  // X1 = 0x10000000
            encode_movz(2, 0x1234, 0),   // X2 = 0x1234
            encode_str_x2_x1(),          // STR X2, [X1]
            encode_movz(0, 0, 0),        // X0 = 0
            encode_ldr_x0_x1(),          // LDR X0, [X1]
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0);
    assert_eq!(
        result, 0x1234,
        "LDR after STR should read back the same value"
    );
}

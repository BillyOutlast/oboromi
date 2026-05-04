//! Integration tests for the eFuse MMIO device — verify that the fuse array
//! responds to real ARM64 load instructions via the Unicorn emulator.
//!
//! These tests exercise the full MMIO path: register device → encode ARM64
//! instructions → run emulation → read register values back.

use crate::cpu::UnicornCPU;
use crate::security::efuse::{EfuseArray, EFUSE_BASE, EFUSE_SIZE};

// ── ARM64 instruction helpers ────────────────────────────────────

/// Encode `LDR Xt, [Xn]` (unsigned offset, 64-bit, offset=0).
fn encode_ldr_x0_x1() -> u32 {
    0xF940_0020
}

/// Encode `LDR Wt, [Xn]` (unsigned offset, 32-bit, offset=0).
fn encode_ldr_w0_x1() -> u32 {
    0xB940_0020
}

/// Encode `LDR Xt, [Xn, #imm12]` (positive unsigned offset, scaled by 8).
/// `imm12` is the byte offset; the encoder scales it to the 12-bit field.
/// Panics if `imm12` is not a multiple of 8 or exceeds 32760.
fn encode_ldr_x_offset(rt: u32, rn: u32, imm12: u32) -> u32 {
    assert!(imm12 % 8 == 0, "LDR X offset must be multiple of 8");
    assert!(imm12 <= 32760, "LDR X offset out of range");
    let imm = imm12 / 8;
    0xF940_0000 | (imm << 10) | (rn << 5) | rt
}

/// Encode `LDR Wt, [Xn, #imm12]` (positive unsigned offset, scaled by 4).
/// Panics if `imm12` is not a multiple of 4 or exceeds 16380.
fn encode_ldr_w_offset(rt: u32, rn: u32, imm12: u32) -> u32 {
    assert!(imm12 % 4 == 0, "LDR W offset must be multiple of 4");
    assert!(imm12 <= 16380, "LDR W offset out of range");
    let imm = imm12 / 4;
    0xB940_0000 | (imm << 10) | (rn << 5) | rt
}

/// Encode `STR Xt, [Xn, #imm12]` (positive unsigned offset, scaled by 8).
fn encode_str_x_offset(rt: u32, rn: u32, imm12: u32) -> u32 {
    assert!(imm12 % 8 == 0, "STR X offset must be multiple of 8");
    assert!(imm12 <= 32760, "STR X offset out of range");
    let imm = imm12 / 8;
    0xF900_0000 | (imm << 10) | (rn << 5) | rt
}

/// Encode `MOVZ Xd, #imm16, LSL #(hw*16)`.
fn encode_movz(d: u32, imm16: u32, hw: u32) -> u32 {
    0xD280_0000 | (hw << 21) | (imm16 << 5) | d
}

/// Encode `MOVK Xd, #imm16, LSL #(hw*16)`.
fn encode_movk(d: u32, imm16: u32, hw: u32) -> u32 {
    0xF280_0000 | (hw << 21) | (imm16 << 5) | d
}

/// Encode `BRK #0` — halts emulation.
fn encode_brk() -> u32 {
    0xD420_0000
}

/// Write MOVZ+MOVK pair to load EFUSE_BASE (0x7000F800) into Xd.
/// Returns two instructions: `[MOVZ, MOVK]`.
fn encode_load_efuse_base_into(d: u32) -> [u32; 2] {
    [
        encode_movz(d, 0xF800, 0),      // MOVZ Xd, #0xF800
        encode_movk(d, 0x7000, 1),      // MOVK Xd, #0x7000, LSL #16 → Xd = 0x7000F800
    ]
}

/// Write a sequence of 32-bit instructions into the UnicornCPU memory.
fn write_code(cpu: &UnicornCPU, addr: u64, insns: &[u32]) {
    for (i, insn) in insns.iter().enumerate() {
        cpu.write_u32(addr + (i as u64) * 4, *insn);
    }
}

// ── Integration tests ────────────────────────────────────────────

#[test]
fn test_efuse_registers_on_mmio_bus() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    // Register eFuse at its TRM-defined base address (EFUSE_BASE).
    // This is a direct bus registration — not accessed via Unicorn LDR hooks
    // (EFUSE_BASE is outside the mmio_map hook range), but find_device works
    // against the bus directly.
    let efuse = EfuseArray::new_t210();
    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, efuse);

    // find_device should locate the eFuse within its range
    let bus = cpu.mmio_bus_ref();
    let result = bus.find_device(EFUSE_BASE);
    assert!(result.is_some(), "eFuse should be found at EFUSE_BASE");
    let (name, base, size) = result.unwrap();
    assert_eq!(name, "efuse");
    assert_eq!(base, EFUSE_BASE);
    assert_eq!(size, EFUSE_SIZE);

    // Out-of-range address should NOT find the device
    assert!(
        bus.find_device(EFUSE_BASE + EFUSE_SIZE).is_none(),
        "Address past end of eFuse region should not map to device"
    );
}

#[test]
fn test_efuse_read_chip_id_via_ldr() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    // Register eFuse at EFUSE_BASE (0x7000F800) — the dedicated mmio_map
    // hook in UnicornCPU intercepts ARM64 LDR targeting this address.
    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // Code at 0x1000:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE (0x7000F800)
    //   LDR W0, [X1]                ; load 32-bit chip ID → W0 (X0 low 32b)
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                       // MOVZ X1, #0xF800
            movk,                       // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_ldr_w0_x1(),         // LDR W0, [X1] → reads offset 0 (chip ID)
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    // X0 low 32 bits should be the chip ID: 0x00000035
    let result = cpu.get_x(0);
    assert_eq!(
        result & 0xFFFF_FFFF, 0x0000_0035,
        "LDR from eFuse base should return chip ID 0x35 (T210 Erista)"
    );
}

#[test]
fn test_efuse_read_vendor_code_via_ldr() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // Code at 0x1000:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE
    //   LDR W0, [X1, #4]            ; load 32-bit from offset 4 → vendor code
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                           // MOVZ X1, #0xF800
            movk,                           // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_ldr_w_offset(0, 1, 4),   // LDR W0, [X1, #4]
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0) & 0xFFFF_FFFF;
    assert_eq!(
        result, 0x4E56_4944,
        "LDR from eFuse offset 4 should return vendor code 'NVID' (0x4E564944)"
    );
}

#[test]
fn test_efuse_read_dram_config_via_ldr() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // DRAM config is at offset 0x100 — word 0 of Reserved_ODM4.
    // Value: 0x00000004 (DRAM size = 4 GB).
    // Code:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE
    //   MOVZ X2, #0x100             ; X2 = 0x100 (offset for DRAM config)
    //   ADD X1, X1, X2              ; X1 = EFUSE_BASE + 0x100
    //   LDR W0, [X1]                ; load DRAM config word
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                       // MOVZ X1, #0xF800
            movk,                       // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_movz(2, 0x100, 0),   // MOVZ X2, #0x100
            0x8B02_0021,                 // ADD X1, X1, X2
            encode_ldr_w0_x1(),          // LDR W0, [X1]
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0) & 0xFFFF_FFFF;
    assert_eq!(
        result, 0x0000_0004,
        "LDR from eFuse offset 0x100 should return DRAM size = 4 GB"
    );
}

#[test]
fn test_efuse_read_security_flags_via_ldr() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // Security flags at offset 0x108 (word 2 of Reserved_ODM4).
    // Value: 0x00000001 (secure boot enabled).
    // Code:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE
    //   MOVZ X2, #0x108             ; X2 = 0x108
    //   ADD X1, X1, X2              ; X1 = EFUSE_BASE + 0x108
    //   LDR W0, [X1]                ; load security flags
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                       // MOVZ X1, #0xF800
            movk,                       // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_movz(2, 0x108, 0),   // MOVZ X2, #0x108
            0x8B02_0021,                 // ADD X1, X1, X2
            encode_ldr_w0_x1(),          // LDR W0, [X1]
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0) & 0xFFFF_FFFF;
    assert_eq!(
        result, 0x0000_0001,
        "LDR from eFuse offset 0x108 should return security flags = 0x01 (secure boot enabled)"
    );
}

#[test]
fn test_efuse_str_is_noop() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // STR a different value to the chip ID location, then LDR back.
    // The eFuse rejects writes — the LDR should still return the original.
    // Code:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE (0x7000F800)
    //   MOVZ X2, #0xDEAD            ; X2 = 0xDEAD
    //   MOVK X2, #0xBEEF, LSL #16   ; X2 = 0xBEEFDEAD
    //   STR W2, [X1]                ; try to write new value to chip ID
    //   LDR W0, [X1]                ; read back via LDR
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                           // MOVZ X1, #0xF800
            movk,                           // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_movz(2, 0xDEAD, 0),     // X2 = 0xDEAD
            0xF2A0_0000 | (1 << 21) | (0xBEEF << 5) | 2, // MOVK X2, #0xBEEF, LSL #16
            0xB900_0022,                    // STR W2, [X1] — 32-bit store
            encode_ldr_w0_x1(),             // LDR W0, [X1] — 32-bit load
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0) & 0xFFFF_FFFF;
    assert_eq!(
        result, 0x0000_0035,
        "STR to eFuse chip ID offset should be silently ignored; LDR must return original value 0x35"
    );
}

#[test]
fn test_efuse_unmapped_offset_returns_zero() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // Offset 0x800 is beyond EFUSE_SIZE (0x400) — should return 0.
    // Code:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE
    //   MOVZ X2, #0x800             ; X2 = 0x800
    //   ADD X1, X1, X2              ; X1 = EFUSE_BASE + 0x800
    //   LDR W0, [X1]                ; load from unmapped offset
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                       // MOVZ X1, #0xF800
            movk,                       // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_movz(2, 0x800, 0),   // X2 = 0x800
            0x8B02_0021,                 // ADD X1, X1, X2
            encode_ldr_w0_x1(),          // LDR W0, [X1]
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0) & 0xFFFF_FFFF;
    assert_eq!(
        result, 0,
        "LDR from unmapped eFuse offset 0x800 should return 0"
    );
}

#[test]
fn test_efuse_read_full_64bit_via_ldr() {
    let mut cpu = UnicornCPU::new().expect("Failed to create UnicornCPU");

    cpu.mmio_bus_mut()
        .register_device("efuse", EFUSE_BASE, EFUSE_SIZE, EfuseArray::new_t210());

    // 64-bit LDR from offset 0: combines chip ID (lo) + vendor code (hi)
    // Expected: 0x4E56_4944_0000_0035
    // Code:
    //   MOVZ X1, #0xF800            ; X1 = 0xF800
    //   MOVK X1, #0x7000, LSL #16   ; X1 = EFUSE_BASE (0x7000F800)
    //   LDR X0, [X1]                ; load 64-bit → X0
    //   BRK #0
    let code_addr = 0x1000u64;
    let [movz, movk] = encode_load_efuse_base_into(1);
    write_code(
        &cpu,
        code_addr,
        &[
            movz,                       // MOVZ X1, #0xF800
            movk,                       // MOVK X1, #0x7000, LSL #16 → X1 = EFUSE_BASE
            encode_ldr_x0_x1(),         // LDR X0, [X1]
            encode_brk(),
        ],
    );

    cpu.set_pc(code_addr);
    cpu.run();

    let result = cpu.get_x(0);
    assert_eq!(
        result, 0x4E56_4944_0000_0035,
        "64-bit LDR from eFuse offset 0 should return vendor code : chip ID"
    );
}

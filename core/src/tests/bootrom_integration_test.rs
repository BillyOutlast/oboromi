//! BootROM integration test — full secure boot chain against firmware dump.
//!
//! Exercises the complete BootROM pipeline end-to-end:
//! 1. Create UnicornCPU with EFUSE MMIO hooks at EFUSE_BASE (0x7000F800)
//! 2. Register eFuse device on the MmioBus
//! 3. Create BootRom (RSA-2048 key + KeyDerivation from eFuse)
//! 4. Call boot(cpu, firmware)
//! 5. Verify Package2 is placed at 0x40010000 in CPU memory
//!
//! **Requires a real or stitched firmware dump.** The happy-path integration
//! test is `#[ignore]` when no firmware blob is available.

use crate::cpu::unicorn_interface::UnicornCPU;
use crate::security::bootrom::{BootRom, PACKAGE2_LOAD_ADDR};
use crate::security::efuse::{EfuseArray, EFUSE_BASE, EFUSE_SIZE};

/// Environment variable or filesystem path to a firmware dump.
pub const FIRMWARE_PATH: &str = "fw/package1_enc.bin";

/// Full end-to-end integration test: load firmware dump, run BootROM chain,
/// verify Package2 lands at the correct T210 address.
///
/// Ignored when no firmware dump is present.
#[test]
#[ignore = "requires firmware dump at fw/package1_enc.bin"]
fn bootrom_full_chain_integration() {
    // ── 1. Create UnicornCPU (EFUSE MMIO hooks are registered by constructor) ──
    let mut cpu = UnicornCPU::new()
        .expect("UnicornCPU::new() must succeed");

    // ── 2. Register eFuse device on the MmioBus at EFUSE_BASE ──
    let efuse = EfuseArray::new();
    {
        let mut bus = cpu.mmio_bus_mut();
        bus.register_device("efuse", EFUSE_BASE, EFUSE_SIZE, efuse.clone());
    }

    // ── 3. Create BootRom ──
    let bootrom = BootRom::new(&efuse);

    // ── 4. Load firmware dump ──
    let firmware = std::fs::read(FIRMWARE_PATH)
        .unwrap_or_else(|e| panic!("firmware dump not found at {FIRMWARE_PATH}: {e}"));

    // ── 5. Boot ──
    let result = bootrom
        .boot(&mut cpu, &firmware)
        .expect("BootROM boot() must succeed");

    // ── 6. Verify BootResult diagnostics ──
    assert_eq!(result.package2_load_addr, PACKAGE2_LOAD_ADDR);
    assert!(result.package2_size > 0, "Package2 must be non-empty");
    assert!(
        result.diagnostics.signature_valid,
        "RSA signature must be valid"
    );
    assert!(result.diagnostics.elapsed_us > 0, "diagnostics must record elapsed time");
    assert!(
        result.diagnostics.phases_completed.len() == 7,
        "all 7 boot phases must complete, got {}",
        result.diagnostics.phases_completed.len()
    );
    assert_eq!(
        result.diagnostics.pk11_magic,
        0x504B_3131,
        "PK11 magic must be 0x504B3131"
    );

    // ── 7. Read Package2 from CPU memory at 0x40010000 ──
    // Package2 starts with the kernel binary. We validate the first 4 bytes
    // are non-zero (a real kernel image won't be all zeros).
    let first_word = cpu.read_u32(PACKAGE2_LOAD_ADDR);
    assert_ne!(
        first_word, 0,
        "Package2 at 0x{PACKAGE2_LOAD_ADDR:08X} must be non-zero (kernel image)"
    );

    // Verify the last word of Package2 is readable
    let last_addr = PACKAGE2_LOAD_ADDR + (result.package2_size as u64) - 4;
    let _last_word = cpu.read_u32(last_addr);

    println!(
        "BootROM integration: Package2 {} bytes loaded at 0x{PACKAGE2_LOAD_ADDR:08X} ({} µs)",
        result.package2_size, result.diagnostics.elapsed_us
    );
}

/// Smoke test: construct the full BootROM chain without a firmware dump.
/// This test always runs — it validates that all types compose and the
/// constructors don't panic.
#[test]
fn bootrom_full_chain_smoke() {
    let mut cpu = UnicornCPU::new()
        .expect("UnicornCPU::new() must succeed");

    let efuse = EfuseArray::new();
    {
        let mut bus = cpu.mmio_bus_mut();
        bus.register_device("efuse", EFUSE_BASE, EFUSE_SIZE, efuse.clone());
    }

    let bootrom = BootRom::new(&efuse);

    // Verify BootRom Debug impl doesn't leak secrets
    let debug_str = format!("{bootrom:?}");
    assert!(
        !debug_str.contains("rsa_pub") && !debug_str.contains("sbk"),
        "Debug impl must not leak key material"
    );
}

/// Negative test: boot with too-short firmware produces the correct error.
#[test]
fn bootrom_integration_too_short_firmware() {
    let mut cpu = UnicornCPU::new()
        .expect("UnicornCPU::new() must succeed");

    let efuse = EfuseArray::new();
    {
        let mut bus = cpu.mmio_bus_mut();
        bus.register_device("efuse", EFUSE_BASE, EFUSE_SIZE, efuse.clone());
    }

    let bootrom = BootRom::new(&efuse);
    let err = bootrom.boot(&mut cpu, &[0xAA; 10]).unwrap_err();
    assert!(
        err.to_string().contains("too short"),
        "expected 'too short' error, got: {err}"
    );
}

/// Negative test: firmware with bad PK11 magic produces the correct error.
#[test]
fn bootrom_integration_bad_pk11_magic() {
    let mut cpu = UnicornCPU::new()
        .expect("UnicornCPU::new() must succeed");

    let efuse = EfuseArray::new();
    {
        let mut bus = cpu.mmio_bus_mut();
        bus.register_device("efuse", EFUSE_BASE, EFUSE_SIZE, efuse.clone());
    }

    let bootrom = BootRom::new(&efuse);

    // Minimum firmware = 256 (sig) + 256 (PK11 header) + 1 (payload)
    let mut fw = vec![0u8; 513];
    // Set PK11 magic to something invalid
    fw[256..260].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());

    let err = bootrom.boot(&mut cpu, &fw).unwrap_err();
    assert!(
        err.to_string().contains("PK11 parse"),
        "expected PK11 parse error, got: {err}"
    );
}

/// Negative test: firmware with all-zero signature fails RSA verification.
#[test]
fn bootrom_integration_bad_signature() {
    let mut cpu = UnicornCPU::new()
        .expect("UnicornCPU::new() must succeed");

    let efuse = EfuseArray::new();
    {
        let mut bus = cpu.mmio_bus_mut();
        bus.register_device("efuse", EFUSE_BASE, EFUSE_SIZE, efuse.clone());
    }

    let bootrom = BootRom::new(&efuse);

    let mut fw = vec![0u8; 513];
    // Valid PK11 magic
    fw[256..260].copy_from_slice(&0x504B_3131u32.to_le_bytes());
    fw[260..264].copy_from_slice(&1u32.to_le_bytes());
    fw[264..272].copy_from_slice(&1u64.to_le_bytes());
    // Signature is all zeros → RSA verify fails

    let err = bootrom.boot(&mut cpu, &fw).unwrap_err();
    assert!(
        err.to_string().contains("signature"),
        "expected signature verification error, got: {err}"
    );
}

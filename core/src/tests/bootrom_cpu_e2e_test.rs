//! BootROM → CPU end-to-end integration tests.
//!
//! Exercises the full boot chain: firmware construction → BootROM validation →
//! Package2 placement → end-state verification. Uses the real UnicornCPU
//! backing CpuManager, with a custom test RSA keypair so we can both sign
//! and verify without depending on the hardcoded T210 key.

use crate::security::bootrom::{BootPhase, BootError, PACKAGE2_LOAD_ADDR};
use crate::security::efuse::EfuseArray;
use crate::security::rsa::generate_test_keypair;
use crate::sys::State;
use crate::tests::firmware_builder::MinimalFirmware;

/// ARMv8 NOP instruction encoding (matches firmware_builder's NOP sled).
const ARM64_NOP: u32 = 0xD503_201F;

// ── End-to-end boot succeeds ──────────────────────────────────────

#[test]
fn bootrom_cpu_e2e_boot_succeeds() {
    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();

    // Build valid firmware signed with the test private key
    let fw = MinimalFirmware::build(&efuse, &priv_key);
    let fw_bytes = fw.as_bytes();

    let mut state = State::new();
    let result = state
        .boot_rom_with_key(&efuse, fw_bytes, &pub_key)
        .expect("boot should succeed with valid firmware");

    assert_eq!(result.phase, BootPhase::Package2Placement);
    assert_eq!(result.package2_load_addr, PACKAGE2_LOAD_ADDR);
    assert_eq!(result.package2_size, 64, "Package2 should be 64 bytes (16 NOPs)");

    let diag = &result.diagnostics;
    assert_eq!(diag.phases_completed.len(), 7, "all 7 boot phases must complete");
    assert!(diag.signature_valid, "signature must be reported valid");
}

// ── Verify Package2 in core 0 memory after boot ───────────────────

#[test]
fn bootrom_cpu_e2e_package2_in_memory() {
    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();

    let fw = MinimalFirmware::build(&efuse, &priv_key);
    let fw_bytes = fw.as_bytes();

    let mut state = State::new();
    let result = state
        .boot_rom_with_key(&efuse, fw_bytes, &pub_key)
        .expect("boot should succeed");

    assert_eq!(result.package2_size, 64);

    // Read back the first word at the Package2 load address from core 0
    let core = state
        .cpu_manager
        .get_core(0)
        .expect("core 0 must exist");
    let first_word = core.read_u32(PACKAGE2_LOAD_ADDR);

    assert_eq!(
        first_word, ARM64_NOP,
        "first word at 0x{PACKAGE2_LOAD_ADDR:08X} must be ARMv8 NOP"
    );

    // Also verify the last word (offset 60 bytes = 15 * 4) is a NOP
    let last_word = core.read_u32(PACKAGE2_LOAD_ADDR + 60);
    assert_eq!(
        last_word, ARM64_NOP,
        "last word at offset 60 must also be ARMv8 NOP"
    );
}

// ── Bad signature should fail ─────────────────────────────────────

#[test]
fn bootrom_cpu_e2e_bad_signature_fails() {
    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();

    let mut fw = MinimalFirmware::build(&efuse, &priv_key);
    let fw_bytes_mut = fw.into_vec();

    // Tamper with a byte in the signature region (first 256 bytes)
    let mut tampered = fw_bytes_mut;
    tampered[0] ^= 0xFF; // flip all bits of the first byte

    let mut state = State::new();
    let err = state
        .boot_rom_with_key(&efuse, &tampered, &pub_key)
        .expect_err("boot must fail with a tampered signature");

    assert!(
        matches!(err, BootError::SignatureVerify(_)),
        "expected SignatureVerify error, got: {err:?}"
    );
}

// ── Boot phase ordering is correct ────────────────────────────────

#[test]
fn bootrom_cpu_e2e_phase_ordering() {
    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();
    let fw = MinimalFirmware::build(&efuse, &priv_key);

    let mut state = State::new();
    let result = state
        .boot_rom_with_key(&efuse, fw.as_bytes(), &pub_key)
        .expect("boot should succeed");

    let phases = &result.diagnostics.phases_completed;
    assert_eq!(phases.len(), 7);
    assert_eq!(phases[0], BootPhase::EfuseInit);
    assert_eq!(phases[1], BootPhase::KeyDerivation);
    assert_eq!(phases[2], BootPhase::Pk11Parse);
    assert_eq!(phases[3], BootPhase::RsaVerify);
    assert_eq!(phases[4], BootPhase::CtrDecrypt);
    assert_eq!(phases[5], BootPhase::Pk11Validate);
    assert_eq!(phases[6], BootPhase::Package2Placement);
}

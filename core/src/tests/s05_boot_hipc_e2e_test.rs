//! S05: Boot + HIPC end-to-end integration tests.
//!
//! These tests prove the full firmware→boot→IPC round-trip in a single
//! State instance, exercising the composed boot chain and HIPC dispatch
//! together. Error paths (bad signature, service-not-found, no-host-services)
//! are exercised through the full State lifecycle for integration-level
//! observability beyond the unit-level hipc_sm_test.

use crate::nn::hipc::{DispatchError, HipcRouter};
use crate::nn::{self, sm};
use crate::security::bootrom::BootError;
use crate::security::efuse::EfuseArray;
use crate::security::rsa::generate_test_keypair;
use crate::sys::State;
use crate::tests::firmware_builder::MinimalFirmware;

// ── Helpers (duplicated from hipc_sm_test.rs — those are private) ───────

/// Build a minimal HIPC request buffer: 8-byte header + method_id u32 +
/// payload padded to word boundary.
fn build_hipc_message(method_id: u32, payload: &[u8]) -> Vec<u8> {
    let total_payload = 4usize + payload.len(); // method_id + payload
    let pad = (4 - (total_payload % 4)) % 4;
    let raw_words = ((total_payload + pad) / 4) as u32;

    let hdr0 = 0u32; // tag=Request, no descriptors
    let hdr1 = raw_words & 0x3FF;

    let mut buf = Vec::with_capacity(8 + total_payload + pad);
    buf.extend_from_slice(&hdr0.to_le_bytes());
    buf.extend_from_slice(&hdr1.to_le_bytes());
    buf.extend_from_slice(&method_id.to_le_bytes());
    buf.extend_from_slice(payload);
    buf.resize(8 + total_payload + pad, 0u8);
    buf
}

/// Build a RegisterService payload: 4-byte name_len LE + UTF-8 name bytes.
fn register_payload(name: &str) -> Vec<u8> {
    let bytes = name.as_bytes();
    let mut p = Vec::with_capacity(4 + bytes.len());
    p.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    p.extend_from_slice(bytes);
    p
}

/// Build a GetServiceHandle payload: 4-byte handle_id LE.
fn get_handle_payload(handle_id: u32) -> Vec<u8> {
    handle_id.to_le_bytes().to_vec()
}

// ── T01: Boot + HIPC e2e tests ──────────────────────────────────────────

/// Full boot chain → RegisterService → GetServiceHandle round-trip
/// through the composed State lifecycle.
#[test]
fn test_boot_then_hipc_roundtrip() {
    sm::reset_handle_table();

    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();
    let fw = MinimalFirmware::build(&efuse, &priv_key);

    // ── 1. Create State, register host services, boot ────────────
    let mut state = State::new();
    nn::start_host_services(&mut state);

    let boot_result = state
        .boot_rom_with_key(&efuse, fw.as_bytes(), &pub_key)
        .expect("boot must succeed with valid firmware");
    assert!(
        boot_result.diagnostics.signature_valid,
        "signature must be reported valid"
    );
    assert_eq!(
        boot_result.diagnostics.phases_completed.len(),
        7,
        "all 7 boot phases must complete"
    );

    // ── 2. RegisterService("spl") via dispatch_message ───────────
    let reg_buf = build_hipc_message(0, &register_payload("spl"));
    let reg_resp = state
        .hipc_router
        .dispatch_message(&reg_buf, "sm")
        .expect("RegisterService dispatch must succeed");
    assert_eq!(reg_resp.result_code, 0, "RegisterService must return success");
    assert_eq!(
        reg_resp.data.len(),
        4,
        "Response must contain a 4-byte handle_id"
    );
    let handle_id = u32::from_le_bytes(reg_resp.data[..4].try_into().unwrap());
    assert!(
        handle_id > 0,
        "Handle ID must be non-zero (0 is INVALID_HANDLE sentinel)"
    );

    // ── 3. GetServiceHandle(handle_id) via dispatch_message ──────
    let get_buf = build_hipc_message(1, &get_handle_payload(handle_id));
    let get_resp = state
        .hipc_router
        .dispatch_message(&get_buf, "sm")
        .expect("GetServiceHandle dispatch must succeed");
    assert_eq!(get_resp.result_code, 0, "GetServiceHandle must return success");

    let name_len = u32::from_le_bytes(get_resp.data[..4].try_into().unwrap()) as usize;
    let name = std::str::from_utf8(&get_resp.data[4..4 + name_len]).unwrap();
    assert_eq!(name, "spl", "GetServiceHandle must return the original service name");
}

/// Tampered firmware (first signature byte flipped) → boot fails with
/// SignatureVerify. The HIPC router (populated before boot via
/// start_host_services) must remain usable after the boot error.
#[test]
fn test_bad_signature_blocks_boot_hipc_clean() {
    sm::reset_handle_table();

    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();
    let fw = MinimalFirmware::build(&efuse, &priv_key);
    let mut fw_bytes = fw.into_vec();

    // Tamper with the first byte of the RSA signature (first 256 bytes).
    fw_bytes[0] ^= 0xFF;

    // ── 1. Create State, register host services (BEFORE boot) ────
    let mut state = State::new();
    nn::start_host_services(&mut state);

    // ── 2. Boot must fail with tampered signature ─────────────────
    let boot_err = state
        .boot_rom_with_key(&efuse, &fw_bytes, &pub_key)
        .expect_err("boot must fail with a tampered signature");
    assert!(
        matches!(boot_err, BootError::SignatureVerify(_)),
        "expected SignatureVerify error, got: {boot_err:?}"
    );

    // ── 3. HIPC router must still be usable after boot failure ────
    // sm handlers were registered by start_host_services before boot.
    let reg_buf = build_hipc_message(0, &register_payload("test"));
    let reg_resp = state
        .hipc_router
        .dispatch_message(&reg_buf, "sm")
        .expect("RegisterService must still work after boot failure");
    assert_eq!(
        reg_resp.result_code, 0,
        "RegisterService must return success even after boot failure"
    );
    let handle_id = u32::from_le_bytes(reg_resp.data[..4].try_into().unwrap());
    assert!(handle_id > 0, "handle_id must be valid");

    // Also verify the round-trip: GetServiceHandle on the new handle
    let get_buf = build_hipc_message(1, &get_handle_payload(handle_id));
    let get_resp = state
        .hipc_router
        .dispatch_message(&get_buf, "sm")
        .expect("GetServiceHandle must still work after boot failure");
    assert_eq!(get_resp.result_code, 0);
}

/// Boot succeeds → start_host_services → dispatch to a nonexistent
/// service name must return ServiceNotFound.
#[test]
fn test_service_not_found_after_boot() {
    sm::reset_handle_table();

    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();
    let fw = MinimalFirmware::build(&efuse, &priv_key);

    let mut state = State::new();
    nn::start_host_services(&mut state);

    let _boot_result = state
        .boot_rom_with_key(&efuse, fw.as_bytes(), &pub_key)
        .expect("boot must succeed");

    // Dispatch to a service name that was never registered.
    let reg_buf = build_hipc_message(0, &register_payload("spl"));
    let result = state.hipc_router.dispatch_message(&reg_buf, "nonexistent");

    assert!(
        result.is_err(),
        "dispatch to nonexistent service must fail"
    );
    assert!(
        matches!(result.unwrap_err(), DispatchError::ServiceNotFound),
        "expected ServiceNotFound for nonexistent service"
    );
}

/// Boot succeeds WITHOUT calling start_host_services →
/// dispatch to "sm" must return ServiceNotFound since no sm
/// handlers were registered.
#[test]
fn test_hipc_without_start_host_services() {
    sm::reset_handle_table();

    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();
    let fw = MinimalFirmware::build(&efuse, &priv_key);

    let mut state = State::new();
    // NOTE: intentionally NOT calling start_host_services()

    let _boot_result = state
        .boot_rom_with_key(&efuse, fw.as_bytes(), &pub_key)
        .expect("boot must succeed");

    // Dispatch to "sm" — its handlers were never registered.
    let reg_buf = build_hipc_message(0, &register_payload("test"));
    let result = state.hipc_router.dispatch_message(&reg_buf, "sm");

    assert!(
        result.is_err(),
        "dispatch to sm must fail when start_host_services was never called"
    );
    assert!(
        matches!(result.unwrap_err(), DispatchError::ServiceNotFound),
        "expected ServiceNotFound when no sm handlers registered"
    );
}

/// Boot → start_host_services → register two services ("spl", "hid")
/// via dispatch → look up both handles → assert names match.
#[test]
fn test_multiple_register_services_after_boot() {
    sm::reset_handle_table();

    let efuse = EfuseArray::new();
    let (pub_key, priv_key) = generate_test_keypair();
    let fw = MinimalFirmware::build(&efuse, &priv_key);

    let mut state = State::new();
    nn::start_host_services(&mut state);

    let _boot_result = state
        .boot_rom_with_key(&efuse, fw.as_bytes(), &pub_key)
        .expect("boot must succeed");

    // ── Register "spl" ──────────────────────────────────────────
    let reg_buf = build_hipc_message(0, &register_payload("spl"));
    let resp = state
        .hipc_router
        .dispatch_message(&reg_buf, "sm")
        .expect("RegisterService spl must succeed");
    assert_eq!(resp.result_code, 0);
    let spl_id = u32::from_le_bytes(resp.data[..4].try_into().unwrap());
    assert!(spl_id > 0);

    // ── Register "hid" ──────────────────────────────────────────
    let reg_buf = build_hipc_message(0, &register_payload("hid"));
    let resp = state
        .hipc_router
        .dispatch_message(&reg_buf, "sm")
        .expect("RegisterService hid must succeed");
    assert_eq!(resp.result_code, 0);
    let hid_id = u32::from_le_bytes(resp.data[..4].try_into().unwrap());
    assert!(hid_id > 0);
    assert_ne!(spl_id, hid_id, "handle IDs must be distinct");

    // ── Look up "spl" ───────────────────────────────────────────
    let get_buf = build_hipc_message(1, &get_handle_payload(spl_id));
    let get_resp = state
        .hipc_router
        .dispatch_message(&get_buf, "sm")
        .expect("GetServiceHandle spl must succeed");
    let name_len = u32::from_le_bytes(get_resp.data[..4].try_into().unwrap()) as usize;
    let name = std::str::from_utf8(&get_resp.data[4..4 + name_len]).unwrap();
    assert_eq!(name, "spl");

    // ── Look up "hid" ───────────────────────────────────────────
    let get_buf = build_hipc_message(1, &get_handle_payload(hid_id));
    let get_resp = state
        .hipc_router
        .dispatch_message(&get_buf, "sm")
        .expect("GetServiceHandle hid must succeed");
    let name_len = u32::from_le_bytes(get_resp.data[..4].try_into().unwrap()) as usize;
    let name = std::str::from_utf8(&get_resp.data[4..4 + name_len]).unwrap();
    assert_eq!(name, "hid");
}

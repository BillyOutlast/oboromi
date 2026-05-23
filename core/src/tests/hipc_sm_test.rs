//! Integration tests for Service Manager (sm) via HipcRouter::dispatch_message.
//!
//! These tests prove the full round-trip: raw HIPC bytes → parse → dispatch
//! → sm handler → HandleTable → response, matching the S02 slice demo
//! contract.

use crate::nn::hipc::HipcRouter;
use crate::nn::sm;

// ── Helpers ─────────────────────────────────────────────────────────────

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

// ── Integration Tests ───────────────────────────────────────────────────

#[test]
fn test_register_service_returns_valid_handle() {
    // Reset the handle table for isolation.
    sm::reset_handle_table();

    let mut router = HipcRouter::new();
    router.register("sm", 0, sm::handler_register_service);
    router.register("sm", 1, sm::handler_get_service_handle);

    let buf = build_hipc_message(0, &register_payload("spl"));
    let resp = router.dispatch_message(&buf, "sm").unwrap();

    assert_eq!(resp.result_code, 0, "RegisterService must return success");
    assert_eq!(resp.data.len(), 4, "Response must contain a 4-byte handle_id");
    let handle_id = u32::from_le_bytes(resp.data[..4].try_into().unwrap());
    assert!(handle_id > 0, "Handle ID must be non-zero (0 is INVALID_HANDLE sentinel)");
}

#[test]
fn test_get_service_handle_returns_name() {
    sm::reset_handle_table();

    let mut router = HipcRouter::new();
    router.register("sm", 0, sm::handler_register_service);
    router.register("sm", 1, sm::handler_get_service_handle);

    // Register a service first using the direct handler
    let reg_resp = sm::handler_register_service(&register_payload("hid"));
    assert_eq!(reg_resp.result_code, 0);
    let handle_id = u32::from_le_bytes(reg_resp.data[..4].try_into().unwrap());

    // Now look it up via dispatch_message
    let buf = build_hipc_message(1, &get_handle_payload(handle_id));
    let resp = router.dispatch_message(&buf, "sm").unwrap();

    assert_eq!(resp.result_code, 0, "GetServiceHandle must return success");
    assert!(resp.data.len() >= 4, "Response must contain name_len + name bytes");
    let name_len = u32::from_le_bytes(resp.data[..4].try_into().unwrap()) as usize;
    assert_eq!(name_len, 3, "Name length must match 'hid' (3 bytes)");
    let name = std::str::from_utf8(&resp.data[4..4 + name_len]).unwrap();
    assert_eq!(name, "hid");
}

#[test]
fn test_get_service_handle_invalid_id() {
    sm::reset_handle_table();

    // Call handler directly with a nonexistent handle_id
    let resp = sm::handler_get_service_handle(&0xDEAD_BEEFu32.to_le_bytes());
    assert_eq!(
        resp.result_code,
        crate::kernel::handle_table::result::INVALID_HANDLE,
        "Nonexistent handle_id must return INVALID_HANDLE (0xD401)"
    );
}

#[test]
fn test_dispatch_message_malformed() {
    let router = HipcRouter::new();

    // Buffer shorter than 8-byte header
    let result = router.dispatch_message(&[0xAA, 0xBB], "sm");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        crate::nn::hipc::DispatchError::MalformedMessage
    ));
}

#[test]
fn test_register_service_roundtrip() {
    sm::reset_handle_table();

    let mut router = HipcRouter::new();
    router.register("sm", 0, sm::handler_register_service);
    router.register("sm", 1, sm::handler_get_service_handle);

    // Step 1: RegisterService via dispatch_message
    let reg_buf = build_hipc_message(0, &register_payload("spl"));
    let reg_resp = router.dispatch_message(&reg_buf, "sm").unwrap();
    assert_eq!(reg_resp.result_code, 0);
    let handle_id = u32::from_le_bytes(reg_resp.data[..4].try_into().unwrap());
    assert!(handle_id > 0);

    // Step 2: GetServiceHandle via dispatch_message with the handle_id
    let get_buf = build_hipc_message(1, &get_handle_payload(handle_id));
    let get_resp = router.dispatch_message(&get_buf, "sm").unwrap();
    assert_eq!(get_resp.result_code, 0);

    // Step 3: Verify the returned name matches original
    let name_len = u32::from_le_bytes(get_resp.data[..4].try_into().unwrap()) as usize;
    let name = std::str::from_utf8(&get_resp.data[4..4 + name_len]).unwrap();
    assert_eq!(name, "spl");
}

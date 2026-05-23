//! Service Manager (sm) — the first functional HIPC service.
//!
//! Provides RegisterService (method 0) and GetServiceHandle (method 1)
//! backed by a thread-local [`HandleTable`]. Handlers are registered
//! into the [`HipcRouter`] during [`start_host_services`].

use crate::kernel::handle_table::{HandleTable, KernelObject};
use crate::nn::hipc::HipcResponse;
use crate::nn::ServiceTrait;
use crate::sys;
use log::{trace, warn};
use std::cell::RefCell;

// ── State ───────────────────────────────────────────────────────────────

/// Empty state struct matching the `define_service!` convention.
pub struct State {}

impl State {
    pub fn new(_state: &mut sys::State) -> Self {
        Self {}
    }
}

impl ServiceTrait for State {
    fn run(state: &mut sys::State) {
        state.services.sm = Some(State::new(state));
    }
}

// ── Per-thread handle table ─────────────────────────────────────────────

thread_local! {
    /// Global service-manager handle table. Stores KernelObject::Session
    /// entries keyed by handle ID.
    static SM_HANDLE_TABLE: RefCell<HandleTable> = RefCell::new(HandleTable::new());
}

// ── Handlers ────────────────────────────────────────────────────────────

/// RegisterService (method_id=0).
///
/// Input: 4-byte name_len LE + UTF-8 name bytes.
/// Output: 4-byte handle_id LE on success, or error result code.
pub fn handler_register_service(data: &[u8]) -> HipcResponse {
    if data.len() < 4 {
        warn!("sm::handler_register_service: data too short for name_len (got {} bytes)", data.len());
        return HipcResponse::new(crate::kernel::handle_table::result::INVALID_HANDLE);
    }
    let name_len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    let name_end = 4usize.checked_add(name_len).filter(|&end| end <= data.len());
    let name = match name_end {
        Some(end) => {
            String::from_utf8_lossy(&data[4..end]).into_owned()
        }
        None => {
            warn!(
                "sm::handler_register_service: name_len={} overflows (data={} bytes)",
                name_len, data.len()
            );
            return HipcResponse::new(crate::kernel::handle_table::result::INVALID_HANDLE);
        }
    };

    SM_HANDLE_TABLE.with(|ht| {
        let mut ht = ht.borrow_mut();
        match ht.create_handle(KernelObject::Session(name)) {
            Ok(raw_id) => {
                let handle_id = raw_id + 1; // 0 is reserved as invalid handle
                trace!(
                    "sm::RegisterService: created handle_id={} (raw={})",
                    handle_id,
                    raw_id
                );
                HipcResponse::with_data(0, handle_id.to_le_bytes().to_vec())
            }
            Err(code) => {
                warn!("sm::RegisterService: create_handle failed err={:#x}", code);
                HipcResponse::new(code)
            }
        }
    })
}

/// GetServiceHandle (method_id=1).
///
/// Input: 4-byte handle_id LE.
/// Output: 4-byte name_len LE + UTF-8 name bytes, or error result code.
pub fn handler_get_service_handle(data: &[u8]) -> HipcResponse {
    if data.len() < 4 {
        warn!("sm::handler_get_service_handle: data too short (got {} bytes)", data.len());
        return HipcResponse::new(crate::kernel::handle_table::result::INVALID_HANDLE);
    }
    let external_id = u32::from_le_bytes(data[..4].try_into().unwrap());

    // External handle IDs are offset by +1 (0 is INVALID_HANDLE sentinel).
    // Map back to the internal HandleTable slot index.
    if external_id == 0 {
        warn!("sm::GetServiceHandle: received invalid handle_id=0");
        return HipcResponse::new(crate::kernel::handle_table::result::INVALID_HANDLE);
    }
    let raw_id = external_id - 1;

    SM_HANDLE_TABLE.with(|ht| {
        let ht = ht.borrow();
        let session_obj = KernelObject::Session(String::new()); // type discriminant only
        let obj = match ht.get_handle(raw_id, &session_obj) {
            Ok(obj) => obj,
            Err(code) => {
                warn!(
                    "sm::GetServiceHandle: get_handle({}) failed err={:#x}",
                    raw_id, code
                );
                return HipcResponse::new(code);
            }
        };
        // Clone the name out — we must hold the ref while reading
        let name = match obj {
            KernelObject::Session(name) => name.clone(),
            _ => {
                warn!(
                    "sm::GetServiceHandle: type mismatch for handle_id={}",
                    raw_id
                );
                return HipcResponse::new(
                    crate::kernel::handle_table::result::INVALID_HANDLE,
                );
            }
        };
        // Release the borrow before mutating
        drop(ht);

        // Release the ref we took during get_handle
        SM_HANDLE_TABLE.with(|ht| {
            let mut ht = ht.borrow_mut();
            let _ = ht.release_handle(raw_id);
        });

        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len() as u32;
        let mut response_data = Vec::with_capacity(4 + name_bytes.len());
        response_data.extend_from_slice(&name_len.to_le_bytes());
        response_data.extend_from_slice(name_bytes);
        trace!(
            "sm::GetServiceHandle: resolved external_id={} -> '{}'",
            external_id,
            name
        );
        HipcResponse::with_data(0, response_data)
    })
}

/// Reset the thread-local handle table to a fresh empty state.
/// Useful in tests to isolate test cases from each other.
pub fn reset_handle_table() {
    SM_HANDLE_TABLE.with(|ht| {
        *ht.borrow_mut() = HandleTable::new();
    });
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nn::hipc::HipcRouter;

    /// Build a RegisterService payload: name_len + UTF-8 name bytes.
    fn register_payload(name: &str) -> Vec<u8> {
        let bytes = name.as_bytes();
        let mut payload = Vec::with_capacity(4 + bytes.len());
        payload.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        payload.extend_from_slice(bytes);
        payload
    }

    fn get_handle_payload(handle_id: u32) -> Vec<u8> {
        handle_id.to_le_bytes().to_vec()
    }

    #[test]
    fn test_register_service_returns_nonzero_handle() {
        let resp = handler_register_service(&register_payload("ns"));
        assert_eq!(resp.result_code, 0);
        assert_eq!(resp.data.len(), 4);
        let id = u32::from_le_bytes(resp.data[..4].try_into().unwrap());
        assert!(id > 0);
    }

    #[test]
    fn test_register_and_get_handle_round_trip() {
        // Register
        let reg_resp = handler_register_service(&register_payload("fs"));
        assert_eq!(reg_resp.result_code, 0);
        let id = u32::from_le_bytes(reg_resp.data[..4].try_into().unwrap());

        // Get
        let get_resp = handler_get_service_handle(&get_handle_payload(id));
        assert_eq!(get_resp.result_code, 0);
        let name_len = u32::from_le_bytes(get_resp.data[..4].try_into().unwrap()) as usize;
        let name = std::str::from_utf8(&get_resp.data[4..4 + name_len]).unwrap();
        assert_eq!(name, "fs");
    }

    #[test]
    fn test_register_sequential_ids() {
        let r1 = handler_register_service(&register_payload("a"));
        let r2 = handler_register_service(&register_payload("b"));
        let r3 = handler_register_service(&register_payload("c"));
        let id1 = u32::from_le_bytes(r1.data[..4].try_into().unwrap());
        let id2 = u32::from_le_bytes(r2.data[..4].try_into().unwrap());
        let id3 = u32::from_le_bytes(r3.data[..4].try_into().unwrap());
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[test]
    fn test_get_handle_invalid_id_returns_error() {
        let resp = handler_get_service_handle(&get_handle_payload(9999));
        assert_eq!(resp.result_code, crate::kernel::handle_table::result::INVALID_HANDLE);
    }

    #[test]
    fn test_get_handle_zero_id_returns_error() {
        let resp = handler_get_service_handle(&get_handle_payload(0));
        assert_eq!(resp.result_code, crate::kernel::handle_table::result::INVALID_HANDLE);
    }

    #[test]
    fn test_register_empty_name() {
        let resp = handler_register_service(&register_payload(""));
        assert_eq!(resp.result_code, 0);
    }

    #[test]
    fn test_register_truncated_name_len() {
        // Only 2 bytes — not enough for the 4-byte name_len field
        let resp = handler_register_service(&[0x03, 0x00]);
        assert_eq!(resp.result_code, crate::kernel::handle_table::result::INVALID_HANDLE);
    }

    #[test]
    fn test_register_name_len_overflow() {
        // Claim 100 bytes but only provide 2
        let mut payload = vec![100u8, 0, 0, 0]; // name_len=100
        payload.push(b'a');
        payload.push(b'b');
        let resp = handler_register_service(&payload);
        assert_eq!(resp.result_code, crate::kernel::handle_table::result::INVALID_HANDLE);
    }

    #[test]
    fn test_get_handle_truncated_input() {
        let resp = handler_get_service_handle(&[0x01]);
        assert_eq!(resp.result_code, crate::kernel::handle_table::result::INVALID_HANDLE);
    }

    /// Build a minimal HIPC request buffer with just raw data (no descriptors).
    fn build_hipc_request(raw_data: &[u8]) -> Vec<u8> {
        // raw_count is the number of u32 words covered by the raw section.
        // Must be an exact match for the parser; pad payload to word boundary.
        let pad_len = (4 - (raw_data.len() % 4)) % 4;
        let total_raw_len = raw_data.len() + pad_len;
        let raw_words = (total_raw_len / 4) as u32;
        let hdr0 = 0u32;
        let hdr1 = raw_words & 0x3FF;

        let mut buf = Vec::with_capacity(8 + total_raw_len);
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());
        buf.extend_from_slice(raw_data);
        buf.resize(8 + total_raw_len, 0u8); // zero-pad to word boundary
        buf
    }

    fn build_hipc_request_with_method(method_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut raw = Vec::with_capacity(4 + payload.len());
        raw.extend_from_slice(&method_id.to_le_bytes());
        raw.extend_from_slice(payload);
        build_hipc_request(&raw)
    }

    #[test]
    fn test_dispatch_message_through_router() {
        // Reset the handle table
        SM_HANDLE_TABLE.with(|ht| {
            *ht.borrow_mut() = HandleTable::new();
        });

        let mut router = HipcRouter::new();
        router.register("sm", 0, handler_register_service);
        router.register("sm", 1, handler_get_service_handle);

        let buf = build_hipc_request_with_method(0, &register_payload("hid"));
        let resp = router.dispatch_message(&buf, "sm").unwrap();
        assert_eq!(resp.result_code, 0);
        assert_eq!(resp.data.len(), 4);
        let id = u32::from_le_bytes(resp.data[..4].try_into().unwrap());
        assert!(id > 0);

        // Round-trip: look up by external handle ID
        let g_resp = router.dispatch("sm", 1, &get_handle_payload(id)).unwrap();
        assert_eq!(g_resp.result_code, 0);
        let name_len = u32::from_le_bytes(g_resp.data[..4].try_into().unwrap()) as usize;
        let name = std::str::from_utf8(&g_resp.data[4..4 + name_len]).unwrap();
        assert_eq!(name, "hid");
    }

    #[test]
    fn test_dispatch_message_parse_failure_returns_malformed() {
        let router = HipcRouter::new();
        let result = router.dispatch_message(b"not a valid hipc message", "sm");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::nn::hipc::DispatchError::MalformedMessage
        ));
    }

    #[test]
    fn test_dispatch_message_service_not_found() {
        let mut router = HipcRouter::new();
        router.register("sm", 0, handler_register_service);

        let buf = build_hipc_request_with_method(0, &register_payload("ns"));
        let result = router.dispatch_message(&buf, "nonexistent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::nn::hipc::DispatchError::ServiceNotFound
        ));
    }

    #[test]
    fn test_dispatch_message_not_implemented() {
        let mut router = HipcRouter::new();
        router.register("sm", 0, handler_register_service);

        let buf = build_hipc_request_with_method(99, &get_handle_payload(1));
        let result = router.dispatch_message(&buf, "sm");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::nn::hipc::DispatchError::NotImplemented
        ));
    }
}

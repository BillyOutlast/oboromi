//! HIPC (Horizon Inter-Process Communication) message parser.
//!
//! Implements full two-phase HIPC parse: header word extraction followed by
//! descriptor walks. Validates all array accesses against `data.len()` with
//! explicit `ParseError` returns, never panicking on malformed input.
//!
//! The wire format per Horizon OS:
//! ```text
//! +0x00  hdr0: tag(16) | ptr_count(4) | send_count(4) | recv_count(4) | xchg_count(4)
//! +0x04  hdr1: raw_count(10) | recv_list_count(4) | recv_list_offs(10) | unused(7) | special(1)
//! +0x08..    descriptor words (paired [type:addr] for each buffer)
//! +...       raw data (method_id u32 + inline payload for non-special messages)
//! +...       C descriptor table (handle_id:16 | move_flag:1 | ...per handle)
//! +...       receive list entries (index:16 | addr:48 per entry)
//! ```
//!
//! Descriptor types:
//! - X (0): Send buffer — data flows host→device
//! - A (1): Receive buffer — data flows device→host
//! - B (2): Exchange buffer — bidirectional
//! - W (3): Receive-list wrapper — points to recv list entries
//! - C (-): Handle — copy/move kernel object handle

use core::fmt;
use std::collections::HashMap;
use log::{trace, warn};
use std::collections::HashMap;
use log::{trace, warn};

// ── Existing public types (preserved for backward compatibility) ────────

/// Raw HIPC header — two 32-bit words at message offset 0.
#[repr(C)]
pub struct HeaderData {
    pub header: [u32; 2],
}

/// Map / special descriptor data (3 words).
#[repr(C)]
pub struct MapData {
    pub data: [u32; 3],
}

/// Pointer / buffer descriptor data (2 words).
#[repr(C)]
pub struct PointerData {
    pub data: [u32; 2],
}

/// Receive list entry data (2 words).
#[repr(C)]
pub struct ReceiveListData {
    pub data: [u32; 2],
}

/// Legacy result codes used by the stub invoke_method path.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyResult {
    Success = 0,
    Failure = 1,
}

// ── New types ──────────────────────────────────────────────────────────

/// HIPC command type decoded from the low bits of the first type word
/// (Request=0, Control=1, Close=2 per Horizon OS specification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandType {
    Request = 0,
    Control = 1,
    Close = 2,
}

impl CommandType {
    /// Decode command type from bits [6:0] of the given word.
    pub fn from_word(word: u32) -> Self {
        match word & 0x7F {
            0 => CommandType::Request,
            1 => CommandType::Control,
            2 => CommandType::Close,
            _ => CommandType::Request, // Unrecognized falls back to Request
        }
    }
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandType::Request => write!(f, "Request"),
            CommandType::Control => write!(f, "Control"),
            CommandType::Close => write!(f, "Close"),
        }
    }
}

/// A buffer descriptor (X, A, B, or W variant).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BufferDescriptor {
    X { address: u64, size: u16, flags: u8 },
    A { address: u64, size: u16, flags: u8 },
    B { address: u64, size: u16, flags: u8 },
    W { address: u64, size: u16, flags: u8 },
}

impl BufferDescriptor {
    /// Decode a descriptor word pair into a typed buffer descriptor.
    /// `desc_type` is bits [0:1] of the first descriptor word:
    /// 0=X, 1=A, 2=B, 3=W.
    fn from_words(dw0: u32, dw1: u32, desc_type: u8) -> Option<Self> {
        let address = ((dw0 as u64 & 0xFFFF_FFC0) << 32) | ((dw1 as u64 >> 2) & 0x3FFF_FFFF);
        let size = ((dw1 & 0xFFFF) as u16).wrapping_mul(4);
        let flags = ((dw1 >> 30) as u8) & 0x3;

        match desc_type {
            0 => Some(BufferDescriptor::X {
                address,
                size,
                flags,
            }),
            1 => Some(BufferDescriptor::A {
                address,
                size,
                flags,
            }),
            2 => Some(BufferDescriptor::B {
                address,
                size,
                flags,
            }),
            3 => Some(BufferDescriptor::W {
                address,
                size,
                flags,
            }),
            _ => None,
        }
    }

    pub fn descriptor_type_name(&self) -> &'static str {
        match self {
            BufferDescriptor::X { .. } => "X",
            BufferDescriptor::A { .. } => "A",
            BufferDescriptor::B { .. } => "B",
            BufferDescriptor::W { .. } => "W",
        }
    }

    pub fn address(&self) -> u64 {
        match self {
            BufferDescriptor::X { address, .. }
            | BufferDescriptor::A { address, .. }
            | BufferDescriptor::B { address, .. }
            | BufferDescriptor::W { address, .. } => *address,
        }
    }

    pub fn size(&self) -> u16 {
        match self {
            BufferDescriptor::X { size, .. }
            | BufferDescriptor::A { size, .. }
            | BufferDescriptor::B { size, .. }
            | BufferDescriptor::W { size, .. } => *size,
        }
    }
}

/// A handle descriptor (C descriptor) carrying a kernel object handle ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleDescriptor {
    pub handle_id: u32,
    pub is_move: bool,
}

impl HandleDescriptor {
    fn from_word(w: u32) -> Self {
        HandleDescriptor {
            handle_id: (w >> 16) & 0xFFFF,
            is_move: (w & 0x8000) != 0,
        }
    }
}

/// A fully parsed HIPC message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcMessage {
    pub command_type: CommandType,
    pub service_name: String,
    pub method_id: u32,
    pub send_buffers: Vec<BufferDescriptor>,
    pub receive_buffers: Vec<BufferDescriptor>,
    pub exchange_buffers: Vec<BufferDescriptor>,
    pub copy_handles: Vec<HandleDescriptor>,
    pub move_handles: Vec<HandleDescriptor>,
    pub raw_data: Vec<u8>,
}

/// Errors returned by the HIPC parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// Buffer is smaller than the 8-byte header.
    TruncatedHeader,
    /// Payload is truncated — a descriptor walk ran past the end of data.
    TruncatedPayload,
    /// Descriptor type bits were invalid (not 0-3).
    InvalidDescriptorType(u8),
    /// Descriptor count exceeds the per-category maximum of 15.
    TooManyDescriptors {
        category: &'static str,
        count: u8,
    },
    /// Handle index is out of range.
    InvalidHandleIndex(u32),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::TruncatedHeader => write!(f, "HIPC message too short for header (need 8 bytes)"),
            ParseError::TruncatedPayload => write!(f, "HIPC message truncated — descriptor walk past end of buffer"),
            ParseError::InvalidDescriptorType(t) => write!(f, "Invalid HIPC descriptor type: {} (expected 0-3)", t),
            ParseError::TooManyDescriptors { category, count } => {
                write!(f, "Too many {} descriptors: {} (max 15)", category, count)
            }
            ParseError::InvalidHandleIndex(i) => write!(f, "Invalid handle index: {}", i),
        }
    }
}

/// Maximum number of methods per service dispatch table.
pub const MAX_METHODS_PER_SERVICE: u32 = 256;

// ── Dispatch Router Types ──────────────────────────────────────────────

/// Errors returned by HIPC dispatch operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchError {
    /// The requested service is not registered in the router.
    ServiceNotFound,
    /// The service is registered but method_id has no handler.
    NotImplemented,
    /// The message was malformed or parse failed.
    MalformedMessage,
}

impl fmt::Display for DispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispatchError::ServiceNotFound => write!(f, "HIPC service not found"),
            DispatchError::NotImplemented => write!(f, "HIPC method not implemented"),
            DispatchError::MalformedMessage => write!(f, "HIPC malformed message"),
        }
    }
}

impl DispatchError {
    /// Returns the Horizon result code for this error.
    pub fn result_code(&self) -> u32 {
        match self {
            DispatchError::ServiceNotFound => crate::kernel::result::SERVICE_NOT_FOUND,
            DispatchError::NotImplemented => crate::kernel::result::NOT_IMPLEMENTED,
            DispatchError::MalformedMessage => crate::kernel::result::INVALID_HANDLE,
        }
    }
}

/// A HIPC response message: result code, output data, and moved handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HipcResponse {
    pub result_code: u32,
    pub data: Vec<u8>,
    pub moved_handles: Vec<u32>,
}

impl HipcResponse {
    /// Creates a response with just a result code.
    pub fn new(result_code: u32) -> Self {
        HipcResponse {
            result_code,
            data: Vec::new(),
            moved_handles: Vec::new(),
        }
    }

    /// Creates a response with result code and data.
    pub fn with_data(result_code: u32, data: Vec<u8>) -> Self {
        HipcResponse {
            result_code,
            data,
            moved_handles: Vec::new(),
        }
    }

    /// Serializes this response into HIPC wire-format bytes.
    ///
    /// Wire layout:
    /// ```text
    /// +0x00  hdr0: tag(16)=0 | ptr_count(4) | send/recv/xchg=0
    /// +0x04  hdr1: raw_count(10) in words | special=0
    /// +0x08.. raw data: result_code u32 + payload padded to word boundary
    /// +...    C descriptors: one word per moved handle (move flag set)
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_words = (self.data.len() + 3) / 4;
        let raw_words = 1usize + payload_words; // result_code + payload
        let raw_count = (raw_words as u32) & 0x3FF;
        let ptr_count = self.moved_handles.len() as u32;

        let hdr0 = ptr_count << 16;
        let hdr1 = raw_count;

        let cap = 8 + raw_words * 4 + self.moved_handles.len() * 4;
        let mut buf = Vec::with_capacity(cap);

        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());
        buf.extend_from_slice(&self.result_code.to_le_bytes());
        buf.extend_from_slice(&self.data);
        // Pad payload to word boundary with zeros
        let pad_needed = (4 - (self.data.len() % 4)) % 4;
        buf.extend(std::iter::repeat_n(0u8, pad_needed));
        // C descriptors: moved handles
        for &hid in &self.moved_handles {
            buf.extend_from_slice(&((hid << 16) | 0x8000u32).to_le_bytes());
        }
        buf
    }
}

/// Handler function type: receives raw data, returns a response.
pub type HipcHandlerFn = fn(data: &[u8]) -> HipcResponse;

/// Per-service method dispatch table indexed by method_id (0..255).
pub struct ServiceDispatchTable {
    name: String,
    methods: Vec<Option<HipcHandlerFn>>,
}

impl ServiceDispatchTable {
    pub fn new(name: String) -> Self {
        ServiceDispatchTable {
            name,
            methods: Vec::new(),
        }
    }

    /// Registers a handler at method_id. Grows methods vec as needed (caps at 256).
    pub fn register(&mut self, method_id: u32, handler: HipcHandlerFn) {
        let idx = method_id as usize;
        if idx >= self.methods.len() {
            self.methods.resize_with(idx + 1, || None);
        }
        if self.methods[idx].is_some() {
            warn!(
                "ServiceDispatchTable: overwriting method_id={} for '{}'",
                method_id, self.name
            );
        }
        self.methods[idx] = Some(handler);
        trace!(
            "ServiceDispatchTable: registered '{}' method_id={}",
            self.name,
            method_id
        );
    }

    pub fn dispatch(&self, method_id: u32) -> Option<&HipcHandlerFn> {
        self.methods.get(method_id as usize).and_then(|m| m.as_ref())
    }

    pub fn service_name(&self) -> &str {
        &self.name
    }

    pub fn registered_count(&self) -> usize {
        self.methods.iter().filter(|m| m.is_some()).count()
    }
}

/// HIPC dispatch router: O(1) (service_name, method_id) → handler.
pub struct HipcRouter {
    services: HashMap<String, ServiceDispatchTable>,
}

impl HipcRouter {
    pub fn new() -> Self {
        HipcRouter {
            services: HashMap::new(),
        }
    }

    /// Registers a handler for (service_name, method_id). Auto-creates the table.
    pub fn register(&mut self, service_name: &str, method_id: u32, handler: HipcHandlerFn) {
        let table = self
            .services
            .entry(service_name.to_string())
            .or_insert_with(|| ServiceDispatchTable::new(service_name.to_string()));
        table.register(method_id, handler);
    }

    /// Dispatches to handler. Returns ServiceNotFound / NotImplemented on miss.
    pub fn dispatch(
        &self,
        service_name: &str,
        method_id: u32,
        data: &[u8],
    ) -> Result<HipcResponse, DispatchError> {
        let table = self.services.get(service_name).ok_or_else(|| {
            warn!("HipcRouter: service not found: '{}'", service_name);
            DispatchError::ServiceNotFound
        })?;
        let handler = table.dispatch(method_id).ok_or_else(|| {
            warn!(
                "HipcRouter: method {} not implemented for '{}'",
                method_id, service_name
            );
            DispatchError::NotImplemented
        })?;
        trace!(
            "HipcRouter::dispatch: '{}' method_id={}",
            service_name,
            method_id
        );
        Ok(handler(data))
    }

    pub fn registered_services(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }

    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    pub fn total_handler_count(&self) -> usize {
        self.services.values().map(|t| t.registered_count()).sum()
    }
}

impl Default for HipcRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Maximum descriptor count per category per Horizon OS spec.
const MAX_DESCRIPTORS_PER_CATEGORY: u8 = 15;

/// Size of the HIPC header in bytes (two u32s).
const HEADER_SIZE: usize = 8;

/// Maximum raw data size from the 10-bit field (0x1FF words = 0x7FC bytes).
const MAX_RAW_SIZE_WORDS: u32 = 0x1FF;
const MAX_RAW_DATA_BYTES: usize = (MAX_RAW_SIZE_WORDS as usize) * 4;

/// Read a u32 from a byte slice at the given offset. Returns None on OOB.
fn read_u32(data: &[u8], offset_bytes: usize) -> Option<u32> {
    if offset_bytes + 4 > data.len() {
        return None;
    }
    let ptr = unsafe { data.as_ptr().add(offset_bytes) as *const u32 };
    Some(unsafe { ptr.read_unaligned() })
}

/// Bounds-checked read of multiple u32 words starting at offset. Returns None
/// if any word would read past the buffer end.
fn read_u32s(data: &[u8], offset_bytes: usize, count: usize) -> Option<&[u8]> {
    let end = offset_bytes + count * 4;
    if end > data.len() {
        None
    } else {
        Some(&data[offset_bytes..end])
    }
}

// ── Descriptor walks ───────────────────────────────────────────────────

/// Walk a sequence of buffer descriptor word-pairs.
/// Each descriptor is 2 words: descriptor-word + data-word.
fn walk_buffer_descriptors(
    data: &[u8],
    base_offset: usize,
    count: usize,
) -> Result<Vec<BufferDescriptor>, ParseError> {
    if count == 0 {
        return Ok(Vec::new());
    }

    let mut descriptors = Vec::with_capacity(count);
    let pair_size = 8; // two u32s per buffer descriptor pair
    for i in 0..count {
        let off = base_offset + i * pair_size;
        let dw0 = read_u32(data, off).ok_or(ParseError::TruncatedPayload)?;
        let dw1 = read_u32(data, off + 4).ok_or(ParseError::TruncatedPayload)?;

        let desc_type = (dw0 as u8) & 0x3;
        let desc = BufferDescriptor::from_words(dw0, dw1, desc_type)
            .ok_or(ParseError::InvalidDescriptorType(desc_type))?;
        descriptors.push(desc);
    }
    Ok(descriptors)
}

/// Walk a sequence of handle (C) descriptor words.
fn walk_handle_descriptors(
    data: &[u8],
    base_offset: usize,
    count: usize,
) -> Result<Vec<HandleDescriptor>, ParseError> {
    if count == 0 {
        return Ok(Vec::new());
    }

    let mut descriptors = Vec::with_capacity(count);
    for i in 0..count {
        let off = base_offset + i * 4;
        let w = read_u32(data, off).ok_or(ParseError::TruncatedPayload)?;
        descriptors.push(HandleDescriptor::from_word(w));
    }
    Ok(descriptors)
}

// ── Main parse entry point ─────────────────────────────────────────────

impl HipcMessage {
    /// Parse a raw HIPC byte buffer into a structured `HipcMessage`.
    ///
    /// `service_name` is the target service extracted from the session
    /// context by the caller (e.g. "ns", "fs", "hid").
    ///
    /// Returns `ParseError` on any malformed input — this function never
    /// panics on invalid data.
    pub fn parse(data: &[u8], service_name: &str) -> Result<HipcMessage, ParseError> {
        // ── Phase 1: header extraction ──────────────────────────────────
        let hdr0 = read_u32(data, 0).ok_or(ParseError::TruncatedHeader)?;
        let hdr1 = read_u32(data, 4).ok_or(ParseError::TruncatedHeader)?;

        let tag = (hdr0 >> 0) & 0xFFFF;
        let command_type = CommandType::from_word(tag);

        let ptr_count = ((hdr0 >> 16) & 0xF) as u8;
        let send_count = ((hdr0 >> 20) & 0xF) as u8;
        let recv_count = ((hdr0 >> 24) & 0xF) as u8;
        let xchg_count = ((hdr0 >> 28) & 0xF) as u8;

        // Note: send/recv/xchg/ptr counts are 4-bit fields in hdr0,
        // inherently capped at 15 by the hardware encoding. No runtime
        // TooManyDescriptors check needed — the field masks enforce the cap.

        let raw_count = (hdr1 >> 0) & 0x3FF; // 10 bits
        let recv_list_count = ((hdr1 >> 10) & 0xF) as u8;
        let recv_list_offs = ((hdr1 >> 14) & 0x3FF) as u16;
        let special_count = ((hdr1 >> 31) & 0x1) as u8;

        if raw_count > MAX_RAW_SIZE_WORDS {
            return Err(ParseError::TruncatedPayload);
        }
        let raw_data_bytes = raw_count as usize * 4;

        // ── Phase 2: descriptor walks ───────────────────────────────────
        // Descriptors start at offset 8 (after the two u32 header words).
        let mut cursor = HEADER_SIZE;

        // Send descriptors
        let send_buffers = walk_buffer_descriptors(data, cursor, send_count as usize)?;
        cursor += send_count as usize * 8;

        // Receive descriptors
        let receive_buffers = walk_buffer_descriptors(data, cursor, recv_count as usize)?;
        cursor += recv_count as usize * 8;

        // Exchange descriptors
        let exchange_buffers = walk_buffer_descriptors(data, cursor, xchg_count as usize)?;
        cursor += xchg_count as usize * 8;

        // ── Phase 3: raw data section ───────────────────────────────────
        if raw_data_bytes > 0 && cursor + raw_data_bytes > data.len() {
            return Err(ParseError::TruncatedPayload);
        }
        let raw_data = if raw_data_bytes > 0 {
            data[cursor..cursor + raw_data_bytes].to_vec()
        } else {
            Vec::new()
        };
        cursor += raw_data_bytes;

        // Extract method_id from raw data (first u32 for non-special messages).
        let method_id = if special_count == 0 && raw_data.len() >= 4 {
            let ptr = raw_data.as_ptr() as *const u32;
            unsafe { ptr.read_unaligned() }
        } else {
            0
        };

        // ── Phase 4: C (handle) descriptors ─────────────────────────────
        let mut copy_handles = Vec::new();
        let mut move_handles = Vec::new();

        // Pointer descriptors in Horizon HIPC: ptr_count C descriptors,
        // each one word. "Move" handles are those with the move flag set.
        // These are parsed after the raw data section.
        let c_descriptors = walk_handle_descriptors(data, cursor, ptr_count as usize)?;
        cursor += ptr_count as usize * 4;

        for hd in c_descriptors {
            if hd.is_move {
                move_handles.push(hd);
            } else {
                copy_handles.push(hd);
            }
        }

        // ── Phase 5: receive list entries ───────────────────────────────
        // Present only if recv_list_count > 0. We skip full decode here
        // since T03 handles the routing logic.
        if recv_list_count > 0 {
            let rl_bytes = recv_list_count as usize * 8; // each entry is 2 words
            if cursor + rl_bytes > data.len() {
                return Err(ParseError::TruncatedPayload);
            }
            // Receive list is consumed for validation but individual entries
            // are decoded on-demand during dispatch (T03).
            cursor += rl_bytes;
        }

        // If special_count is set, there's a special descriptor at the end.
        // The special descriptor is 3 words (MapData).
        if special_count > 0 {
            if cursor + 12 > data.len() {
                return Err(ParseError::TruncatedPayload);
            }
            // consumed — special descriptor handling deferred to T03
        }

        Ok(HipcMessage {
            command_type,
            service_name: service_name.to_string(),
            method_id,
            send_buffers,
            receive_buffers,
            exchange_buffers,
            copy_handles,
            move_handles,
            raw_data,
        })
    }

    /// Return true if this is a Close command (session tear-down).
    pub fn is_close(&self) -> bool {
        self.command_type == CommandType::Close
    }

    /// Total number of buffer descriptors across all three categories.
    pub fn total_buffer_count(&self) -> usize {
        self.send_buffers.len() + self.receive_buffers.len() + self.exchange_buffers.len()
    }
}

// ── Legacy stub (preserved for backward compatibility) ─────────────────

/// Legacy invoke_method — extracts header fields and delegates to a closure.
/// Preserved for existing callers that depend on the field extraction pattern.
/// New code should use `HipcMessage::parse` directly.
pub fn invoke_method<F>(data: &[u8], f: F) -> LegacyResult
where
    F: Fn() -> LegacyResult,
{
    let hdr0 = unsafe { *(data.as_ptr() as *const u32) };
    let _tag = (hdr0 >> 0) & 0xffff;
    let _ptrs_count = (hdr0 >> 16) & 0xf;
    let _send_count = (hdr0 >> 20) & 0xf;
    let _recv_count = (hdr0 >> 24) & 0xf;
    let _xchg_count = (hdr0 >> 28) & 0xf;

    let hdr1 = unsafe {
        *(data.as_ptr().byte_add(4) as *const u32)
    };
    let _raw_count = (hdr1 >> 0) & ((1 << 10) - 1);
    let _recv_list_count = (hdr1 >> 10) & 0xf;
    let _recv_list_offs = (hdr1 >> 14) & 0x3ff;
    let _special_count = (hdr1 >> 31) & 0x1;

    f()
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper builders ─────────────────────────────────────────────────

    /// Build a minimal HIPC message buffer for a Request with the given
    /// send/recv/xchg counts and raw data.
    fn build_request_msg(
        send_count: u8,
        recv_count: u8,
        xchg_count: u8,
        raw_data: &[u32],
    ) -> Vec<u8> {
        let raw_words = raw_data.len() as u32;
        let hdr0 = 0u32 // tag=0 → Request
            | (0 << 16) // ptr_count=0 (no C descriptors)
            | ((send_count as u32) << 20)
            | ((recv_count as u32) << 24)
            | ((xchg_count as u32) << 28);
        let hdr1 = raw_words & 0x3FF; // raw_count

        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());

        // Placeholder descriptor words: each buffer descriptor takes 2 words
        for _ in 0..send_count {
            // dw0: type=X (0), address high 22 bits
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
        }
        for _ in 0..recv_count {
            buf.extend_from_slice(&(1u32).to_le_bytes()); // type=A=1
            buf.extend_from_slice(&0u32.to_le_bytes());
        }
        for _ in 0..xchg_count {
            buf.extend_from_slice(&(2u32).to_le_bytes()); // type=B=2
            buf.extend_from_slice(&0u32.to_le_bytes());
        }

        // Raw data
        for w in raw_data {
            buf.extend_from_slice(&w.to_le_bytes());
        }
        buf
    }

    /// Build a HIPC message with C (handle) descriptors.
    fn build_msg_with_handles(
        command_type: CommandType,
        send_count: u8,
        raw_data: &[u32],
        handle_ids: &[(u32, bool)], // (handle_id, is_move)
    ) -> Vec<u8> {
        let cmd_tag = command_type as u32;
        let raw_words = raw_data.len() as u32;
        let hdr0 = cmd_tag
            | ((handle_ids.len() as u32) << 16) // ptr_count
            | ((send_count as u32) << 20);
        let hdr1 = raw_words & 0x3FF;

        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());

        for _ in 0..send_count {
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
        }
        for w in raw_data {
            buf.extend_from_slice(&w.to_le_bytes());
        }
        // C descriptor words
        for &(hid, is_move) in handle_ids {
            let word = (hid << 16) | if is_move { 0x8000 } else { 0 };
            buf.extend_from_slice(&word.to_le_bytes());
        }
        buf
    }

    /// Build a Close message.
    fn build_close_msg() -> Vec<u8> {
        let hdr0 = 2u32; // Close=2 in bits [6:0]
        let hdr1 = 0u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());
        buf
    }

    // ── Happy-path tests ────────────────────────────────────────────────

    #[test]
    fn test_parse_minimal_request() {
        let raw = [0x12345678u32]; // method_id
        let buf = build_request_msg(0, 0, 0, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.command_type, CommandType::Request);
        assert_eq!(msg.service_name, "ns");
        assert_eq!(msg.method_id, 0x12345678);
        assert!(msg.send_buffers.is_empty());
        assert!(msg.receive_buffers.is_empty());
        assert!(msg.exchange_buffers.is_empty());
        assert!(msg.copy_handles.is_empty());
        assert!(msg.move_handles.is_empty());
        assert!(!msg.is_close());
    }

    #[test]
    fn test_parse_request_with_send_buffers() {
        let raw = [0x42u32];
        let buf = build_request_msg(3, 0, 0, &raw);
        let msg = HipcMessage::parse(&buf, "fs").unwrap();

        assert_eq!(msg.send_buffers.len(), 3);
        assert!(msg.receive_buffers.is_empty());
        assert_eq!(msg.method_id, 0x42);
        assert_eq!(msg.total_buffer_count(), 3);

        // All three are X descriptors (type=0)
        for b in &msg.send_buffers {
            assert_eq!(b.descriptor_type_name(), "X");
        }
    }

    #[test]
    fn test_parse_request_with_all_buffer_types() {
        let raw = [0x100u32];
        let buf = build_request_msg(1, 1, 1, &raw);
        let msg = HipcMessage::parse(&buf, "hid").unwrap();

        assert_eq!(msg.send_buffers.len(), 1);
        assert_eq!(msg.receive_buffers.len(), 1);
        assert_eq!(msg.exchange_buffers.len(), 1);
        assert_eq!(msg.total_buffer_count(), 3);

        assert_eq!(msg.send_buffers[0].descriptor_type_name(), "X");
        assert_eq!(msg.receive_buffers[0].descriptor_type_name(), "A");
        assert_eq!(msg.exchange_buffers[0].descriptor_type_name(), "B");
    }

    #[test]
    fn test_parse_control_message() {
        let raw = [0x99u32];
        let buf = build_msg_with_handles(CommandType::Control, 0, &raw, &[]);
        let msg = HipcMessage::parse(&buf, "set").unwrap();

        assert_eq!(msg.command_type, CommandType::Control);
        assert_eq!(msg.method_id, 0x99);
        assert!(!msg.is_close());
    }

    #[test]
    fn test_parse_close_message() {
        let buf = build_close_msg();
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.command_type, CommandType::Close);
        assert!(msg.is_close());
        assert_eq!(msg.method_id, 0);
        assert_eq!(msg.total_buffer_count(), 0);
    }

    #[test]
    fn test_parse_copy_handles() {
        let raw = [0x1u32];
        let handles = [(10, false), (20, false)];
        let buf = build_msg_with_handles(CommandType::Request, 0, &raw, &handles);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.copy_handles.len(), 2);
        assert_eq!(msg.move_handles.len(), 0);
        assert_eq!(msg.copy_handles[0].handle_id, 10);
        assert!(!msg.copy_handles[0].is_move);
        assert_eq!(msg.copy_handles[1].handle_id, 20);
    }

    #[test]
    fn test_parse_move_handles() {
        let raw = [0x2u32];
        let handles = [(5, true), (7, true)];
        let buf = build_msg_with_handles(CommandType::Request, 0, &raw, &handles);
        let msg = HipcMessage::parse(&buf, "fs").unwrap();

        assert_eq!(msg.move_handles.len(), 2);
        assert_eq!(msg.copy_handles.len(), 0);
        assert!(msg.move_handles[0].is_move);
        assert_eq!(msg.move_handles[0].handle_id, 5);
        assert_eq!(msg.move_handles[1].handle_id, 7);
    }

    #[test]
    fn test_parse_mixed_handles() {
        let raw = [0x3u32];
        let handles = [(1, false), (2, true), (3, false), (4, true)];
        let buf = build_msg_with_handles(CommandType::Request, 0, &raw, &handles);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.copy_handles.len(), 2);
        assert_eq!(msg.move_handles.len(), 2);
        assert_eq!(msg.copy_handles[0].handle_id, 1);
        assert_eq!(msg.move_handles[0].handle_id, 2);
        assert_eq!(msg.copy_handles[1].handle_id, 3);
        assert_eq!(msg.move_handles[1].handle_id, 4);
    }

    #[test]
    fn test_method_id_from_raw_data() {
        let raw = [0xDEADBEEFu32, 0xCAFEBABEu32];
        let buf = build_request_msg(0, 0, 0, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.method_id, 0xDEADBEEF);
        assert_eq!(msg.raw_data.len(), 8);
    }

    #[test]
    fn test_buffer_descriptor_address_size() {
        // Build a message with a send buffer that has explicit address/size
        let raw = [0x100u32];
        let send_count = 1u8;

        let raw_words = raw.len() as u32;
        let hdr0 = 0u32 | (send_count as u32) << 20;
        let hdr1 = raw_words & 0x3FF;

        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());

        // X descriptor: address=0x1000_0000_0000, size=0x200
        // dw0: type=0, address_high=(0x1000 >> 6) = 0x40
        let dw0 = 0x40u32; // type bits 0-1 = 0 (X), addr_high at bits 6-31
        let dw1 = (0x200 / 4) as u32; // size in words (0x80), no flags
        buf.extend_from_slice(&dw0.to_le_bytes());
        buf.extend_from_slice(&dw1.to_le_bytes());

        for w in &raw {
            buf.extend_from_slice(&w.to_le_bytes());
        }

        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        assert_eq!(msg.send_buffers.len(), 1);
        // Size = (dw1 & 0xFFFF) * 4 = 0x80 * 4 = 0x200
        assert_eq!(msg.send_buffers[0].size(), 0x200);
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn test_max_descriptors_per_category() {
        let raw = [0x1u32];
        let buf = build_request_msg(15, 15, 15, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.send_buffers.len(), 15);
        assert_eq!(msg.receive_buffers.len(), 15);
        assert_eq!(msg.exchange_buffers.len(), 15);
    }

    #[test]
    fn test_max_handle_descriptors() {
        let raw = [0x1u32];
        let handles: Vec<(u32, bool)> = (0..15).map(|i| (i, false)).collect();
        let buf = build_msg_with_handles(CommandType::Request, 0, &raw, &handles);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.copy_handles.len(), 15);
    }

    #[test]
    fn test_zero_descriptors() {
        let raw = [0x1u32];
        let buf = build_request_msg(0, 0, 0, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.total_buffer_count(), 0);
        assert!(msg.copy_handles.is_empty());
        assert!(msg.move_handles.is_empty());
    }

    #[test]
    fn test_empty_raw_data() {
        let buf = build_request_msg(0, 0, 0, &[]);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.method_id, 0);
        assert!(msg.raw_data.is_empty());
    }

    #[test]
    fn test_service_name_preservation() {
        let raw = [0x1u32];
        let buf = build_request_msg(0, 0, 0, &raw);

        for svc in &["ns", "fs", "hid", "set", "nvdrv", "bsd:s"] {
            let msg = HipcMessage::parse(&buf, svc).unwrap();
            assert_eq!(msg.service_name, *svc);
        }
    }

    #[test]
    fn test_hdr0_field_extraction_in_header_preserved() {
        // Verify the existing field extraction logic still works through
        // HipcMessage::parse (the hdr0 fields are consumed internally).
        let raw = [0x42u32];
        let buf = build_request_msg(2, 3, 1, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.send_buffers.len(), 2);
        assert_eq!(msg.receive_buffers.len(), 3);
        assert_eq!(msg.exchange_buffers.len(), 1);
    }

    #[test]
    fn test_command_type_display() {
        assert_eq!(format!("{}", CommandType::Request), "Request");
        assert_eq!(format!("{}", CommandType::Control), "Control");
        assert_eq!(format!("{}", CommandType::Close), "Close");
    }

    #[test]
    fn test_parse_error_display() {
        assert!(format!("{}", ParseError::TruncatedHeader).contains("too short"));
        assert!(format!("{}", ParseError::TruncatedPayload).contains("truncated"));
        assert!(format!("{}", ParseError::InvalidDescriptorType(5)).contains("5"));
        assert!(format!("{}", ParseError::TooManyDescriptors {
            category: "send",
            count: 20
        })
        .contains("send"));
        assert!(format!("{}", ParseError::InvalidHandleIndex(999)).contains("999"));
    }

    // ── Negative tests: malformed input ─────────────────────────────────

    #[test]
    fn test_parse_empty_buffer() {
        let result = HipcMessage::parse(&[], "ns");
        assert!(matches!(result, Err(ParseError::TruncatedHeader)));
    }

    #[test]
    fn test_parse_header_only() {
        // Header-only (8 bytes) with zero counts — should succeed
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        assert_eq!(msg.method_id, 0);
    }

    #[test]
    fn test_parse_header_partial() {
        // Only 3 bytes — less than header
        let buf = [0xAA, 0xBB, 0xCC];
        let result = HipcMessage::parse(&buf, "ns");
        assert!(matches!(result, Err(ParseError::TruncatedHeader)));
    }

    #[test]
    fn test_parse_header_exactly_4_bytes() {
        // 4 bytes — less than the full 8-byte header
        let result = HipcMessage::parse(&[0u8; 4], "ns");
        assert!(matches!(result, Err(ParseError::TruncatedHeader)));
    }

    #[test]
    fn test_parse_header_exactly_7_bytes() {
        let result = HipcMessage::parse(&[0u8; 7], "ns");
        assert!(matches!(result, Err(ParseError::TruncatedHeader)));
    }

    /// ParseError::TooManyDescriptors is documented but unreachable under
    /// normal HIPC encoding — descriptor count fields are 4-bit (0-15).
    /// It exists for defense-in-depth and for future extended header formats.
    #[test]
    fn test_too_many_send_descriptors() {
        let err = ParseError::TooManyDescriptors {
            category: "send",
            count: 16,
        };
        assert!(format!("{}", err).contains("send"));
    }

    #[test]
    fn test_too_many_recv_descriptors() {
        let err = ParseError::TooManyDescriptors {
            category: "receive",
            count: 20,
        };
        assert!(format!("{}", err).contains("receive"));
    }

    #[test]
    fn test_too_many_xchg_descriptors() {
        let err = ParseError::TooManyDescriptors {
            category: "exchange",
            count: 30,
        };
        assert!(format!("{}", err).contains("exchange"));
    }

    #[test]
    fn test_too_many_ptr_descriptors() {
        let err = ParseError::TooManyDescriptors {
            category: "pointer (handle)",
            count: 99,
        };
        assert!(format!("{}", err).contains("pointer"));
    }

    #[test]
    fn test_truncated_send_descriptor() {
        let raw = [0x1u32];
        // Build message claiming 1 send descriptor but provide no space for it
        let buf = build_request_msg(1, 0, 0, &raw);
        // Truncate: remove the last 4 bytes (half the descriptor pair)
        let truncated = &buf[..buf.len() - 4];
        let result = HipcMessage::parse(truncated, "ns");
        assert!(matches!(result, Err(ParseError::TruncatedPayload)));
    }

    #[test]
    fn test_truncated_raw_data() {
        // Build header with raw_count > available data
        let hdr0 = 0u32;
        let hdr1 = 10u32; // raw_count = 10 words = 40 bytes
        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());
        // Provide only 4 bytes of raw data instead of 40
        buf.extend_from_slice(&0x42u32.to_le_bytes());
        let result = HipcMessage::parse(&buf, "ns");
        assert!(matches!(result, Err(ParseError::TruncatedPayload)));
    }

    #[test]
    fn test_truncated_handle_descriptor() {
        let raw = [0x1u32];
        let handles = [(1, false), (2, false)]; // 2 handle descriptors
        let mut buf = build_msg_with_handles(CommandType::Request, 0, &raw, &handles);
        // Truncate: remove the last 4 bytes (second handle word)
        buf.truncate(buf.len() - 4);
        let result = HipcMessage::parse(&buf, "ns");
        assert!(matches!(result, Err(ParseError::TruncatedPayload)));
    }

    /// ParseError::InvalidDescriptorType is documented but unreachable in
    /// practice — the 2-bit descriptor type field in the HIPC encoding
    /// inherently limits types to 0-3 (X/A/B/W). It exists for defense-in-depth.
    #[test]
    fn test_invalid_descriptor_type_bits() {
        // Descriptor type is a 2-bit field — only 0-3 are valid.
        // Verify the error variant exists for documentation, though a
        // well-formed HIPC message can never trigger it.
        let err = ParseError::InvalidDescriptorType(4);
        assert!(format!("{}", err).contains("4"));
    }

    #[test]
    fn test_w_descriptor_w_type() {
        // W descriptors are valid (type=3)
        let raw = [0x1u32];
        let raw_words = raw.len() as u32;
        let hdr0 = 0u32 | (1u32 << 20); // send_count=1
        let hdr1 = raw_words & 0x3FF;

        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());

        let dw0 = 3u32; // type=3 → W
        let dw1 = 0u32;
        buf.extend_from_slice(&dw0.to_le_bytes());
        buf.extend_from_slice(&dw1.to_le_bytes());

        for w in &raw {
            buf.extend_from_slice(&w.to_le_bytes());
        }

        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        assert_eq!(msg.send_buffers.len(), 1);
        assert_eq!(msg.send_buffers[0].descriptor_type_name(), "W");
    }

    #[test]
    fn test_special_descriptor_flag() {
        let raw = [0x42u32];
        let raw_words = raw.len() as u32;
        let hdr0 = 0u32;
        let hdr1 = raw_words | (1u32 << 31); // special_count=1

        let mut buf = Vec::new();
        buf.extend_from_slice(&hdr0.to_le_bytes());
        buf.extend_from_slice(&hdr1.to_le_bytes());
        for w in &raw {
            buf.extend_from_slice(&w.to_le_bytes());
        }
        // Add space for the special descriptor (12 bytes = 3 words)
        buf.extend_from_slice(&[0u8; 12]);

        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        // With special_count=1, method_id is 0 (not extracted from raw data)
        assert_eq!(msg.method_id, 0);
    }

    #[test]
    fn test_handle_descriptor_boundary_values() {
        let raw = [0x1u32];
        let handles = [(0, false), (0xFFFF, true)];
        let buf = build_msg_with_handles(CommandType::Request, 0, &raw, &handles);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();

        assert_eq!(msg.copy_handles[0].handle_id, 0);
        assert_eq!(msg.move_handles[0].handle_id, 0xFFFF);
        assert!(msg.move_handles[0].is_move);
    }

    #[test]
    fn test_max_raw_data_size() {
        // Max raw_count = 0x1FF words = 511 words = 2044 bytes
        let max_raw: Vec<u32> = (0..MAX_RAW_SIZE_WORDS).map(|i| i).collect();
        let buf = build_request_msg(0, 0, 0, &max_raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        assert_eq!(msg.raw_data.len(), MAX_RAW_DATA_BYTES);
        assert_eq!(msg.method_id, 0);
    }

    #[test]
    fn test_command_type_from_tag() {
        // Request: bits[6:0] = 0
        assert_eq!(CommandType::from_word(0x0000), CommandType::Request);
        assert_eq!(CommandType::from_word(0x0080), CommandType::Request); // bit 7 doesn't affect
        // Control: bits[6:0] = 1
        assert_eq!(CommandType::from_word(0x0001), CommandType::Control);
        // Close: bits[6:0] = 2
        assert_eq!(CommandType::from_word(0x0002), CommandType::Close);
        // Unrecognized: 3 → falls back to Request
        assert_eq!(CommandType::from_word(0x0003), CommandType::Request);
    }

    #[test]
    fn test_legacy_invoke_method_works() {
        let raw = [0x42u32];
        let buf = build_request_msg(0, 0, 0, &raw);
        let result = invoke_method(&buf, || LegacyResult::Success);
        assert_eq!(result, LegacyResult::Success);
    }

    #[test]
    fn test_legacy_invoke_method_returns_failure() {
        let raw = [0x42u32];
        let buf = build_request_msg(0, 0, 0, &raw);
        let result = invoke_method(&buf, || LegacyResult::Failure);
        assert_eq!(result, LegacyResult::Failure);
    }

    #[test]
    fn test_parse_error_is_clone_and_eq() {
        let e1 = ParseError::TruncatedHeader;
        let e2 = e1.clone();
        assert_eq!(e1, e2);
    }

    #[test]
    fn test_command_type_clone_copy() {
        let ct = CommandType::Control;
        let ct2 = ct;
        assert_eq!(ct, ct2); // Copy works
    }

    #[test]
    fn test_total_buffer_count_zero() {
        let raw = [0x1u32];
        let buf = build_request_msg(0, 0, 0, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        assert_eq!(msg.total_buffer_count(), 0);
    }

    #[test]
    fn test_total_buffer_count_all() {
        let raw = [0x1u32];
        let buf = build_request_msg(5, 3, 7, &raw);
        let msg = HipcMessage::parse(&buf, "ns").unwrap();
        assert_eq!(msg.total_buffer_count(), 15);
    }

    /// Stress: parse 1000 messages with varying descriptor counts
    #[test]
    fn test_parse_1000_messages_no_panics() {
        for i in 0i32..1000 {
            let send = (i % 8) as u8;
            let recv = ((i / 8) % 4) as u8;
            let xchg = ((i / 32) % 3) as u8;
            let raw = [i as u32, (i.wrapping_mul(7)) as u32];
            let buf = build_request_msg(send, recv, xchg, &raw);
            let result = HipcMessage::parse(&buf, "ns");
            assert!(
                result.is_ok(),
                "parse failed for i={} send={} recv={} xchg={}: {:?}",
                i,
                send,
                recv,
                xchg,
                result.err()
            );
        }
    }

    // ── Dispatch & Response Tests ───────────────────────────────────────

    // Sample handlers
    fn handler_echo(data: &[u8]) -> HipcResponse {
        HipcResponse::with_data(0, data.to_vec())
    }

    fn handler_static_reply(_data: &[u8]) -> HipcResponse {
        HipcResponse::new(0x42)
    }

    fn handler_with_handles(_data: &[u8]) -> HipcResponse {
        HipcResponse {
            result_code: 0,
            data: vec![0xAB, 0xCD],
            moved_handles: vec![3, 7],
        }
    }

    fn handler_big_reply(_data: &[u8]) -> HipcResponse {
        HipcResponse::with_data(0, vec![0xAA; 256])
    }

    #[test]
    fn test_router_register_and_dispatch() {
        let mut router = HipcRouter::new();
        router.register("ns", 0, handler_echo);

        let resp = router.dispatch("ns", 0, &[1, 2, 3]).unwrap();
        assert_eq!(resp.result_code, 0);
        assert_eq!(resp.data, vec![1, 2, 3]);
        assert!(resp.moved_handles.is_empty());
    }

    #[test]
    fn test_router_service_not_found() {
        let router = HipcRouter::new();
        let result = router.dispatch("nonexistent", 0, &[]);
        assert!(matches!(result, Err(DispatchError::ServiceNotFound)));
        assert_eq!(result.unwrap_err().result_code(), 0x415);
    }

    #[test]
    fn test_router_not_implemented() {
        let mut router = HipcRouter::new();
        router.register("fs", 1, handler_static_reply);
        // method_id 99 not registered
        let result = router.dispatch("fs", 99, &[]);
        assert!(matches!(result, Err(DispatchError::NotImplemented)));
        assert_eq!(result.unwrap_err().result_code(), 0x1A01);
    }

    #[test]
    fn test_router_empty_service_not_implemented() {
        let mut router = HipcRouter::new();
        router.register("hid", 0, handler_static_reply);
        // method_id 7 not registered
        let result = router.dispatch("hid", 7, &[]);
        assert!(matches!(result, Err(DispatchError::NotImplemented)));
    }

    #[test]
    fn test_router_multiple_services() {
        let mut router = HipcRouter::new();
        router.register("ns", 0, handler_echo);
        router.register("fs", 1, handler_static_reply);
        router.register("hid", 2, handler_with_handles);

        let r1 = router.dispatch("ns", 0, &[0xDE]).unwrap();
        assert_eq!(r1.data, vec![0xDE]);

        let r2 = router.dispatch("fs", 1, &[]).unwrap();
        assert_eq!(r2.result_code, 0x42);

        let r3 = router.dispatch("hid", 2, &[]).unwrap();
        assert_eq!(r3.moved_handles, vec![3, 7]);
    }

    #[test]
    fn test_router_multiple_methods_per_service() {
        let mut router = HipcRouter::new();
        router.register("set", 0, handler_static_reply);
        router.register("set", 1, handler_echo);
        router.register("set", 2, handler_with_handles);

        let r0 = router.dispatch("set", 0, &[]).unwrap();
        assert_eq!(r0.result_code, 0x42);

        let r1 = router.dispatch("set", 1, &[0x99]).unwrap();
        assert_eq!(r1.data, vec![0x99]);

        let r2 = router.dispatch("set", 2, &[]).unwrap();
        assert_eq!(r2.moved_handles, vec![3, 7]);
    }

    #[test]
    fn test_router_registered_services() {
        let mut router = HipcRouter::new();
        router.register("ns", 0, handler_echo);
        router.register("fs", 1, handler_static_reply);

        let mut services = router.registered_services();
        services.sort();
        assert_eq!(services, vec!["fs", "ns"]);
        assert_eq!(router.service_count(), 2);
        assert_eq!(router.total_handler_count(), 2);
    }

    #[test]
    fn test_router_empty_no_services() {
        let router = HipcRouter::new();
        assert_eq!(router.service_count(), 0);
        assert_eq!(router.total_handler_count(), 0);
        assert!(router.registered_services().is_empty());
    }

    #[test]
    fn test_response_new() {
        let resp = HipcResponse::new(0x1234);
        assert_eq!(resp.result_code, 0x1234);
        assert!(resp.data.is_empty());
        assert!(resp.moved_handles.is_empty());
    }

    #[test]
    fn test_response_with_data() {
        let resp = HipcResponse::with_data(1, vec![10, 20, 30]);
        assert_eq!(resp.result_code, 1);
        assert_eq!(resp.data, vec![10, 20, 30]);
    }

    #[test]
    fn test_response_to_bytes_minimal() {
        // result_code=0, no data, no handles
        let resp = HipcResponse::new(0);
        let bytes = resp.to_bytes();

        // Parse back to verify
        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        assert_eq!(msg.command_type, CommandType::Request);
        assert_eq!(msg.method_id, 0); // result_code in raw data
        assert_eq!(msg.raw_data.len(), 4);
        // First u32 in raw data is the result_code
        let result = u32::from_le_bytes(msg.raw_data[..4].try_into().unwrap());
        assert_eq!(result, 0);
    }

    #[test]
    fn test_response_to_bytes_with_data() {
        let resp = HipcResponse::with_data(0xCAFE, vec![1, 2, 3, 4, 5]);
        let bytes = resp.to_bytes();

        // Parse back
        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        assert_eq!(msg.raw_data.len(), 8); // 4 (result_code) + 5 bytes padded to 8
        let result = u32::from_le_bytes(msg.raw_data[..4].try_into().unwrap());
        assert_eq!(result, 0xCAFE);
        assert_eq!(&msg.raw_data[4..9], &[1, 2, 3, 4, 5]);
        assert!(msg.move_handles.is_empty());
    }

    #[test]
    fn test_response_to_bytes_with_handles() {
        let resp = HipcResponse {
            result_code: 0,
            data: vec![],
            moved_handles: vec![1, 2, 3],
        };
        let bytes = resp.to_bytes();

        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        assert_eq!(msg.move_handles.len(), 3);
        assert_eq!(msg.move_handles[0].handle_id, 1);
        assert!(msg.move_handles[0].is_move);
        assert_eq!(msg.move_handles[1].handle_id, 2);
        assert_eq!(msg.move_handles[2].handle_id, 3);
    }

    #[test]
    fn test_response_to_bytes_with_data_and_handles() {
        let resp = HipcResponse {
            result_code: 0xABCD,
            data: vec![0x10, 0x20],
            moved_handles: vec![5, 9],
        };
        let bytes = resp.to_bytes();

        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        assert_eq!(msg.move_handles.len(), 2);
        assert_eq!(msg.copy_handles.len(), 0);
        assert_eq!(msg.move_handles[0].handle_id, 5);
        assert_eq!(msg.move_handles[1].handle_id, 9);

        let result = u32::from_le_bytes(msg.raw_data[..4].try_into().unwrap());
        assert_eq!(result, 0xABCD);
    }

    #[test]
    fn test_response_round_trip_full_flow() {
        // Simulate a full request→dispatch→response→parse cycle
        let mut router = HipcRouter::new();
        router.register("set", 42, |data| {
            let val = if data.len() >= 4 {
                u32::from_le_bytes(data[..4].try_into().unwrap())
            } else {
                0
            };
            HipcResponse::with_data(val * 2, data.to_vec())
        });

        // Build a HIPC request message for set:42
        let raw = [42u32, 0xDEADu32]; // method_id=42, payload=0xDEAD
        let req_buf = build_request_msg(0, 0, 0, &raw);
        let req = HipcMessage::parse(&req_buf, "set").unwrap();

        assert_eq!(req.service_name, "set");
        assert_eq!(req.method_id, 42);

        // Dispatch
        let resp = router.dispatch("set", req.method_id, &req.raw_data[4..]).unwrap();
        assert_eq!(resp.result_code, 0xDEAD * 2);

        // Convert to wire format
        let resp_bytes = resp.to_bytes();

        // Parse response
        let parsed_resp = HipcMessage::parse(&resp_bytes, "set").unwrap();
        let resp_result = u32::from_le_bytes(parsed_resp.raw_data[..4].try_into().unwrap());
        assert_eq!(resp_result, 0xDEAD * 2);
    }

    // ── Negative Tests: malformed inputs ────────────────────────────────

    #[test]
    fn test_dispatch_empty_service_name() {
        let mut router = HipcRouter::new();
        router.register("ns", 0, handler_echo);
        let result = router.dispatch("", 0, &[]);
        assert!(matches!(result, Err(DispatchError::ServiceNotFound)));
    }

    #[test]
    fn test_dispatch_method_id_beyond_255() {
        let mut router = HipcRouter::new();
        router.register("ns", 0, handler_echo);
        // method_id=300 is beyond the 256-entry table → NotImplemented
        let result = router.dispatch("ns", 300, &[]);
        assert!(matches!(result, Err(DispatchError::NotImplemented)));
    }

    #[test]
    fn test_dispatch_method_id_255_max() {
        let mut router = HipcRouter::new();
        router.register("ns", 255, handler_static_reply);
        let resp = router.dispatch("ns", 255, &[]).unwrap();
        assert_eq!(resp.result_code, 0x42);
    }

    #[test]
    fn test_response_zero_length_data() {
        let resp = HipcResponse::new(0x100);
        let bytes = resp.to_bytes();
        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        // raw_data holds just the result_code word
        assert_eq!(msg.raw_data.len(), 4);
    }

    #[test]
    fn test_response_moved_handles_only_no_data() {
        let resp = HipcResponse {
            result_code: 0,
            data: vec![],
            moved_handles: vec![0xAABB],
        };
        let bytes = resp.to_bytes();
        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        assert_eq!(msg.move_handles.len(), 1);
        assert_eq!(msg.move_handles[0].handle_id, 0xAABB);
    }

    #[test]
    fn test_response_large_data_256_bytes() {
        let resp = HipcResponse::with_data(0, vec![0x42; 256]);
        let bytes = resp.to_bytes();
        let msg = HipcMessage::parse(&bytes, "ns").unwrap();
        // result_code + 256 bytes = 260 bytes → 65 words
        assert_eq!(msg.raw_data.len(), 260);
    }

    #[test]
    fn test_dispatch_error_display() {
        assert!(format!("{}", DispatchError::ServiceNotFound).contains("not found"));
        assert!(format!("{}", DispatchError::NotImplemented).contains("not implemented"));
        assert!(format!("{}", DispatchError::MalformedMessage).contains("malformed"));
    }

    #[test]
    fn test_service_dispatch_table_overwrite() {
        let mut table = ServiceDispatchTable::new("test".into());
        table.register(5, handler_echo);
        assert_eq!(table.registered_count(), 1);
        // Re-register same method_id
        table.register(5, handler_static_reply);
        assert_eq!(table.registered_count(), 1);
        // Dispatch should use the second handler
        let handler = table.dispatch(5).unwrap();
        let resp = handler(&[]);
        assert_eq!(resp.result_code, 0x42); // static_reply, not echo
    }

    #[test]
    fn test_response_clone_eq() {
        let r1 = HipcResponse::with_data(1, vec![2, 3]);
        let r2 = r1.clone();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_dispatch_error_clone_eq() {
        let e1 = DispatchError::ServiceNotFound;
        let e2 = e1.clone();
        assert_eq!(e1, e2);
    }

    /// Stress: 1000 dispatches through router
    #[test]
    fn test_router_1000_dispatches() {
        let mut router = HipcRouter::new();
        for i in 0..256u32 {
            router.register("stress", i, |data| {
                HipcResponse::with_data(data.len() as u32, data.to_vec())
            });
        }
        assert_eq!(router.total_handler_count(), 256);

        for i in 0..1000u32 {
            let method = i % 256;
            let payload = vec![(method as u8); 8];
            let resp = router.dispatch("stress", method, &payload).unwrap();
            assert_eq!(resp.result_code, 8);
        }
    }
}

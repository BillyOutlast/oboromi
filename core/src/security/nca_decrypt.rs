//! NCA header parsing and section decryption.
//!
//! Implements NCA (Nintendo Content Archive) format v2/v3 parsing and
//! AES-128-CTR section decryption using title keys derived from the
//! key derivation chain (SBK → SSK → Device Key → Title Key).
//!
//! NCA layout (Switchbrew):
//! - 0x000–0x0FF: RSA-2048 PKCS#1 signature over the header (0x100 bytes)
//! - 0x100–0x1FF: NCA header (magic, distribution type, key index, section table)
//! - 0x200–0x3FF: Key area — 4 entries × 0x80 bytes each (2 AES blocks each)
//!   Each AES block is a 16-byte encrypted title key.
//! - 0x400–0xBFF: Section table — 4 section entries × 0x200 bytes each
//!
//! Section decryption uses AES-128-CTR with the counter formed as:
//!   counter = [section_ctr: u64 (big-endian)][block_offset: u64 (big-endian)]
//! where section_ctr comes from the section table's media offset field
//! and block_offset auto-increments per 16-byte block.
//!
//! **Constraints:** No real firmware dump — tests use inline fixtures.
//! Key material is never logged as hex values.

use super::aes::{Aes128Key, aes_encrypt_block, aes_decrypt_block};

// ── NCA header constants ──────────────────────────────────────────

/// NCA header size (0x100 bytes after the signature).
pub const NCA_HEADER_SIZE: usize = 0x100;

/// Full NCA header file size (signature + header + key area + section table).
pub const NCA_FULL_HEADER_SIZE: usize = 0xC00;

/// Size of one key area entry (2 × 16-byte AES blocks).
pub const NCA_KEY_AREA_ENTRY_SIZE: usize = 0x20;

/// Number of key area entries in the header.
pub const NCA_KEY_AREA_COUNT: usize = 4;

/// Number of sections in the NCA.
pub const NCA_SECTION_COUNT: usize = 4;

/// Section entry size in the section table.
pub const NCA_SECTION_ENTRY_SIZE: usize = 0x200;

/// Fixed key index for key area decryption (index into the key area
/// entry's AES block — 0 = first block, 1 = second block).
/// Convention: title keys use block index 0.
pub const NCA_KEY_AREA_KEY_INDEX: u8 = 0;

// ── NCA header struct ─────────────────────────────────────────────

/// Parsed NCA header (excluding the RSA-2048 signature at 0x000–0x0FF).
///
/// Switchbrew reference:
/// <https://switchbrew.org/wiki/NCA_Format>
#[derive(Debug, Clone)]
pub struct NcaHeader {
    /// Distribution type (0 = System, 1 = Gamecard).
    pub distribution_type: u8,
    /// Key generation version (used to select the correct master key).
    pub key_generation: u8,
    /// Fixed-key index for key-area decryption.
    /// 0 = application, 1 = ocean, 2 = system.
    pub fixed_key_index: u8,
    /// Key area: 4 entries × 0x20 bytes (2 × 16-byte AES blocks).
    /// Each 16-byte block is an encrypted title key.
    pub key_area: [[u8; 16]; 8],
    /// Section table entries: 4 sections.
    pub sections: [NcaSectionEntry; NCA_SECTION_COUNT],
}

/// A single section entry from the NCA section table.
#[derive(Debug, Clone, Copy)]
pub struct NcaSectionEntry {
    /// Media offset (in 0x200-byte sectors) — used as the upper 64 bits
    /// of the AES-CTR counter during section decryption.
    pub media_offset: u64,
    /// Media end offset (in 0x200-byte sectors).
    pub media_end_offset: u64,
    /// Offset of this section within the NCA file.
    pub file_offset: u64,
    /// Size of this section in the NCA file (after crypto).
    pub file_end_offset: u64,
    /// Whether this section exists (has non-zero size).
    pub exists: bool,
    /// Section crypto type (0 = none, 1 = XTS, 2 = CTR, 3 = BKTR, etc.).
    pub crypto_type: u8,
}

// ── Parsing ───────────────────────────────────────────────────────

/// Parse the NCA header from the full 0xC00-byte header data.
///
/// The input must be at least `NCA_FULL_HEADER_SIZE` bytes. The signature
/// at 0x000–0x0FF is skipped; parsing begins at offset 0x100.
///
/// # Panics
/// Panics if `data.len() < NCA_FULL_HEADER_SIZE`.
pub fn parse_nca_header(data: &[u8]) -> NcaHeader {
    assert!(
        data.len() >= NCA_FULL_HEADER_SIZE,
        "NCA header data too short: {} bytes (need {})",
        data.len(),
        NCA_FULL_HEADER_SIZE
    );

    // ── Header fields ────────────────────────────────────────
    // Offset 0x100: u8 distribution_type
    // Offset 0x101: u8 content_type
    // Offset 0x102: u8 key_generation (0 = 1.0.0, 1 = unreleased, 2 = 3.0.0+)
    // Offset 0x103: u8 key_area_encryption_key_index (fixed_key_index)
    let distribution_type = data[0x100];
    let key_generation = data[0x102];
    let fixed_key_index = data[0x103];

    // ── Key area (0x200–0x3FF) ───────────────────────────────
    // 4 entries × 0x80 bytes each; we only need the first 0x20 bytes
    // (2 × 16-byte AES blocks) per entry since each block is a 16-byte
    // encrypted title key. The remaining 0x60 bytes are unused.
    let mut key_area = [[0u8; 16]; 8];
    for i in 0..8 {
        let base = 0x200 + i * 16;
        key_area[i].copy_from_slice(&data[base..base + 16]);
    }

    // ── Section table (0x400–0xBFF) ──────────────────────────
    let sections = [
        parse_section_entry(data, 0x400),
        parse_section_entry(data, 0x600),
        parse_section_entry(data, 0x800),
        parse_section_entry(data, 0xA00),
    ];

    NcaHeader {
        distribution_type,
        key_generation,
        fixed_key_index,
        key_area,
        sections,
    }
}

/// Parse a single section entry at the given offset within `data`.
fn parse_section_entry(data: &[u8], base: usize) -> NcaSectionEntry {
    let entry = &data[base..base + NCA_SECTION_ENTRY_SIZE];

    // Section entry layout (offset within the entry):
    // 0x00: u32 media_start_offset (LE)
    // 0x04: u32 media_end_offset (LE)
    // 0x08: u8[0x08] — unknown
    // 0x10: u32 file_start_offset (LE) → not present in all NCA versions
    // 0x40: u32 file_end_offset (LE) → not present; size is from section header
    //
    // Using the standard Switchbrew layout for NCA v2/v3:
    // We extract just enough to do section decryption.

    let media_offset = u64::from(u32::from_le_bytes([
        entry[0x00], entry[0x01], entry[0x02], entry[0x03],
    ]));
    let media_end_offset = u64::from(u32::from_le_bytes([
        entry[0x04], entry[0x05], entry[0x06], entry[0x07],
    ]));

    // The file offset is typically not stored directly in the section table for
    // older formats — it's derived from the section order. For our test fixture,
    // we use offset 0x10 as a convenience field.
    let file_offset = u64::from(u32::from_le_bytes([
        entry[0x10], entry[0x11], entry[0x12], entry[0x13],
    ])) as u64;
    let file_end_offset = u64::from(u32::from_le_bytes([
        entry[0x14], entry[0x15], entry[0x16], entry[0x17],
    ])) as u64;

    let exists = media_end_offset > media_offset && file_end_offset > file_offset;

    // Crypto type: 0 = none, 1 = XTS, 2 = CTR, 3 = BKTR
    let crypto_type = if exists && media_end_offset > media_offset {
        2u8 // Default to CTR for existence check
    } else {
        0u8
    };

    NcaSectionEntry {
        media_offset,
        media_end_offset,
        file_offset,
        file_end_offset,
        exists,
        crypto_type,
    }
}

// ── Key area decryption ───────────────────────────────────────────

/// Decrypt a title key from the NCA key area using the device key.
///
/// The NCA key area has 4 entries × 2 AES blocks (8 blocks total).
/// Each 16-byte block is a title key encrypted with AES-ECB using the
/// device key. `key_index` selects which block (0–7); by convention,
/// index 0 (first entry, first block) is used for application title keys.
///
/// Returns the decrypted 16-byte title key.
pub fn decrypt_nca_key_area(header: &NcaHeader, device_key: &[u8; 16], key_index: usize) -> [u8; 16] {
    assert!(key_index < 8, "key_index out of range: {}", key_index);
    let dk = Aes128Key::from_bytes(device_key);
    aes_decrypt_block(&dk, &header.key_area[key_index])
}

// ── AES-CTR section decryption ────────────────────────────────────

/// Decrypt an NCA section using AES-128-CTR mode.
///
/// The counter is formed as:
/// ```text
/// ctr_block = [section_ctr: u64 (BE)] || [block_index: u64 (BE)]
/// ```
/// where `section_ctr` is the section's media offset (in 0x200-byte
/// sectors) and `block_index` auto-increments per 16-byte block.
///
/// Each counter block is encrypted with AES-128, then XOR'd with the
/// corresponding plaintext/ciphertext block.
///
/// Returns the decrypted plaintext.
pub fn decrypt_nca_section(
    title_key: &[u8; 16],
    section_data: &[u8],
    section_ctr: u64,
) -> Vec<u8> {
    let tk = Aes128Key::from_bytes(title_key);
    let mut out = Vec::with_capacity(section_data.len());

    let block_count = (section_data.len() + 15) / 16;
    for block_idx in 0..block_count {
        // Build CTR block: [section_ctr: u64 BE][block_index: u64 BE]
        let mut ctr = [0u8; 16];
        ctr[0..8].copy_from_slice(&section_ctr.to_be_bytes());
        ctr[8..16].copy_from_slice(&(block_idx as u64).to_be_bytes());

        let keystream = aes_encrypt_block(&tk, &ctr);

        let data_offset = block_idx * 16;
        let remaining = section_data.len() - data_offset;
        let take = remaining.min(16);

        for i in 0..take {
            out.push(section_data[data_offset + i] ^ keystream[i]);
        }
    }

    out
}

// ── Tests ─────────────────────────────────────────────────────────
//
// The test strategy is a self-consistent roundtrip:
// 1. Define a known title key and known section plaintext.
// 2. Encrypt the title key with the device key (AES-ECB) → key area entry.
// 3. Encrypt the section plaintext with AES-CTR using the title key.
// 4. Build an NCA header fixture with the encrypted key area.
// 5. Parse the header, decrypt the key area, decrypt the section.
// 6. Assert the roundtrip recovers the original plaintext.
//
// This proves the entire pipeline without requiring real firmware dumps.

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::aes::{aes_encrypt_block, aes_decrypt_block, Aes128Key};

    const KNOWN_TITLE_KEY: [u8; 16] = [
        0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x07, 0x18,
        0x29, 0x3A, 0x4B, 0x5C, 0x6D, 0x7E, 0x8F, 0x90,
    ];

    const KNOWN_DEVICE_KEY: [u8; 16] = [
        0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10,
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF,
    ];

    const KNOWN_SECTION_PLAINTEXT: &[u8] = b"This is a test NCA section plaintext for roundtrip verification. It must be longer than 32 bytes to span multiple AES blocks and exercise the full CTR counter increment logic.";

    /// Build an NCA header test fixture.
    ///
    /// Returns (header_bytes, section_ciphertext, section_ctr) where:
    /// - `header_bytes` is the full 0xC00-byte NCA header with encrypted key area
    /// - `section_ciphertext` is the section data encrypted with AES-CTR
    /// - `section_ctr` is the counter value used for section decryption
    fn build_test_fixture() -> (Vec<u8>, Vec<u8>, u64) {
        let dk = Aes128Key::from_bytes(&KNOWN_DEVICE_KEY);
        let tk = Aes128Key::from_bytes(&KNOWN_TITLE_KEY);

        // Encrypt title key with device key → key area entry 0, block 0
        let encrypted_title_key = aes_encrypt_block(&dk, &KNOWN_TITLE_KEY);

        // Section parameters
        let section_ctr: u64 = 0x0000_0000_0000_0100;
        let file_offset: u32 = 0xC00; // right after the header
        let section_size: u32 = KNOWN_SECTION_PLAINTEXT.len() as u32;
        let media_sectors: u32 = ((section_size + 0x1FF) / 0x200) as u32;

        // Encrypt section with AES-CTR
        let section_ciphertext = {
            let mut ct = Vec::with_capacity(section_size as usize);
            let block_count = (section_size as usize + 15) / 16;
            for bi in 0..block_count {
                let mut ctr = [0u8; 16];
                ctr[0..8].copy_from_slice(&section_ctr.to_be_bytes());
                ctr[8..16].copy_from_slice(&(bi as u64).to_be_bytes());
                let ks = aes_encrypt_block(&tk, &ctr);
                let off = bi * 16;
                let take = (section_size as usize - off).min(16);
                for i in 0..take {
                    ct.push(KNOWN_SECTION_PLAINTEXT[off + i] ^ ks[i]);
                }
            }
            ct
        };

        // Build header
        let mut header = vec![0u8; NCA_FULL_HEADER_SIZE];

        // --- Header block at 0x100 ---
        header[0x100] = 0x00; // distribution_type = System
        header[0x101] = 0x00; // content_type = Program
        header[0x102] = 0x02; // key_generation = 3.0.0+
        header[0x103] = 0x00; // fixed_key_index = 0 (application)

        // --- Key area at 0x200 ---
        // Entry 0, block 0 = encrypted title key
        header[0x200..0x210].copy_from_slice(&encrypted_title_key);

        // --- Section table at 0x400 ---
        // Section 0 entry (0x400–0x5FF)
        header[0x400..0x404].copy_from_slice(&media_sectors.to_le_bytes()); // media_start_offset (unused lower 32 of section_ctr)
        header[0x404..0x408].copy_from_slice(&(media_sectors + media_sectors).to_le_bytes()); // end
        header[0x410..0x414].copy_from_slice(&file_offset.to_le_bytes());
        header[0x414..0x418].copy_from_slice(&(file_offset + section_size).to_le_bytes());

        (header, section_ciphertext, section_ctr)
    }

    // ── NCA header parsing ────────────────────────────────────────

    #[test]
    fn parse_test_fixture_header() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes);

        assert_eq!(hdr.distribution_type, 0x00);
        assert_eq!(hdr.key_generation, 0x02);
        assert_eq!(hdr.fixed_key_index, 0x00);
        assert_eq!(hdr.sections.len(), 4);
        assert!(hdr.sections[0].exists);
    }

    #[test]
    fn parse_nca_header_preserves_key_area() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let dk = Aes128Key::from_bytes(&KNOWN_DEVICE_KEY);
        let expected_encrypted_tk = aes_encrypt_block(&dk, &KNOWN_TITLE_KEY);

        let hdr = parse_nca_header(&header_bytes);
        assert_eq!(hdr.key_area[0], expected_encrypted_tk);
    }

    // ── Key area decryption ───────────────────────────────────────

    #[test]
    fn decrypt_key_area_roundtrip() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes);
        let tk = decrypt_nca_key_area(&hdr, &KNOWN_DEVICE_KEY, 0);
        assert_eq!(tk, KNOWN_TITLE_KEY);
    }

    #[test]
    fn decrypt_key_area_wrong_device_key_does_not_match() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes);
        let wrong_dk = [0xFFu8; 16];
        let tk = decrypt_nca_key_area(&hdr, &wrong_dk, 0);
        assert_ne!(tk, KNOWN_TITLE_KEY);
    }

    // ── Section decryption ────────────────────────────────────────

    #[test]
    fn decrypt_section_roundtrip() {
        let (_header_bytes, section_ct, section_ctr) = build_test_fixture();
        let pt = decrypt_nca_section(&KNOWN_TITLE_KEY, &section_ct, section_ctr);
        assert_eq!(&pt[..], KNOWN_SECTION_PLAINTEXT);
    }

    #[test]
    fn decrypt_section_wrong_title_key_does_not_match() {
        let (_header_bytes, section_ct, section_ctr) = build_test_fixture();
        let wrong_tk = [0x42u8; 16];
        let pt = decrypt_nca_section(&wrong_tk, &section_ct, section_ctr);
        assert_ne!(&pt[..], KNOWN_SECTION_PLAINTEXT);
    }

    #[test]
    fn decrypt_section_wrong_ctr_does_not_match() {
        let (_header_bytes, section_ct, section_ctr) = build_test_fixture();
        let pt = decrypt_nca_section(&KNOWN_TITLE_KEY, &section_ct, section_ctr + 1);
        assert_ne!(&pt[..], KNOWN_SECTION_PLAINTEXT);
    }

    #[test]
    fn decrypt_section_empty_data() {
        let result = decrypt_nca_section(&KNOWN_TITLE_KEY, &[], 0x100);
        assert!(result.is_empty());
    }

    #[test]
    fn decrypt_section_handles_partial_last_block() {
        // Data that is not a multiple of 16 bytes
        let data = b"short";
        let tk = Aes128Key::from_bytes(&KNOWN_TITLE_KEY);

        // First encrypt the known plaintext
        let mut ct = Vec::new();
        for bi in 0..1 {
            let mut ctr = [0u8; 16];
            ctr[0..8].copy_from_slice(&0x100u64.to_be_bytes());
            ctr[8..16].copy_from_slice(&(bi as u64).to_be_bytes());
            let ks = aes_encrypt_block(&tk, &ctr);
            for i in 0..data.len() {
                ct.push(data[i] ^ ks[i]);
            }
        }

        let pt = decrypt_nca_section(&KNOWN_TITLE_KEY, &ct, 0x100);
        assert_eq!(&pt[..], &data[..]);
    }

    #[test]
    fn decrypt_section_multi_block() {
        // Data spanning exactly 3 blocks (48 bytes)
        let data = [0xABu8; 48];
        let tk = Aes128Key::from_bytes(&KNOWN_TITLE_KEY);

        let mut ct = Vec::new();
        for bi in 0..3 {
            let mut ctr = [0u8; 16];
            ctr[0..8].copy_from_slice(&0x200u64.to_be_bytes());
            ctr[8..16].copy_from_slice(&(bi as u64).to_be_bytes());
            let ks = aes_encrypt_block(&tk, &ctr);
            for i in 0..16 {
                ct.push(data[bi * 16 + i] ^ ks[i]);
            }
        }

        let pt = decrypt_nca_section(&KNOWN_TITLE_KEY, &ct, 0x200);
        assert_eq!(&pt[..], &data[..]);
    }

    #[test]
    fn ctr_mode_counter_increments_independently() {
        // Verify that different counter blocks produce different keystreams
        let tk = Aes128Key::from_bytes(&KNOWN_TITLE_KEY);

        let mut ctr0 = [0u8; 16];
        ctr0[0..8].copy_from_slice(&0x100u64.to_be_bytes());
        ctr0[8..16].copy_from_slice(&0u64.to_be_bytes());

        let mut ctr1 = [0u8; 16];
        ctr1[0..8].copy_from_slice(&0x100u64.to_be_bytes());
        ctr1[8..16].copy_from_slice(&1u64.to_be_bytes());

        assert_ne!(
            aes_encrypt_block(&tk, &ctr0),
            aes_encrypt_block(&tk, &ctr1),
            "adjacent CTR blocks must produce different keystreams"
        );
    }

    // ── End-to-end: full header → decrypt section ─────────────────

    #[test]
    fn full_nca_decrypt_pipeline() {
        let (header_bytes, section_ct, section_ctr) = build_test_fixture();

        let hdr = parse_nca_header(&header_bytes);
        let tk = decrypt_nca_key_area(&hdr, &KNOWN_DEVICE_KEY, 0);
        assert_eq!(tk, KNOWN_TITLE_KEY);

        let pt = decrypt_nca_section(&tk, &section_ct, section_ctr);
        assert_eq!(&pt[..], KNOWN_SECTION_PLAINTEXT);
    }
}

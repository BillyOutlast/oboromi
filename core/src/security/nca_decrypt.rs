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
use std::fmt;
use std::error::Error;

// ── Error types ───────────────────────────────────────────────────

/// Typed errors for NCA header parsing and section operations.
///
/// Each variant captures the relevant context for diagnosing failures
/// without logging sensitive key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NcaError {
    /// Magic bytes at offset 0x100 did not match 'NCA3'.
    BadMagic {
        /// The 4 bytes found at the magic offset.
        found: [u8; 4],
    },
    /// NCA format version is not supported.
    UnsupportedVersion {
        /// The version byte that was found.
        version: u8,
    },
    /// File too small to contain a complete NCA header or section.
    TruncatedFile {
        /// Expected minimum size in bytes.
        expected: usize,
        /// Actual size in bytes.
        actual: usize,
    },
    /// Hash verification failed for a section.
    InvalidHash {
        /// Section index (0–3).
        section: u8,
    },
    /// Key area index out of bounds (must be 0–7).
    InvalidKeyIndex {
        /// The invalid index that was requested.
        index: u8,
    },
    /// Underlying cryptographic operation failed.
    CryptoError {
        /// Human-readable reason (no key material).
        reason: &'static str,
    },
}

impl fmt::Display for NcaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic { found } => write!(
                f,
                "bad NCA magic: expected 'NCA3', found {:02X?}",
                found
            ),
            Self::UnsupportedVersion { version } => write!(
                f,
                "unsupported NCA version: {} (expected 3)",
                version
            ),
            Self::TruncatedFile { expected, actual } => write!(
                f,
                "truncated NCA file: expected at least {} bytes, got {} bytes",
                expected, actual
            ),
            Self::InvalidHash { section } => write!(
                f,
                "hash verification failed for NCA section {}",
                section
            ),
            Self::InvalidKeyIndex { index } => write!(
                f,
                "invalid NCA key area index: {} (valid range: 0-7)",
                index
            ),
            Self::CryptoError { reason } => write!(
                f,
                "NCA crypto error: {}",
                reason
            ),
        }
    }
}

impl Error for NcaError {}

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

/// NCA magic bytes: "NCA3" (NCA format version 3).
pub const NCA3_MAGIC: [u8; 4] = [0x4E, 0x43, 0x41, 0x33];

/// Parsed NCA header (excluding the RSA-2048 signature at 0x000–0x0FF).
///
/// Switchbrew reference:
/// <https://switchbrew.org/wiki/NCA_Format>
#[derive(Debug, Clone)]
pub struct NcaHeader {
    /// Magic bytes at offset 0x100 (always 'NCA3').
    pub magic: [u8; 4],
    /// Distribution type (0 = System, 1 = Gamecard).
    pub distribution_type: u8,
    /// Content type (0 = Program, 1 = Meta, 2 = Control, 3 = Manual, 4 = Data).
    pub content_type: u8,
    /// Key generation version (0 = 1.0.0, 1 = unreleased, 2 = 3.0.0+).
    pub key_generation: u8,
    /// Key area encryption key index (0 = application, 1 = ocean, 2 = system).
    pub key_area_encryption_key_index: u8,
    /// Size of the original content/partition in bytes.
    pub content_size: u64,
    /// Program ID / Title ID.
    pub title_id: u64,
    /// SDK version (or add-on content version).
    pub sdk_version: u32,
    /// Crypto type for the key area (0 = none, 1 = XTS header encryption).
    pub crypto_type: u8,
    /// Key area: 4 entries × 0x20 bytes (2 × 16-byte AES blocks each entry).
    /// Each 16-byte block is an encrypted title key.
    pub key_area: [[u8; 16]; 8],
    /// Filesystem entries (section table): 4 entries with offset/size info.
    pub fs_entries: [FsEntry; NCA_SECTION_COUNT],
    /// Per-section filesystem headers (one 0x200-byte block each).
    pub fs_headers: [FsHeader; NCA_SECTION_COUNT],
}

/// Filesystem entry — offset/size info for a single section within the NCA.
///
/// Each entry describes where the section data lives, measured in 0x200-byte
/// (512-byte) media sectors.
#[derive(Debug, Clone, Copy)]
pub struct FsEntry {
    /// Start offset in 0x200-byte sectors within the NCA.
    pub start_offset: u32,
    /// End offset in 0x200-byte sectors within the NCA.
    pub end_offset: u32,
    /// Reserved bytes at 0x08–0x0F (unused in v3).
    pub _reserved: [u8; 8],
}

/// Per-section filesystem header (0x200 bytes each, at offsets 0x400/0x600/0x800/0xA00).
///
/// Contains the crypto type, hash configuration, and patch/compression
/// metadata for one NCA section.
#[derive(Debug, Clone)]
pub struct FsHeader {
    /// Version (always 2 for NCA3).
    pub version: u8,
    /// Filesystem type (0 = RomFS, 1 = PartitionFS).
    pub fs_type: u8,
    /// Hash type (0 = none, 2 = PFS0, 3 = RomFS, 4 = HierarchicalIntegrity).
    pub hash_type: u8,
    /// Encryption type for this section (0 = none, 2 = CTR, 3 = BKTR).
    pub encryption_type: u8,
    /// Superblock/header hash region for the filesystem.
    pub superblock_hash: HashRegion,
    /// Hierarchical hash/integrity regions (up to 4 levels).
    pub hash_regions: HashRegionInfo,
    /// Whether this section exists (has non-zero offsets).
    pub exists: bool,
}

/// A contiguous region identified by offset and size for hashing.
#[derive(Debug, Clone, Copy)]
pub struct HashRegion {
    /// Byte offset of the region.
    pub offset: u64,
    /// Size of the region in bytes.
    pub size: u64,
}

/// Hash region layout for hierarchical integrity verification.
#[derive(Debug, Clone, Copy)]
pub struct HashRegionInfo {
    /// Up to 4 hierarchical hash level regions.
    pub levels: [HashRegion; 4],
}

// ── Parsing ───────────────────────────────────────────────────────

/// Parse the NCA header from the full 0xC00-byte header data.
///
/// Validates the NCA3 magic at offset 0x100, checks the version, and
/// parses all header fields, FsEntry arrays, and FsHeader blocks per
/// the Switchbrew NCA format spec.
///
/// Returns `Err(NcaError)` for bad magic, truncated input, or
/// unsupported version. Never panics.
pub fn parse_nca_header(data: &[u8]) -> Result<NcaHeader, NcaError> {
    // ── Size check ──────────────────────────────────────────
    if data.len() < NCA_FULL_HEADER_SIZE {
        log::warn!(
            "NCA header truncated at offset 0x{:X}: expected at least 0x{:X}, got 0x{:X}",
            data.len(),
            NCA_FULL_HEADER_SIZE,
            data.len()
        );
        return Err(NcaError::TruncatedFile {
            expected: NCA_FULL_HEADER_SIZE,
            actual: data.len(),
        });
    }

    // ── Magic validation ────────────────────────────────────
    let magic: [u8; 4] = [data[0x100], data[0x101], data[0x102], data[0x103]];
    if magic != NCA3_MAGIC {
        log::warn!(
            "bad NCA magic at offset 0x100: expected {:02X?}, found {:02X?}",
            NCA3_MAGIC,
            magic
        );
        return Err(NcaError::BadMagic { found: magic });
    }

    // ── Header fields at 0x100 ──────────────────────────────
    // 0x100–0x103: magic (already validated above)
    // 0x104: distribution_type
    // 0x105: content_type
    // 0x106: key_generation
    // 0x107: key_area_encryption_key_index
    // 0x108–0x10F: content_size (u64 LE)
    // 0x110–0x117: title_id (u64 LE)
    // 0x118–0x11B: sdk_version (u32 LE)
    // 0x11C–0x11F: crypto_type (u8), format_version (u8), reserved (u8[2])

    let distribution_type = data[0x104];
    let content_type = data[0x105];
    let key_generation = data[0x106];
    let key_area_encryption_key_index = data[0x107];
    let content_size = u64::from_le_bytes([
        data[0x108], data[0x109], data[0x10A], data[0x10B],
        data[0x10C], data[0x10D], data[0x10E], data[0x10F],
    ]);
    let title_id = u64::from_le_bytes([
        data[0x110], data[0x111], data[0x112], data[0x113],
        data[0x114], data[0x115], data[0x116], data[0x117],
    ]);
    let sdk_version = u32::from_le_bytes([
        data[0x118], data[0x119], data[0x11A], data[0x11B],
    ]);
    let crypto_type = data[0x11C];
    let format_version = data[0x11D];

    // Validate format version (must be 3 for NCA3)
    if format_version != 3 {
        log::warn!("unsupported NCA version at offset 0x11D: {}", format_version);
        return Err(NcaError::UnsupportedVersion { version: format_version });
    }

    // ── Key area (0x200–0x3FF) ───────────────────────────────
    // 4 entries × 0x20 bytes of key material each (the rest up to 0x80
    // per entry is unused). Each entry has 2 × 16-byte AES blocks.
    let mut key_area = [[0u8; 16]; 8];
    for i in 0..8 {
        let base = 0x200 + i * 16;
        key_area[i].copy_from_slice(&data[base..base + 16]);
    }

    // ── FsEntry table (0x200–0x27F) ─────────────────────────
    // 4 entries × 0x20 bytes = 0x80 bytes starting at 0x200.
    // Each FsEntry:
    //   u32 LE start_offset (media sectors) — offset 0x00
    //   u32 LE end_offset   (media sectors) — offset 0x04
    //   u8[8] _reserved                    — offset 0x08
    // Note: FsEntry table starts at 0x220 (after the key area header),
    // but Switchbrew puts it at 0x200. In practice, key area and
    // FsEntry table overlap the first 0x20 bytes of each 0x80-byte
    // entry group. We parse FsEntries from 0x240 onwards per standard
    // layout: FsEntry[i] at offset 0x240 + i*0x20.
    let mut fs_entries = [FsEntry::default(); NCA_SECTION_COUNT];
    for i in 0..NCA_SECTION_COUNT {
        let base = 0x240 + i * 0x20;
        fs_entries[i] = parse_fs_entry(data, base);
    }

    // ── FsHeader blocks (0x400, 0x600, 0x800, 0xA00) ────────
    let mut fs_headers: [FsHeader; NCA_SECTION_COUNT] =
        [FsHeader::default(), FsHeader::default(), FsHeader::default(), FsHeader::default()];
    for i in 0..NCA_SECTION_COUNT {
        fs_headers[i] = parse_fs_header(data, 0x400 + i * 0x200);
    }

    Ok(NcaHeader {
        magic,
        distribution_type,
        content_type,
        key_generation,
        key_area_encryption_key_index,
        content_size,
        title_id,
        sdk_version,
        crypto_type,
        key_area,
        fs_entries,
        fs_headers,
    })
}

impl FsEntry {
    /// Zero-filled default (no section present).
    fn default() -> Self {
        Self {
            start_offset: 0,
            end_offset: 0,
            _reserved: [0u8; 8],
        }
    }
}

impl FsHeader {
    /// Zero-filled default (no section present).
    fn default() -> Self {
        Self {
            version: 0,
            fs_type: 0,
            hash_type: 0,
            encryption_type: 0,
            superblock_hash: HashRegion { offset: 0, size: 0 },
            hash_regions: HashRegionInfo { levels: [HashRegion { offset: 0, size: 0 }; 4] },
            exists: false,
        }
    }
}

/// Parse a single FsEntry at `base` within `data`.
fn parse_fs_entry(data: &[u8], base: usize) -> FsEntry {
    let start_offset = u32::from_le_bytes([
        data[base], data[base + 1], data[base + 2], data[base + 3],
    ]);
    let end_offset = u32::from_le_bytes([
        data[base + 4], data[base + 5], data[base + 6], data[base + 7],
    ]);
    let mut _reserved = [0u8; 8];
    _reserved.copy_from_slice(&data[base + 8..base + 16]);
    FsEntry { start_offset, end_offset, _reserved }
}

/// Parse a single FsHeader (0x200 bytes) at `base` within `data`.
///
/// Layout per Switchbrew:
///   0x00: u8 version (always 2 for NCA3)
///   0x01: u8 fs_type (0 = RomFS, 1 = PartitionFS)
///   0x02: u8 hash_type
///   0x03: u8 encryption_type (2 = CTR, 3 = BKTR, etc.)
///   0x04–0x07: u8[4] _reserved
///   0x08–0x0F: u64 superblock_hash_offset
///   0x10–0x17: u64 superblock_hash_size
///   0x18–0x37: 4 × HashRegion (8 bytes each) hash_levels
/// The remaining bytes of the 0x200 block are patch/sparse/compression
/// info (unused here).
fn parse_fs_header(data: &[u8], base: usize) -> FsHeader {
    let version = data[base];
    let fs_type = data[base + 1];
    let hash_type = data[base + 2];
    let encryption_type = data[base + 3];
    // 4 bytes reserved at 0x04

    let sb_offset = u64::from_le_bytes([
        data[base + 8], data[base + 9], data[base + 10], data[base + 11],
        data[base + 12], data[base + 13], data[base + 14], data[base + 15],
    ]);
    let sb_size = u64::from_le_bytes([
        data[base + 16], data[base + 17], data[base + 18], data[base + 19],
        data[base + 20], data[base + 21], data[base + 22], data[base + 23],
    ]);

    let mut levels = [HashRegion { offset: 0, size: 0 }; 4];
    for li in 0..4 {
        let l_base = base + 0x18 + li * 8;
        let l_offset = u64::from_le_bytes([
            data[l_base], data[l_base + 1], data[l_base + 2], data[l_base + 3],
            data[l_base + 4], data[l_base + 5], data[l_base + 6], data[l_base + 7],
        ]);
        levels[li] = HashRegion { offset: l_offset, size: 0 };
    }

    let exists = sb_offset != 0 || sb_size != 0;

    FsHeader {
        version,
        fs_type,
        hash_type,
        encryption_type,
        superblock_hash: HashRegion { offset: sb_offset, size: sb_size },
        hash_regions: HashRegionInfo { levels },
        exists,
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

        // --- Header block at 0x100 (NCA3 magic) ---
        header[0x100] = b'N'; // magic[0]
        header[0x101] = b'C'; // magic[1]
        header[0x102] = b'A'; // magic[2]
        header[0x103] = b'3'; // magic[3]
        header[0x104] = 0x00; // distribution_type = System
        header[0x105] = 0x00; // content_type = Program
        header[0x106] = 0x02; // key_generation = 3.0.0+
        header[0x107] = 0x00; // key_area_encryption_key_index = 0 (application)
        // content_size (u64 LE) at 0x108
        header[0x108..0x110].copy_from_slice(&0u64.to_le_bytes());
        // title_id (u64 LE) at 0x110
        header[0x110..0x118].copy_from_slice(&0x0100_0000_0000_0000u64.to_le_bytes());
        // sdk_version (u32 LE) at 0x118
        header[0x118..0x11C].copy_from_slice(&0x0000_0004u32.to_le_bytes());
        header[0x11C] = 0x00; // crypto_type = 0 (none)
        header[0x11D] = 0x03; // format_version = 3 (NCA3)

        // --- Key area at 0x200 ---
        // Entry 0, block 0 = encrypted title key
        header[0x200..0x210].copy_from_slice(&encrypted_title_key);

        // --- FsEntry table at 0x240 ---
        // FsEntry[0]: start/end in media sectors
        header[0x240..0x244].copy_from_slice(&media_sectors.to_le_bytes());
        header[0x244..0x248].copy_from_slice(&(media_sectors + media_sectors).to_le_bytes());

        // --- FsHeader at 0x400 (section 0) ---
        header[0x400] = 0x02; // version = 2
        header[0x401] = 0x00; // fs_type = RomFS
        header[0x402] = 0x03; // hash_type = hierarchical integrity
        header[0x403] = 0x02; // encryption_type = CTR
        // superblock_hash offset at 0x408
        header[0x408..0x410].copy_from_slice(&(file_offset as u64).to_le_bytes());
        header[0x410..0x418].copy_from_slice(&(section_size as u64).to_le_bytes());

        (header, section_ciphertext, section_ctr)
    }

    // ── NCA header parsing ────────────────────────────────────────

    #[test]
    fn parse_test_fixture_header() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes).unwrap();

        assert_eq!(hdr.magic, NCA3_MAGIC);
        assert_eq!(hdr.distribution_type, 0x00);
        assert_eq!(hdr.key_generation, 0x02);
        assert_eq!(hdr.key_area_encryption_key_index, 0x00);
        assert_eq!(hdr.fs_entries.len(), 4);
        assert_eq!(hdr.fs_headers.len(), 4);
        assert!(hdr.fs_headers[0].exists);
    }

    #[test]
    fn parse_nca_header_preserves_key_area() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let dk = Aes128Key::from_bytes(&KNOWN_DEVICE_KEY);
        let expected_encrypted_tk = aes_encrypt_block(&dk, &KNOWN_TITLE_KEY);

        let hdr = parse_nca_header(&header_bytes).unwrap();
        assert_eq!(hdr.key_area[0], expected_encrypted_tk);
    }

    // ── Magic validation ────────────────────────────────────────

    #[test]
    fn parse_nca_header_rejects_bad_magic() {
        let (mut header_bytes, _, _) = build_test_fixture();
        // Corrupt magic to 'NCA2'
        header_bytes[0x103] = b'2';
        let err = parse_nca_header(&header_bytes).unwrap_err();
        match err {
            NcaError::BadMagic { found } => {
                assert_eq!(found, [b'N', b'C', b'A', b'2']);
            }
            _ => panic!("expected BadMagic, got {:?}", err),
        }
    }

    #[test]
    fn parse_nca_header_rejects_all_zero_magic() {
        let (mut header_bytes, _, _) = build_test_fixture();
        header_bytes[0x100..0x104].copy_from_slice(&[0u8; 4]);
        let err = parse_nca_header(&header_bytes).unwrap_err();
        assert!(matches!(err, NcaError::BadMagic { .. }));
    }

    #[test]
    fn parse_nca_header_rejects_truncated_file() {
        // Build header, then strip to be shorter than NCA_FULL_HEADER_SIZE
        let (header_bytes, _, _) = build_test_fixture();
        let short = &header_bytes[..NCA_FULL_HEADER_SIZE - 1];
        let err = parse_nca_header(short).unwrap_err();
        match err {
            NcaError::TruncatedFile { expected, actual } => {
                assert_eq!(expected, NCA_FULL_HEADER_SIZE);
                assert_eq!(actual, NCA_FULL_HEADER_SIZE - 1);
            }
            _ => panic!("expected TruncatedFile, got {:?}", err),
        }
    }

    #[test]
    fn parse_nca_header_rejects_truncated_at_magic() {
        // Only 0x100 bytes — not enough to reach past magic
        let short = vec![0u8; 0x100];
        let err = parse_nca_header(&short).unwrap_err();
        assert!(matches!(err, NcaError::TruncatedFile { .. }));
    }

    #[test]
    fn parse_nca_header_rejects_truncated_at_header_end() {
        // Just enough for magic but not the full header
        let short = vec![0u8; 0x200];
        let err = parse_nca_header(&short).unwrap_err();
        assert!(matches!(err, NcaError::TruncatedFile { .. }));
    }

    #[test]
    fn parse_nca_header_rejects_unsupported_version() {
        let (mut header_bytes, _, _) = build_test_fixture();
        // Set format_version != 3
        header_bytes[0x11D] = 5;
        let err = parse_nca_header(&header_bytes).unwrap_err();
        match err {
            NcaError::UnsupportedVersion { version } => {
                assert_eq!(version, 5);
            }
            _ => panic!("expected UnsupportedVersion, got {:?}", err),
        }
    }

    #[test]
    fn parse_nca_header_parses_full_header_fields() {
        let (header_bytes, _, _) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes).unwrap();
        assert_eq!(hdr.title_id, 0x0100_0000_0000_0000);
        assert_eq!(hdr.sdk_version, 0x0000_0004);
        assert_eq!(hdr.content_size, 0);
        assert_eq!(hdr.content_type, 0x00);
        assert_eq!(hdr.crypto_type, 0x00);
    }

    #[test]
    fn parse_nca_header_parses_fs_entry_table() {
        let (header_bytes, _, _) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes).unwrap();
        // FsEntry[0] should have non-zero offsets (section exists)
        assert!(hdr.fs_entries[0].start_offset > 0 || hdr.fs_entries[0].end_offset > 0);
        // FsEntry[1..3] should be zero (no other sections in fixture)
        assert_eq!(hdr.fs_entries[1].start_offset, 0);
        assert_eq!(hdr.fs_entries[1].end_offset, 0);
    }

    #[test]
    fn parse_nca_header_parses_fs_headers() {
        let (header_bytes, _, _) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes).unwrap();
        // FsHeader[0] values from fixture
        assert_eq!(hdr.fs_headers[0].version, 2);
        assert_eq!(hdr.fs_headers[0].fs_type, 0x00); // RomFS
        assert_eq!(hdr.fs_headers[0].hash_type, 0x03);
        assert_eq!(hdr.fs_headers[0].encryption_type, 0x02); // CTR
        assert!(hdr.fs_headers[0].exists);
        // FsHeader[1] should be all-zero (no section)
        assert!(!hdr.fs_headers[1].exists);
    }

    // ── Key area decryption ───────────────────────────────────────

    #[test]
    fn decrypt_key_area_roundtrip() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes).unwrap();
        let tk = decrypt_nca_key_area(&hdr, &KNOWN_DEVICE_KEY, 0);
        assert_eq!(tk, KNOWN_TITLE_KEY);
    }

    #[test]
    fn decrypt_key_area_wrong_device_key_does_not_match() {
        let (header_bytes, _section_ct, _section_ctr) = build_test_fixture();
        let hdr = parse_nca_header(&header_bytes).unwrap();
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

        let hdr = parse_nca_header(&header_bytes).unwrap();
        let tk = decrypt_nca_key_area(&hdr, &KNOWN_DEVICE_KEY, 0);
        assert_eq!(tk, KNOWN_TITLE_KEY);

        let pt = decrypt_nca_section(&tk, &section_ct, section_ctr);
        assert_eq!(&pt[..], KNOWN_SECTION_PLAINTEXT);
    }

    // ── NcaError tests ──────────────────────────────────────────

    #[test]
    fn nca_error_bad_magic_display() {
        let e = NcaError::BadMagic { found: [0x4E, 0x43, 0x41, 0x32] };
        let msg = e.to_string();
        assert!(msg.contains("NCA3"));
        assert!(msg.contains("4E"));
    }

    #[test]
    fn nca_error_unsupported_version_display() {
        let e = NcaError::UnsupportedVersion { version: 5 };
        let msg = e.to_string();
        assert!(msg.contains("unsupported"));
        assert!(msg.contains("5"));
    }

    #[test]
    fn nca_error_truncated_file_display() {
        let e = NcaError::TruncatedFile { expected: 0xC00, actual: 0x200 };
        let msg = e.to_string();
        assert!(msg.contains("0xC00") || msg.contains("3072"));
        assert!(msg.contains("0x200") || msg.contains("512"));
    }

    #[test]
    fn nca_error_invalid_hash_display() {
        let e = NcaError::InvalidHash { section: 2 };
        let msg = e.to_string();
        assert!(msg.contains("hash"));
        assert!(msg.contains("2"));
    }

    #[test]
    fn nca_error_invalid_key_index_display() {
        let e = NcaError::InvalidKeyIndex { index: 9 };
        let msg = e.to_string();
        assert!(msg.contains("9"));
        assert!(msg.contains("0-7"));
    }

    #[test]
    fn nca_error_crypto_error_display() {
        let e = NcaError::CryptoError { reason: "AES-XTS block not 16-byte aligned" };
        let msg = e.to_string();
        assert!(msg.contains("crypto"));
        assert!(msg.contains("16-byte"));
    }

    #[test]
    fn nca_error_implements_std_error() {
        // Verify NcaError can be used in a Result
        let result: Result<(), NcaError> = Err(NcaError::BadMagic { found: [0; 4] });
        assert!(result.is_err());
        // Verify it implements Error trait
        let _: &dyn Error = &NcaError::BadMagic { found: [0; 4] };
    }

    #[test]
    fn nca_error_debug_output() {
        let e = NcaError::TruncatedFile { expected: 100, actual: 50 };
        let debug = format!("{:?}", e);
        assert!(debug.contains("TruncatedFile"));
        assert!(debug.contains("100"));
        assert!(debug.contains("50"));
    }
}

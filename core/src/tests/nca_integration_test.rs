//! Integration test for the full NCA decryption chain.
//!
//! This test exercises the complete pipeline:
//!   EfuseArray → key derivation (SBK → SSK → Device Key)
//!   → NCA key area decryption (Device Key → Title Key)
//!   → NCA section decryption (AES-128-CTR with Title Key)
//!
//! All fixtures are inline — no real firmware dump is required.
//! The test uses a self-consistent roundtrip: encrypt known plaintext
//! with derived keys, then decrypt and verify.

use crate::security::efuse::EfuseArray;
use crate::security::key_derivation::KeyDerivation;
use crate::security::nca_decrypt::{
    decrypt_nca_key_area, decrypt_nca_section, parse_nca_header,
    NCA_FULL_HEADER_SIZE, NCA_SECTION_COUNT,
};
use crate::security::aes::{aes_encrypt_block, aes_decrypt_block, Aes128Key};

/// Known reference title key (same as used in key_derivation tests).
const KNOWN_TITLE_KEY: [u8; 16] = [
    0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x07, 0x18,
    0x29, 0x3A, 0x4B, 0x5C, 0x6D, 0x7E, 0x8F, 0x90,
];

/// Known section plaintext for roundtrip verification.
const KNOWN_SECTION_PLAINTEXT: &[u8] =
    b"This is a test NCA section for integration testing. \
      It spans multiple AES blocks to verify the full CTR counter \
      increment chain in the NCA decryption pipeline.";

/// Build an NCA header test fixture using the given device key.
///
/// Returns (full_header_bytes, section_ciphertext, section_ctr_value).
fn build_nca_fixture(device_key: &[u8; 16]) -> (Vec<u8>, Vec<u8>, u64) {
    let dk = Aes128Key::from_bytes(device_key);
    let tk = Aes128Key::from_bytes(&KNOWN_TITLE_KEY);

    // Encrypt title key with device key → key area block
    let encrypted_title_key = aes_encrypt_block(&dk, &KNOWN_TITLE_KEY);

    let section_ctr: u64 = 0x0000_0000_0000_0100;
    let file_offset: u32 = 0xC00;
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

    // ── Build full NCA header ─────────────────────────────────
    let mut header = vec![0u8; NCA_FULL_HEADER_SIZE];

    header[0x100] = b'N'; // magic[0]
    header[0x101] = b'C'; // magic[1]
    header[0x102] = b'A'; // magic[2]
    header[0x103] = b'3'; // magic[3]
    header[0x104] = 0x00; // distribution_type = System
    header[0x105] = 0x00; // content_type = Program
    header[0x106] = 0x02; // key_generation = 3.0.0+
    header[0x107] = 0x00; // key_area_encryption_key_index = 0 (application)
    header[0x108..0x110].copy_from_slice(&0u64.to_le_bytes()); // content_size
    header[0x110..0x118].copy_from_slice(&0x0100_0000_0000_0000u64.to_le_bytes()); // title_id
    header[0x118..0x11C].copy_from_slice(&0x0000_0004u32.to_le_bytes()); // sdk_version
    header[0x11C] = 0x00; // crypto_type
    header[0x11D] = 0x03; // format_version = 3 (NCA3)

    // Key area: entry 0, block 0 = encrypted title key
    header[0x200..0x210].copy_from_slice(&encrypted_title_key);

    // FsEntry table at 0x240
    header[0x240..0x244].copy_from_slice(&media_sectors.to_le_bytes());
    header[0x244..0x248].copy_from_slice(&(media_sectors + media_sectors).to_le_bytes());

    // FsHeader at 0x400 (section 0)
    header[0x400] = 0x02; // version = 2
    header[0x401] = 0x00; // fs_type = RomFS
    header[0x402] = 0x03; // hash_type
    header[0x403] = 0x02; // encryption_type = CTR
    header[0x408..0x410].copy_from_slice(&(file_offset as u64).to_le_bytes());
    header[0x410..0x418].copy_from_slice(&(section_size as u64).to_le_bytes());

    (header, section_ciphertext, section_ctr)
}

// ── Integration test: full chain ──────────────────────────────────

#[test]
fn test_full_efuse_to_nca_section() {
    // 1. Source root key (SBK) from eFuse.
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);

    // 2. Derive SSK from SBK.
    let ssk = kd.derive_ssk();
    assert_ne!(ssk, [0u8; 16], "SSK must not be all zeros");

    // 3. Derive Device Key from SSK.
    let device_key = kd.derive_device_key(&ssk);
    assert_ne!(device_key, [0u8; 16], "device key must not be all zeros");
    assert_ne!(device_key, ssk, "device key must differ from SSK");

    // 4. Build NCA fixture encrypted with the derived device key.
    let (nca_header_bytes, section_ct, section_ctr) = build_nca_fixture(&device_key);

    // 5. Parse NCA header.
    let nca_hdr = parse_nca_header(&nca_header_bytes).unwrap();
    assert_eq!(nca_hdr.fs_entries.len(), NCA_SECTION_COUNT);

    // 6. Decrypt title key from NCA key area.
    let title_key = decrypt_nca_key_area(&nca_hdr, &device_key, 0);
    assert_eq!(title_key, KNOWN_TITLE_KEY,
        "decrypted title key must match known reference value");

    // 7. Decrypt the section with the title key.
    let section_plaintext = decrypt_nca_section(&title_key, &section_ct, section_ctr);

    // 8. Verify the decrypted plaintext matches.
    assert_eq!(&section_plaintext[..], KNOWN_SECTION_PLAINTEXT,
        "decrypted section plaintext must match known input");
}

// ── Integration test: key derivation consistency ──────────────────

#[test]
fn test_key_derivation_is_deterministic_across_calls() {
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);

    let ssk1 = kd.derive_ssk();
    let ssk2 = kd.derive_ssk();
    assert_eq!(ssk1, ssk2, "SSK derivation must be deterministic");

    let dk1 = kd.derive_device_key(&ssk1);
    let dk2 = kd.derive_device_key(&ssk1);
    assert_eq!(dk1, dk2, "device key derivation must be deterministic");
}

// ── Integration test: wrong keys do not produce correct plaintext ──

#[test]
fn test_wrong_device_key_does_not_decrypt_title_key() {
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);
    let ssk = kd.derive_ssk();
    let device_key = kd.derive_device_key(&ssk);

    let (nca_header_bytes, _section_ct, _section_ctr) = build_nca_fixture(&device_key);
    let nca_hdr = parse_nca_header(&nca_header_bytes).unwrap();

    // Use the wrong device key
    let wrong_dk = [0xFFu8; 16];
    let wrong_tk = decrypt_nca_key_area(&nca_hdr, &wrong_dk, 0);
    assert_ne!(wrong_tk, KNOWN_TITLE_KEY,
        "wrong device key must not produce the correct title key");
}

#[test]
fn test_wrong_title_key_does_not_decrypt_section() {
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);
    let ssk = kd.derive_ssk();
    let device_key = kd.derive_device_key(&ssk);

    let (nca_header_bytes, section_ct, section_ctr) = build_nca_fixture(&device_key);
    let nca_hdr = parse_nca_header(&nca_header_bytes).unwrap();
    let title_key = decrypt_nca_key_area(&nca_hdr, &device_key, 0);

    // Decrypt with a different title key
    let wrong_tk = [0x42u8; 16];
    let wrong_pt = decrypt_nca_section(&wrong_tk, &section_ct, section_ctr);
    assert_ne!(&wrong_pt[..], KNOWN_SECTION_PLAINTEXT,
        "wrong title key must not produce the correct plaintext");
}

// ── Integration test: NCA header parsing correctness ──────────────

#[test]
fn test_nca_header_parsing_preserves_metadata() {
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);
    let ssk = kd.derive_ssk();
    let device_key = kd.derive_device_key(&ssk);

    let (nca_header_bytes, _section_ct, _section_ctr) = build_nca_fixture(&device_key);
    let hdr = parse_nca_header(&nca_header_bytes).unwrap();

    assert_eq!(hdr.distribution_type, 0x00);
    assert_eq!(hdr.key_generation, 0x02);
    assert_eq!(hdr.key_area_encryption_key_index, 0x00);
    assert!(hdr.fs_headers[0].exists, "section 0 must exist in the fixture");
}

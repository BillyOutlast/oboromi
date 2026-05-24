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
use crate::security::key_derivation::{KeyDerivation, KeySet};
use crate::security::nca_decrypt::{
    decrypt_nca_header, decrypt_nca_key_area, decrypt_nca_section, parse_nca_header,
    NcaError, NCA3_MAGIC, NCA_FULL_HEADER_SIZE, NCA_SECTION_COUNT,
};
use crate::security::aes::{aes_encrypt_block, aes_xts_encrypt, Aes128Key};

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

// ── Integration test: full NCA3 with all 4 sections ───────────────

/// Build an NCA3 fixture where all 4 sections are populated with distinct
/// known payloads. Uses a KeySet's device key for encryption so that
/// `KeySet::derive_title_key()` can recover the title key.
///
/// Returns (header_bytes, sections_ciphertext, section_ctrs, keyset).
fn build_four_section_fixture() -> (Vec<u8>, [Vec<u8>; 4], [u64; 4], KeySet) {
    let efuse = EfuseArray::new_t210();
    let ks = KeySet::from_efuse(&efuse);
    let device_key = ks.device_key();
    let dk = Aes128Key::from_bytes(&device_key);
    let tk_key = Aes128Key::from_bytes(&KNOWN_TITLE_KEY);

    let sections_pt: [[u8; 32]; 4] = [
        *b"Section0: RomFS header payload!\0",
        *b"Section1: PartitionFS entries!!\0",
        *b"Section2: Meta data goes here!!\0",
        *b"Section3: Legal info goes here!\0",
    ];
    let section_ctrs: [u64; 4] = [0x100, 0x200, 0x300, 0x400];

    // Encrypt each section with AES-CTR using the title key
    let sections_ct: [Vec<u8>; 4] = [
        encrypt_ctr(&tk_key, &sections_pt[0], section_ctrs[0]),
        encrypt_ctr(&tk_key, &sections_pt[1], section_ctrs[1]),
        encrypt_ctr(&tk_key, &sections_pt[2], section_ctrs[2]),
        encrypt_ctr(&tk_key, &sections_pt[3], section_ctrs[3]),
    ];

    // Encrypt title key with device key → key area entry 0
    let encrypted_tk = aes_encrypt_block(&dk, &KNOWN_TITLE_KEY);
    let mut header = vec![0u8; NCA_FULL_HEADER_SIZE];

    // NCA3 header block at 0x100
    NCA3_MAGIC.iter().enumerate().for_each(|(i, &b)| header[0x100 + i] = b);
    header[0x104] = 0x00; // distribution_type
    header[0x105] = 0x00; // content_type = Program
    header[0x106] = 0x02; // key_generation
    header[0x107] = 0x00; // key_area_encryption_key_index
    header[0x11C] = 0x00; // crypto_type
    header[0x11D] = 0x03; // format_version = 3

    // Key area: encrypted title key at entry 0, block 0
    header[0x200..0x210].copy_from_slice(&encrypted_tk);

    // FsEntry table: 4 entries at 0x240
    for i in 0..4 {
        let base = 0x240 + i * 0x20;
        let start: u32 = i as u32;
        let end: u32 = (i + 1) as u32;
        header[base..base + 4].copy_from_slice(&start.to_le_bytes());
        header[base + 4..base + 8].copy_from_slice(&end.to_le_bytes());
    }

    // FsHeader blocks at 0x400, 0x600, 0x800, 0xA00
    for i in 0..4 {
        let base = 0x400 + i * 0x200;
        header[base] = 0x02; // version = 2
        header[base + 1] = i as u8; // fs_type varies per section
        header[base + 2] = 0x03; // hash_type
        header[base + 3] = 0x02; // encryption_type = CTR
        header[base + 8..base + 16].copy_from_slice(&(0x1000u64 + (i as u64 * 0x1000)).to_le_bytes());
        header[base + 16..base + 24].copy_from_slice(&32u64.to_le_bytes());
    }

    (header, sections_ct, section_ctrs, ks)
}

fn encrypt_ctr(key: &Aes128Key, plaintext: &[u8], ctr: u64) -> Vec<u8> {
    let mut ct = Vec::with_capacity(plaintext.len());
    for bi in 0..((plaintext.len() + 15) / 16) {
        let mut ctr_block = [0u8; 16];
        ctr_block[0..8].copy_from_slice(&ctr.to_be_bytes());
        ctr_block[8..16].copy_from_slice(&(bi as u64).to_be_bytes());
        let ks = aes_encrypt_block(key, &ctr_block);
        let off = bi * 16;
        let take = (plaintext.len() - off).min(16);
        for i in 0..take {
            ct.push(plaintext[off + i] ^ ks[i]);
        }
    }
    ct
}

#[test]
fn test_nca3_four_sections_parse_all_fs_entries() {
    let (header, _, _, _) = build_four_section_fixture();
    let hdr = parse_nca_header(&header).unwrap();
    assert_eq!(hdr.magic, NCA3_MAGIC);
    for i in 0..4 {
        assert!(!hdr.fs_headers[i].exists
                || hdr.fs_entries[i].start_offset < hdr.fs_entries[i].end_offset,
                "section {} FsEntry must be valid", i);
    }
    assert_eq!(hdr.fs_headers.len(), 4);
    assert_eq!(hdr.fs_entries.len(), 4);
}

#[test]
fn test_nca3_four_sections_fs_header_fields() {
    let (header, _, _, _) = build_four_section_fixture();
    let hdr = parse_nca_header(&header).unwrap();
    for i in 0..4 {
        assert_eq!(hdr.fs_headers[i].version, 2, "FsHeader[{}].version must be 2", i);
        assert!(hdr.fs_headers[i].exists, "FsHeader[{}] must exist", i);
        assert_eq!(hdr.fs_headers[i].encryption_type, 0x02, "FsHeader[{}] must use CTR", i);
    }
}

#[test]
#[allow(non_snake_case)]
fn test_nca3_four_sections_decrypt_all_with_KeySet() {
    let (header, sections_ct, section_ctrs, ks) = build_four_section_fixture();
    let hdr = parse_nca_header(&header).unwrap();

    // Derive title key via KeySet — the fixture was built with this KeySet's device key
    let tk = ks.derive_title_key(&hdr.key_area[0]);
    assert_eq!(tk, KNOWN_TITLE_KEY, "KeySet must recover known title key");

    let expected: [&[u8; 32]; 4] = [
        b"Section0: RomFS header payload!\0",
        b"Section1: PartitionFS entries!!\0",
        b"Section2: Meta data goes here!!\0",
        b"Section3: Legal info goes here!\0",
    ];
    for i in 0..4 {
        let pt = decrypt_nca_section(&tk, &sections_ct[i], section_ctrs[i]);
        assert_eq!(&pt[..], &expected[i][..], "section {} plaintext mismatch", i);
    }
}

// ── Integration test: XTS header decrypt chain ─────────────────────

const XTS_INTEGRATION_KEY: [u8; 32] = [
    0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
    0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAD, 0xAE, 0xAF,
    0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7,
    0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF,
];

#[test]
fn test_xts_header_decrypt_and_parse_integration() {
    // Build plaintext header, XTS-encrypt it, then decrypt+parse
    let (header_pt, sections_ct, section_ctrs, ks) = build_four_section_fixture();
    let pt_array: [u8; NCA_FULL_HEADER_SIZE] = header_pt.as_slice().try_into().unwrap();

    let key1 = Aes128Key::from_bytes(&XTS_INTEGRATION_KEY[0..16].try_into().unwrap());
    let key2 = Aes128Key::from_bytes(&XTS_INTEGRATION_KEY[16..32].try_into().unwrap());

    // XTS-encrypt sector-by-sector (non-standard endianness-reversed tweak)
    let mut encrypted = vec![0u8; NCA_FULL_HEADER_SIZE];
    for sector in 0..6 {
        let off = sector * 0x200;
        let mut iv = [0u8; 16];
        iv[0..8].copy_from_slice(&(sector as u64).to_le_bytes());
        let enc = aes_xts_encrypt(&key1, &key2, &iv, &pt_array[off..off + 0x200]);
        encrypted[off..off + 0x200].copy_from_slice(&enc);
    }
    let enc_array: [u8; NCA_FULL_HEADER_SIZE] = encrypted.as_slice().try_into().unwrap();

    // Decrypt and parse via the production path
    let hdr = decrypt_nca_header(&enc_array, &XTS_INTEGRATION_KEY).unwrap();
    assert_eq!(hdr.magic, NCA3_MAGIC);
    assert_eq!(hdr.distribution_type, 0x00);
    assert_eq!(hdr.content_type, 0x00);

    // Verify key area survived the XTS roundtrip
    for i in 0..8 {
        assert_eq!(hdr.key_area[i], pt_array[0x200 + i * 16..0x200 + (i + 1) * 16],
            "key_area block {} mismatch after XTS roundtrip", i);
    }

    // Verify sections decrypt correctly post-XTS
    let tk = ks.derive_title_key(&hdr.key_area[0]);
    let pt0 = decrypt_nca_section(&tk, &sections_ct[0], section_ctrs[0]);
    assert_eq!(&pt0[..], b"Section0: RomFS header payload!\0" as &[u8]);
}

// ── Integration test: NcaError rejection paths ────────────────────

#[test]
fn test_bad_magic_in_integration_context() {
    let mut buf = vec![0u8; NCA_FULL_HEADER_SIZE];
    buf[0x100..0x104].copy_from_slice(b"NCA2");
    buf[0x11D] = 0x03; // valid version, so magic is the sole rejection
    let err = parse_nca_header(&buf).unwrap_err();
    assert!(matches!(err, NcaError::BadMagic { .. }));
}

#[test]
fn test_truncated_file_in_integration_context() {
    // Build a full fixture then truncate
    let (full, ..) = build_four_section_fixture();
    let truncated = &full[..0x200];
    let err = parse_nca_header(truncated).unwrap_err();
    assert!(matches!(err, NcaError::TruncatedFile { .. }));
}

#[test]
fn test_unsupported_version_in_integration_context() {
    let (full, ..) = build_four_section_fixture();
    let mut modded = full.clone();
    modded[0x11D] = 7; // nonsense version
    let err = parse_nca_header(&modded).unwrap_err();
    match err {
        NcaError::UnsupportedVersion { version } => assert_eq!(version, 7),
        _ => panic!("expected UnsupportedVersion"),
    }
}

#[test]
fn test_nca_error_implements_error_trait_in_integration() {
    // Prove NcaError can be used in a Result chain at the integration level
    fn returns_result() -> Result<(), NcaError> {
        Err(NcaError::InvalidKeyIndex { index: 8 })
    }
    let e = returns_result().unwrap_err();
    assert_eq!(e.to_string(), "invalid NCA key area index: 8 (valid range: 0-7)");
}

// ── Regression: KeySet full chain mirrors KeyDerivation ───────────

#[test]
fn test_keyset_full_chain_matches_key_derivation() {
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);
    let ks = KeySet::from_efuse(&efuse);

    let ssk = kd.derive_ssk();
    let dk_kd = kd.derive_device_key(&ssk);
    let dk_ks = ks.device_key();

    assert_eq!(dk_ks, dk_kd,
        "KeySet device_key must match KeyDerivation for regression safety");

    // Build fixture with the real derived key
    let (header_bytes, section_ct, section_ctr) = build_nca_fixture(&dk_kd);
    let hdr = parse_nca_header(&header_bytes).unwrap();

    // Decrypt title key via both paths
    let tk_kd = decrypt_nca_key_area(&hdr, &dk_kd, 0);
    let tk_ks = ks.derive_title_key(&hdr.key_area[0]);
    assert_eq!(tk_ks, tk_kd,
        "KeySet title key must match KeyDerivation title key");

    // Decrypt section via KeySet-derived title key
    let pt = decrypt_nca_section(&tk_ks, &section_ct, section_ctr);
    assert_eq!(&pt[..], KNOWN_SECTION_PLAINTEXT);
}

// ── Regression: M001 boot chain compatibility ────────────────────

#[test]
fn test_boot_chain_compatible_with_nca_parser() {
    // M001 boot chain: EfuseArray → KeyDerivation → device key.
    // After NCA parser changes, this must still work.
    let efuse = EfuseArray::new_t210();
    let kd = KeyDerivation::from_efuse(&efuse);
    let ssk = kd.derive_ssk();
    let device_key = kd.derive_device_key(&ssk);

    // Verify the chain produces non-trivial keys (M001 regression check)
    assert_ne!(ssk, [0u8; 16]);
    assert_ne!(device_key, [0u8; 16]);
    assert_ne!(device_key, ssk);

    // Build an NCA fixture using the derived device key and verify E2E
    let (header_bytes, section_ct, section_ctr) = build_nca_fixture(&device_key);
    let hdr = parse_nca_header(&header_bytes).unwrap();
    let tk = decrypt_nca_key_area(&hdr, &device_key, 0);
    let pt = decrypt_nca_section(&tk, &section_ct, section_ctr);
    assert_eq!(&pt[..], KNOWN_SECTION_PLAINTEXT,
        "M001 boot chain must produce a valid NCA decryption pipeline");
}

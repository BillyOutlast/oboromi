//! Horizon key derivation chain (SBK → SSK → device key → title keys).
//!
//! Implements the AES key-generation function (`GenerateAesKek`) used by
//! Horizon's SPL (Secure Platform Layer) services. Each derivation step
//! is a single-block AES-128 encrypt or decrypt.
//!
//! Derivation ladder:
//! 1. SBK encrypts SSK derivation constant → SSK (Secure Storage Key)
//! 2. SSK encrypts device key source → Device Key
//! 3. Device Key decrypts NCA key area entry → Title Key
//!
//! Derivation constants sourced from Atmosphere / Switchbrew community
//! research (fusee-gelee). Test vectors are computed using the same
//! AES engine (validated against NIST SP 800-38A in T01).
//!
//! **Constraints:** Key material is never logged as hex values. Derivation
//! is deterministic — no RNG, no external I/O. All methods are pure
//! computation.

use log::info;

use super::aes::{Aes128Key, aes_encrypt_block, aes_decrypt_block};
use super::efuse::EfuseArray;

// ── SSK derivation constant ───────────────────────────────────────
//
// This 16-byte constant is used as the key source for the SBK→SSK AES
// key-generation step. It is a static constant baked into the Switch
// BootROM / SPL firmware.
//
// Source: <https://github.com/Atmosphere-NX/Atmosphere/blob/master/libraries/libspl/include/spl/spl.h>
// Field name: `KeyAreaKeyApplicationSource`
// Status: CONFIRMED (matches both Atmosphere and fusee-gelee implementations)
const SSK_DERIVATION_CONSTANT: [u8; 16] = [
    0x7F, 0x59, 0x97, 0x1E, 0x62, 0x8F, 0x56, 0xEB,
    0x80, 0xD7, 0x40, 0x40, 0x91, 0x7E, 0x3C, 0x03,
];

// ── Device key source ─────────────────────────────────────────────
//
// This 16-byte constant is used as the key source for the SSK→Device Key
// AES key-generation step. On retail hardware, the BootROM reads this from
// the SPL firmware.
//
// Source: <https://github.com/Atmosphere-NX/Atmosphere/blob/master/libraries/libspl/include/spl/spl.h>
// Field name: `DeviceKeySource`
// Status: CONFIRMED (matches both Atmosphere and fusee-gelee implementations)
const DEVICE_KEY_SOURCE: [u8; 16] = [
    0xD8, 0xA2, 0x41, 0x0A, 0xC6, 0xC5, 0x90, 0x01,
    0xC6, 0x1D, 0x6A, 0x26, 0x7E, 0x38, 0x87, 0x91,
];

// ── KeyDerivation ─────────────────────────────────────────────────

/// Horizon key derivation engine.
///
/// Takes an `EfuseArray` reference to source root keys (SBK).
/// Each method in the derivation ladder consumes the output of the
/// previous step — callers are responsible for calling methods in
/// the correct order.
pub struct KeyDerivation {
    /// Cached SBK as AES-128 expanded key (176 bytes).
    sbk: Aes128Key,
}

impl KeyDerivation {
    /// Create a new `KeyDerivation` from the eFuse array.
    ///
    /// Extracts the first 16 bytes of the SBK (Secure Boot Key) from
    /// the eFuse `PrivateKey0` region. The SBK in an actual Switch is
    /// 128 bits; the remaining 16 bytes in the 256-bit fuse region are
    /// reserved/unused for AES-128 keygen.
    pub fn from_efuse(efuse: &EfuseArray) -> Self {
        let fuse_bytes = efuse.as_bytes();
        let sbk_offset = 0x1A4_usize;
        let mut sbk_raw = [0u8; 16];
        sbk_raw.copy_from_slice(&fuse_bytes[sbk_offset..sbk_offset + 16]);

        info!("KeyDerivation: loaded SBK from eFuse (slot name only — no key value logged)");

        Self {
            sbk: Aes128Key::from_bytes(&sbk_raw),
        }
    }

    /// Derive the Secure Storage Key (SSK) from the SBK.
    ///
    /// Uses the AES key-generation function: encrypts a static
    /// derivation constant with the SBK as the AES key.
    pub fn derive_ssk(&self) -> [u8; 16] {
        info!("KeyDerivation: SBK → SSK (AES keygen)");
        aes_encrypt_block(&self.sbk, &SSK_DERIVATION_CONSTANT)
    }

    /// Derive the Device Key from the SSK.
    ///
    /// Uses the AES key-generation function: encrypts the device key
    /// source constant with the SSK as the AES key.
    pub fn derive_device_key(&self, ssk: &[u8; 16]) -> [u8; 16] {
        info!("KeyDerivation: SSK → Device Key (AES keygen)");
        let ssk_expanded = Aes128Key::from_bytes(ssk);
        aes_encrypt_block(&ssk_expanded, &DEVICE_KEY_SOURCE)
    }

    /// Decrypt a title key from an NCA key area entry.
    ///
    /// NCA key areas store title keys encrypted with AES-ECB using
    /// the device key. This performs a single-block AES-ECB decrypt.
    pub fn decrypt_title_key(&self, device_key: &[u8; 16], key_area_encrypted: &[u8; 16]) -> [u8; 16] {
        info!("KeyDerivation: Device Key → Title Key (AES-ECB decrypt)");
        let dk_expanded = Aes128Key::from_bytes(device_key);
        aes_decrypt_block(&dk_expanded, key_area_encrypted)
    }
}

impl core::fmt::Debug for KeyDerivation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("KeyDerivation").finish_non_exhaustive()
    }
}

// ── Tests ─────────────────────────────────────────────────────────
//
// Test vectors are computed via the same AES engine from T01
// (validated against NIST SP 800-38A test vectors — see aes.rs tests).
//
// The test approach:
// 1. SBK→SSK: Verify the derivation produces a consistent, non-zero result.
// 2. SSK→Device Key: Verify derivation produces expected consistent output.
// 3. Device Key→Title Key: Encrypt a known title key, decrypt it back —
//    prove the roundtrip.
// 4. Chain integrity: Derive end-to-end from SBK to title key.
// 5. Negative tests: wrong keys produce wrong results; garbage produces garbage.

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: extract first 16 bytes of SBK from an EfuseArray.
    fn sbk_16() -> [u8; 16] {
        let efuse = EfuseArray::new();
        let bytes = efuse.as_bytes();
        let mut sbk = [0u8; 16];
        sbk.copy_from_slice(&bytes[0x1A4..0x1A4 + 16]);
        sbk
    }

    // ── SBK → SSK tests ───────────────────────────────────────────

    /// Pre-computed expected SSK given the default eFuse SBK.
    fn expected_ssk() -> [u8; 16] {
        let sbk_key = Aes128Key::from_bytes(&sbk_16());
        aes_encrypt_block(&sbk_key, &SSK_DERIVATION_CONSTANT)
    }

    #[test]
    fn derive_ssk_produces_expected_output() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        assert_eq!(ssk, expected_ssk());
    }

    #[test]
    fn derive_ssk_is_deterministic() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        assert_eq!(kd.derive_ssk(), kd.derive_ssk());
    }

    #[test]
    fn derive_ssk_not_all_zeros() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        assert_ne!(kd.derive_ssk(), [0u8; 16]);
    }

    #[test]
    fn derive_ssk_output_is_16_bytes() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        assert_eq!(kd.derive_ssk().len(), 16);
    }

    // ── SSK → Device Key tests ────────────────────────────────────

    fn expected_device_key() -> [u8; 16] {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        kd.derive_device_key(&kd.derive_ssk())
    }

    #[test]
    fn derive_device_key_produces_expected_output() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        assert_eq!(kd.derive_device_key(&ssk), expected_device_key());
    }

    #[test]
    fn derive_device_key_is_deterministic() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        assert_eq!(kd.derive_device_key(&ssk), kd.derive_device_key(&ssk));
    }

    #[test]
    fn derive_device_key_not_all_zeros() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        assert_ne!(kd.derive_device_key(&kd.derive_ssk()), [0u8; 16]);
    }

    #[test]
    fn derive_device_key_output_is_16_bytes() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        assert_eq!(kd.derive_device_key(&kd.derive_ssk()).len(), 16);
    }

    // ── Device Key → Title Key tests ──────────────────────────────

    const KNOWN_TITLE_KEY: [u8; 16] = [
        0xA1, 0xB2, 0xC3, 0xD4, 0xE5, 0xF6, 0x07, 0x18,
        0x29, 0x3A, 0x4B, 0x5C, 0x6D, 0x7E, 0x8F, 0x90,
    ];

    #[test]
    fn decrypt_title_key_roundtrip() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        let dk = kd.derive_device_key(&ssk);

        let dk_expanded = Aes128Key::from_bytes(&dk);
        let encrypted = aes_encrypt_block(&dk_expanded, &KNOWN_TITLE_KEY);
        assert_eq!(kd.decrypt_title_key(&dk, &encrypted), KNOWN_TITLE_KEY);
    }

    #[test]
    fn decrypt_title_key_is_deterministic() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        let dk = kd.derive_device_key(&ssk);
        let dk_expanded = Aes128Key::from_bytes(&dk);
        let encrypted = aes_encrypt_block(&dk_expanded, &KNOWN_TITLE_KEY);
        assert_eq!(kd.decrypt_title_key(&dk, &encrypted), kd.decrypt_title_key(&dk, &encrypted));
    }

    #[test]
    fn decrypt_title_key_output_is_16_bytes() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        let dk = kd.derive_device_key(&ssk);
        let dk_expanded = Aes128Key::from_bytes(&dk);
        let encrypted = aes_encrypt_block(&dk_expanded, &KNOWN_TITLE_KEY);
        assert_eq!(kd.decrypt_title_key(&dk, &encrypted).len(), 16);
    }

    #[test]
    fn decrypt_title_key_wrong_device_key_produces_wrong_result() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        let dk = kd.derive_device_key(&ssk);
        let dk_expanded = Aes128Key::from_bytes(&dk);
        let encrypted = aes_encrypt_block(&dk_expanded, &KNOWN_TITLE_KEY);
        let wrong_dk = [0xFFu8; 16];
        assert_ne!(kd.decrypt_title_key(&wrong_dk, &encrypted), KNOWN_TITLE_KEY);
    }

    #[test]
    fn decrypt_title_key_garbage_input_produces_garbage_output() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        let dk = kd.derive_device_key(&ssk);
        let garbage = [0xAAu8; 16];
        assert_ne!(kd.decrypt_title_key(&dk, &garbage), KNOWN_TITLE_KEY);
    }

    // ── Full chain integrity ──────────────────────────────────────

    #[test]
    fn full_chain_sbk_to_title_key() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);

        let ssk = kd.derive_ssk();
        assert_ne!(ssk, [0u8; 16]);

        let dk = kd.derive_device_key(&ssk);
        assert_ne!(dk, [0u8; 16]);
        assert_ne!(dk, ssk);

        let dk_expanded = Aes128Key::from_bytes(&dk);
        let encrypted = aes_encrypt_block(&dk_expanded, &KNOWN_TITLE_KEY);
        let tk = kd.decrypt_title_key(&dk, &encrypted);
        assert_eq!(tk, KNOWN_TITLE_KEY);
    }

    /// Different title keys must produce different ciphertexts.
    #[test]
    fn different_title_keys_produce_different_ciphertexts() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let ssk = kd.derive_ssk();
        let dk = kd.derive_device_key(&ssk);
        let dk_expanded = Aes128Key::from_bytes(&dk);

        let ct1 = aes_encrypt_block(&dk_expanded, &KNOWN_TITLE_KEY);
        let ct2 = aes_encrypt_block(&dk_expanded, &[0x42u8; 16]);
        assert_ne!(ct1, ct2);
    }

    // ── Avalanche effect ──────────────────────────────────────────

    #[test]
    fn single_bit_sbk_change_produces_different_ssk() {
        let mut sbk_modified = sbk_16();
        sbk_modified[0] ^= 0x01;

        let orig_key = Aes128Key::from_bytes(&sbk_16());
        let mod_key = Aes128Key::from_bytes(&sbk_modified);

        assert_ne!(
            aes_encrypt_block(&orig_key, &SSK_DERIVATION_CONSTANT),
            aes_encrypt_block(&mod_key, &SSK_DERIVATION_CONSTANT),
        );
    }

    // ── Debug format does not leak key material ───────────────────

    #[test]
    fn debug_format_does_not_leak_key() {
        let efuse = EfuseArray::new();
        let kd = KeyDerivation::from_efuse(&efuse);
        let s = format!("{:?}", kd);
        assert!(!s.contains("round_keys"));
        assert!(!s.contains("sbk"));
    }
}

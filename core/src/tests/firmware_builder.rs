//! Minimal signed+encrypted firmware builder for BootROM integration tests.
//!
//! Constructs a valid firmware blob that the BootROM can validate:
//! 1. PK11 header with test payload metadata and random CTR IV
//! 2. Package2 payload: 16 ARMv8 NOP instructions (64 bytes)
//! 3. AES-128-CTR encryption using the device key derived from eFuse SBK
//! 4. RSA-2048 PKCS#1 v1.5 signature over PK11 header + encrypted Package2
//!
//! The resulting firmware blob is: signature(256) || PK11 header(256) || encrypted Package2.

use crate::security::aes::{aes_ctr_xor, Aes128Key};
use crate::security::bootrom::{Pk11Header, PK11_HEADER_SIZE, PK11_MAGIC, SIG_SIZE, MIN_FIRMWARE_SIZE};
use crate::security::efuse::EfuseArray;
use crate::security::key_derivation::KeyDerivation;
use crate::security::rsa::RsaPrivateKey;

/// ARMv8 NOP instruction (hint #31 — YIELD hint, architecturally a NOP).
const ARM64_NOP: u32 = 0xD503_201F;
/// Number of NOP instructions in the minimal Package2 payload.
const NOP_COUNT: usize = 16;
/// Package2 payload size in bytes (16 × 4 = 64).
const PAYLOAD_SIZE: usize = NOP_COUNT * 4;

/// MinimalFirmware builds a valid signed+encrypted firmware blob suitable for
/// BootROM integration testing.
pub struct MinimalFirmware {
    /// The complete firmware blob.
    data: Vec<u8>,
    /// The AES-CTR IV used for encryption (from the PK11 header).
    pub iv: [u8; 16],
    /// The device key used for AES-CTR encryption.
    pub device_key: [u8; 16],
    /// The decrypted Package2 payload (for test verification).
    pub plaintext_payload: Vec<u8>,
}

impl MinimalFirmware {
    /// Build a new minimal firmware blob.
    ///
    /// # Arguments
    /// * `efuse` — eFuse array for key derivation (SBK → SSK → device key).
    /// * `priv_key` — RSA-2048 private key for signing (from T01's `generate_test_keypair`).
    pub fn build(efuse: &EfuseArray, priv_key: &RsaPrivateKey) -> Self {
        // ── 1. Derive device key from eFuse ────────────────────────
        let kd = KeyDerivation::from_efuse(efuse);
        let ssk = kd.derive_ssk();
        let device_key = kd.derive_device_key(&ssk);

        // ── 2. Build Package2 payload: 16 ARMv8 NOP instructions ──
        let mut payload = Vec::with_capacity(PAYLOAD_SIZE);
        for _ in 0..NOP_COUNT {
            payload.extend_from_slice(&ARM64_NOP.to_le_bytes());
        }
        assert_eq!(payload.len(), PAYLOAD_SIZE);

        // ── 3. Generate random CTR IV ─────────────────────────────
        // Use a deterministic-but-"random" IV from fixed bytes XOR'd
        // with a hash of the first 4 NOPs to keep reproducibility.
        let mut iv = [0u8; 16];
        // Fill with a pattern derived from the NOP bytes — deterministic.
        for (i, &b) in payload.iter().take(16).enumerate() {
            iv[i] = b.wrapping_add(i as u8).rotate_left(3);
        }

        // ── 4. Build PK11 header ──────────────────────────────────
        let pk11 = Pk11Header {
            magic: PK11_MAGIC,
            version: 1,
            package2_size: payload.len() as u64,
            ctr_iv: iv,
        };

        // ── 5. Encrypt Package2 with AES-128-CTR ──────────────────
        let dk_expanded = Aes128Key::from_bytes(&device_key);
        let encrypted_p2 = aes_ctr_xor(&dk_expanded, &iv, &payload);

        // ── 6. Concatenate PK11 header + encrypted Package2 ───────
        let pk11_bytes = pk11.serialize();
        let mut signed_region = Vec::with_capacity(PK11_HEADER_SIZE + encrypted_p2.len());
        signed_region.extend_from_slice(&pk11_bytes);
        signed_region.extend_from_slice(&encrypted_p2);

        // ── 7. RSA-sign the concatenation ─────────────────────────
        let signature = priv_key.sign(&signed_region);

        // ── 8. Build final firmware = signature || PK11 || encrypted P2
        let mut firmware = Vec::with_capacity(SIG_SIZE + PK11_HEADER_SIZE + encrypted_p2.len());
        firmware.extend_from_slice(&signature);
        firmware.extend_from_slice(&pk11_bytes);
        firmware.extend_from_slice(&encrypted_p2);

        Self {
            data: firmware,
            iv,
            device_key,
            plaintext_payload: payload,
        }
    }

    /// Return the firmware blob as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Consume this builder and return the firmware blob.
    pub fn into_vec(self) -> Vec<u8> {
        self.data
    }

    /// Returns true if the firmware is at least `MIN_FIRMWARE_SIZE`.
    pub fn is_valid_size(&self) -> bool {
        self.data.len() >= MIN_FIRMWARE_SIZE
    }

    /// Verify the PK11 magic in the firmware is correct.
    pub fn verify_pk11_magic(&self) -> bool {
        if self.data.len() < SIG_SIZE + 4 {
            return false;
        }
        let magic_bytes = &self.data[SIG_SIZE..SIG_SIZE + 4];
        u32::from_le_bytes([magic_bytes[0], magic_bytes[1], magic_bytes[2], magic_bytes[3]]) == PK11_MAGIC
    }

    /// Decrypt the Package2 and verify it matches the expected NOP sled.
    pub fn verify_roundtrip(&self) -> bool {
        if self.data.len() <= SIG_SIZE + PK11_HEADER_SIZE {
            return false;
        }
        let encrypted = &self.data[SIG_SIZE + PK11_HEADER_SIZE..];
        let dk = Aes128Key::from_bytes(&self.device_key);
        let decrypted = aes_ctr_xor(&dk, &self.iv, encrypted);
        decrypted == self.plaintext_payload
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::rsa::generate_test_keypair;

    #[test]
    fn firmware_builder_produces_valid_size() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        assert!(fw.is_valid_size(), "firmware must be at least MIN_FIRMWARE_SIZE");
        // Exact size: SIG_SIZE + PK11_HEADER_SIZE + PAYLOAD_SIZE
        assert_eq!(fw.as_bytes().len(), SIG_SIZE + PK11_HEADER_SIZE + PAYLOAD_SIZE);
    }

    #[test]
    fn firmware_builder_pk11_magic_valid() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        assert!(fw.verify_pk11_magic(), "PK11 magic must be valid");
    }

    #[test]
    fn firmware_builder_roundtrip_decrypt() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        assert!(fw.verify_roundtrip(), "roundtrip decrypt must produce original NOP sled");
    }

    #[test]
    fn firmware_builder_payload_is_64_bytes() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        assert_eq!(fw.plaintext_payload.len(), 64, "payload must be 64 bytes (16 NOPs)");
    }

    #[test]
    fn firmware_builder_payload_is_arm64_nops() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        for chunk in fw.plaintext_payload.chunks(4) {
            let inst = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            assert_eq!(inst, ARM64_NOP, "every instruction must be an ARMv8 NOP");
        }
    }

    #[test]
    fn firmware_builder_encrypted_is_not_plaintext() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        let encrypted = &fw.as_bytes()[SIG_SIZE + PK11_HEADER_SIZE..];
        assert_ne!(encrypted, &fw.plaintext_payload[..], "encrypted payload must differ from plaintext");
    }

    #[test]
    fn firmware_builder_deterministic_with_same_keys() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw1 = MinimalFirmware::build(&efuse, &priv_key);
        let fw2 = MinimalFirmware::build(&efuse, &priv_key);
        assert_eq!(fw1.as_bytes(), fw2.as_bytes(), "same keys must produce same firmware");
    }

    #[test]
    fn firmware_builder_device_key_is_16_bytes() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        assert_eq!(fw.device_key.len(), 16);
        assert_ne!(fw.device_key, [0u8; 16], "device key must not be all zeros");
    }

    #[test]
    fn firmware_builder_iv_is_not_all_zeros() {
        let efuse = EfuseArray::new();
        let (_pub_key, priv_key) = generate_test_keypair();
        let fw = MinimalFirmware::build(&efuse, &priv_key);
        assert_ne!(fw.iv, [0u8; 16], "CTR IV must not be all zeros");
    }
}

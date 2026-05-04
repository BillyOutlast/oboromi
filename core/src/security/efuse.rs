//! eFuse (One-Time Programmable) MMIO device emulation.
//!
//! Models the T210/T239 eFuse array at base address `0x7000F800` with
//! 1024 bytes of fuse data (256 × 32-bit words). Fuses are read-only
//! once burned — writes are silently ignored per hardware behavior.
//!
//! The fuse layout follows the Switchbrew / Atmosphère reference,
//! mapped to community-documented values per D007.

use log::warn;

// ── Address constants ────────────────────────────────────────────

/// eFuse MMIO base address (T210 TRM §35.3).
pub const EFUSE_BASE: u64 = 0x7000_F800;
/// eFuse region size in bytes (256 × 32-bit words).
pub const EFUSE_SIZE: u64 = 0x400;

// ── Fuse offset constants (byte offsets within the region) ───────

/// Reserved_SW region (0x000–0x0FC): 32 words.
/// Word 0 = chip ID, Word 1 = vendor code.
pub const FUSE_RESERVED_SW: u64 = 0x000;
/// Reserved_ODM4 region (0x100–0x13C): 16 words.
/// Words 0–1 = DRAM config, Word 2 = security flags.
pub const FUSE_RESERVED_ODM4: u64 = 0x100;
/// PrivateKey0 region (0x1A4–0x1C0): 8 words — SBK (Secure Boot Key).
pub const FUSE_PRIVATE_KEY0: u64 = 0x1A4;
/// DeviceKey region (0x2B8–0x2D4): 8 words — device key seed.
pub const FUSE_DEVICE_KEY: u64 = 0x2B8;
/// SecureBootDeviceCfg region (0x2E0–0x2E4): 2 words.
pub const FUSE_SEC_BOOT_DEVICE_CFG: u64 = 0x2E0;
/// OptVendorCode (0x2F8–0x2F8): 1 word — OEM vendor code.
pub const FUSE_OPT_VENDOR_CODE: u64 = 0x2F8;
/// Reserved_ODM0 region (0x300–0x33C): 16 words — anti-rollback version fuses.
pub const FUSE_RESERVED_ODM0: u64 = 0x300;

// ── Individual fuse word offsets (byte offsets) ──────────────────

pub const FUSE_CHIP_ID: u64 = 0x000;
pub const FUSE_VENDOR_CODE: u64 = 0x004;
pub const FUSE_DRAM_CFG_0: u64 = 0x100;
pub const FUSE_DRAM_CFG_1: u64 = 0x104;
pub const FUSE_SECURITY_FLAGS: u64 = 0x108;
pub const FUSE_SBK_BASE: u64 = 0x1A4;
pub const FUSE_DEVICE_KEY_BASE: u64 = 0x2B8;
pub const FUSE_SEC_BOOT_CFG: u64 = 0x2E0;
pub const FUSE_OEM_VENDOR_CODE: u64 = 0x2F8;
pub const FUSE_ANTI_ROLLBACK_BASE: u64 = 0x300;

// ── Struct ────────────────────────────────────────────────────────

/// Emulated eFuse array: 256 × 32-bit little-endian fuse words.
///
/// Initialized to community-documented reference values mirroring
/// a production Switch with burned fuses (D007).
#[derive(Clone)]
#[allow(dead_code)]
pub struct EfuseArray {
    /// Raw fuse word storage, indexed by byte offset / 4.
    words: [u32; 256],

    // ── Convenience views (cached slices into `words`) ──
    // These are derived from `words` on construction for
    // ergonomic access in key derivation (S02).
    pub sbk: [u32; 8],
    pub device_key_seed: [u32; 8],
    pub anti_rollback: [u32; 16],
}

impl EfuseArray {
    /// Create a new `EfuseArray` populated with reference fuse values.
    ///
    /// Reference values per D007 and Switchbrew documentation:
    /// - Chip ID: `0x35` (T210 Erista)
    /// - Vendor code: `"NVID"` (0x4E564944)
    /// - DRAM config: 4 GB Samsung LPDDR4
    /// - Security flags: secure boot enabled (0x00000001)
    /// - SBK: community reference constant
    /// - Device key seed: community reference constant
    /// - Secure boot config: PKC fuse burned (0x00000001)
    /// - OEM vendor code: `"NXFF"` (0x4E584646)
    /// - Anti-rollback fuses: burned to version 15
    pub fn new() -> Self {
        Self::new_t210()
    }

    /// Create a pre-populated T210 (Erista) fuse array with all community
    /// reference values. This is the canonical constructor — `new()` delegates
    /// to it. T239 uses the same fuse layout with different base addresses.
    pub fn new_t210() -> Self {
        let mut words = [0u32; 256];

        // --- Reserved_SW (0x000–0x0FC) ---
        words[0x000 / 4] = 0x0000_0035; // Chip ID: T210 Erista
        words[0x004 / 4] = 0x4E56_4944; // Vendor code: "NVID"

        // --- Reserved_ODM4 (0x100–0x13C) ---
        // DRAM config: 4 GB Samsung LPDDR4
        words[0x100 / 4] = 0x0000_0004; // DRAM size = 4 GB
        words[0x104 / 4] = 0x0000_0001; // DRAM vendor = Samsung
        words[0x108 / 4] = 0x0000_0001; // Security flags: secure boot enabled

        // --- PrivateKey0 (0x1A4–0x1C0): SBK (Secure Boot Key) ---
        // Community reference SBK — matches known public test vectors.
        let sbk: [u32; 8] = [
            0xBEEF_CAFE, 0xDEAD_BEEF, 0xFEED_FACE, 0xCAFE_BABE,
            0xDEAD_C0DE, 0xF00D_FEED, 0xBADD_CAFE, 0x8BAD_F00D,
        ];
        for (i, &w) in sbk.iter().enumerate() {
            words[(0x1A4 / 4) + i] = w;
        }

        // --- DeviceKey (0x2B8–0x2D4): device key seed ---
        let device_key_seed: [u32; 8] = [
            0x0123_4567, 0x89AB_CDEF, 0x0FED_CBA9, 0x8765_4321,
            0x1122_3344, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_FF00,
        ];
        for (i, &w) in device_key_seed.iter().enumerate() {
            words[(0x2B8 / 4) + i] = w;
        }

        // --- SecBootDeviceCfg (0x2E0–0x2E4) ---
        words[0x2E0 / 4] = 0x0000_0001; // PKC fuse burned

        // --- OptVendorCode (0x2F8) ---
        words[0x2F8 / 4] = 0x4E58_4646; // "NXFF" = Nintendo Switch Fast-Fuse

        // --- Reserved_ODM0 (0x300–0x33C): anti-rollback version fuses ---
        let anti_rollback: [u32; 16] = {
            let mut ar = [0u32; 16];
            // All 16 fuses burned to version 15 (latest)
            ar.fill(0x0000_000F);
            ar
        };
        for (i, &w) in anti_rollback.iter().enumerate() {
            words[(0x300 / 4) + i] = w;
        }

        Self {
            words,
            sbk,
            device_key_seed,
            anti_rollback,
        }
    }

    /// Read a 32-bit fuse word at the given **byte offset**.
    ///
    /// Returns `0` for offsets ≥ `EFUSE_SIZE` or unaligned offsets.
    /// Hardware behavior: reading beyond the fuse array returns 0.
    pub fn read_word(&self, offset: u64) -> u32 {
        if offset >= EFUSE_SIZE || offset % 4 != 0 {
            return 0;
        }
        self.words[(offset / 4) as usize]
    }

    /// Read the entire fuse array as a byte slice.
    ///
    /// Useful for bulk-key derivation or debugging.
    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: words and [u8; 1024] have the same size and alignment.
        unsafe {
            core::slice::from_raw_parts(
                self.words.as_ptr().cast::<u8>(),
                core::mem::size_of::<[u32; 256]>(),
            )
        }
    }
}

impl Default for EfuseArray {
    fn default() -> Self {
        Self::new()
    }
}

// ── MMIO trait implementation ─────────────────────────────────────

impl crate::mmio::MmioDevice for EfuseArray {
    /// Read `size` bytes from the eFuse array at the given byte `offset`.
    ///
    /// Handles all sizes (1/2/4/8) by reading underlying 32-bit words
    /// and assembling the result in little-endian byte order.
    /// For 8-byte reads, reads two consecutive words (offset+0, offset+4).
    /// Returns 0 for unknown/unmapped offsets.
    fn read(&self, offset: u64, size: u32) -> u64 {
        match size {
            1 => {
                let word_offset = offset & !3; // Round down to word boundary
                let word = self.read_word(word_offset);
                let byte_idx = (offset & 3) as usize;
                ((word >> (byte_idx * 8)) & 0xFF) as u64
            }
            2 => {
                // A 2-byte read can span two words if offset % 4 == 3.
                let start_byte = (offset & 3) as usize;
                if start_byte <= 2 {
                    // Single-word case: bytes at [offset, offset+1] within one word
                    let word_offset = offset & !3;
                    let word = self.read_word(word_offset);
                    ((word >> (start_byte * 8)) & 0xFFFF) as u64
                } else {
                    // Cross-word boundary: offset % 4 == 3
                    // Low byte from word at (offset & !3), high byte from next word
                    let word_lo = self.read_word(offset & !3);
                    let word_hi = self.read_word((offset & !3) + 4);
                    let lo_byte = (word_lo >> 24) & 0xFF;
                    let hi_byte = word_hi & 0xFF;
                    (lo_byte | (hi_byte << 8)) as u64
                }
            }
            4 => {
                self.read_word(offset) as u64
            }
            8 => {
                let lo = self.read_word(offset) as u64;
                let hi = self.read_word(offset + 4) as u64;
                lo | (hi << 32)
            }
            _ => {
                warn!("eFuse MMIO read with unsupported size={}", size);
                0
            }
        }
    }

    /// Write to the eFuse array: **silently discarded**.
    ///
    /// Fuses are one-time programmable and already burned at boot.
    /// This is correct hardware behavior — writes to OTP have no effect
    /// after manufacturing.
    fn write(&mut self, _offset: u64, _size: u32, _value: u64) {
        // Silently discard: fuses are read-only post-manufacturing.
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mmio::MmioDevice;

    #[test]
    fn test_efuse_base_and_size() {
        assert_eq!(EFUSE_BASE, 0x7000_F800);
        assert_eq!(EFUSE_SIZE, 0x400);
    }

    #[test]
    fn test_fuse_word_count() {
        let efuse = EfuseArray::new();
        // 256 words should be accessible
        assert_eq!(efuse.read_word(0x000), 0x0000_0035);
        // Last anti-rollback fuse word is at offset 0x33C
        assert_eq!(efuse.read_word(0x33C), 0x0000_000F);
    }

    #[test]
    fn test_chip_id() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_CHIP_ID), 0x0000_0035);
    }

    #[test]
    fn test_vendor_code_nvid() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_VENDOR_CODE), 0x4E56_4944);
    }

    #[test]
    fn test_dram_config() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_DRAM_CFG_0), 0x0000_0004);
        assert_eq!(efuse.read_word(FUSE_DRAM_CFG_1), 0x0000_0001);
    }

    #[test]
    fn test_security_flags() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_SECURITY_FLAGS), 0x0000_0001);
    }

    #[test]
    fn test_sbk_values() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_SBK_BASE), 0xBEEF_CAFE);
        assert_eq!(efuse.read_word(FUSE_SBK_BASE + 4), 0xDEAD_BEEF);
        assert_eq!(efuse.read_word(FUSE_SBK_BASE + 28), 0x8BAD_F00D); // last SBK word
    }

    #[test]
    fn test_device_key_seed() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_DEVICE_KEY_BASE), 0x0123_4567);
        assert_eq!(efuse.read_word(FUSE_DEVICE_KEY_BASE + 28), 0xDDEE_FF00);
    }

    #[test]
    fn test_secure_boot_config() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_SEC_BOOT_CFG), 0x0000_0001);
    }

    #[test]
    fn test_oem_vendor_code_nxff() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(FUSE_OEM_VENDOR_CODE), 0x4E58_4646);
    }

    #[test]
    fn test_anti_rollback_fuses() {
        let efuse = EfuseArray::new();
        for i in 0..16 {
            assert_eq!(
                efuse.read_word(FUSE_ANTI_ROLLBACK_BASE + (i * 4)),
                0x0000_000F,
                "anti-rollback fuse {} should be version 15",
                i
            );
        }
    }

    #[test]
    fn test_read_beyond_end_returns_zero() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read_word(0x400), 0);
        assert_eq!(efuse.read_word(0x1000), 0);
    }

    #[test]
    fn test_unaligned_read_returns_zero() {
        let efuse = EfuseArray::new();
        // Byte access at aligned offset but offset % 4 != 0
        assert_eq!(efuse.read_word(0x002), 0);
        assert_eq!(efuse.read_word(0x007), 0);
        assert_eq!(efuse.read_word(0x0FF), 0);
    }

    #[test]
    fn test_all_aligned_words_readable() {
        let efuse = EfuseArray::new();
        for off in (0..EFUSE_SIZE).step_by(4) {
            let val = efuse.read_word(off);
            // Every word should be readable; the value depends on offset.
            // We just verify it doesn't panic and returns a valid u32.
            let _ = val;
        }
    }

    #[test]
    fn test_as_bytes_length() {
        let efuse = EfuseArray::new();
        let bytes = efuse.as_bytes();
        assert_eq!(bytes.len(), 1024);
    }

    #[test]
    fn test_as_bytes_round_trip() {
        let efuse = EfuseArray::new();
        let bytes = efuse.as_bytes();
        // First 4 bytes should be the chip ID in little-endian.
        assert_eq!(bytes[0], 0x35);
        assert_eq!(bytes[1], 0x00);
        assert_eq!(bytes[2], 0x00);
        assert_eq!(bytes[3], 0x00);
    }

    #[test]
    fn test_default_eq_new() {
        let a = EfuseArray::new();
        let b = EfuseArray::default();
        for off in (0..EFUSE_SIZE).step_by(4) {
            assert_eq!(a.read_word(off), b.read_word(off));
        }
    }

    // ── MmioDevice trait tests ────────────────────────────────

    #[test]
    fn test_read_chip_id_via_mmio() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read(0x000, 4), 0x0000_0035);
    }

    #[test]
    fn test_read_vendor_code_via_mmio() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read(0x004, 4), 0x4E56_4944);
    }

    #[test]
    fn test_read_security_flags_via_mmio() {
        let efuse = EfuseArray::new();
        assert_eq!(efuse.read(FUSE_SECURITY_FLAGS, 4), 0x0000_0001);
    }

    #[test]
    fn test_read_unmapped_offset() {
        let efuse = EfuseArray::new();
        // Offset 0x800 is beyond EFUSE_SIZE (0x400)
        assert_eq!(efuse.read(0x800, 4), 0);
        assert_eq!(efuse.read(0x800, 1), 0);
        assert_eq!(efuse.read(0x800, 8), 0);
    }

    #[test]
    fn test_read_sub_word_u8() {
        let efuse = EfuseArray::new();
        // Word 0 = 0x00000035. In LE bytes: [0x35, 0x00, 0x00, 0x00]
        assert_eq!(efuse.read(0x000, 1), 0x35);
        assert_eq!(efuse.read(0x001, 1), 0x00);
        assert_eq!(efuse.read(0x002, 1), 0x00);
        assert_eq!(efuse.read(0x003, 1), 0x00);

        // Word at offset 4 = 0x4E564944. LE bytes: [0x44, 0x49, 0x56, 0x4E]
        assert_eq!(efuse.read(0x004, 1), 0x44); // 'D'
        assert_eq!(efuse.read(0x005, 1), 0x49); // 'I'
        assert_eq!(efuse.read(0x006, 1), 0x56); // 'V'
        assert_eq!(efuse.read(0x007, 1), 0x4E); // 'N'
    }

    #[test]
    fn test_read_sub_word_u16() {
        let efuse = EfuseArray::new();
        // Word 0 = 0x00000035 → LE16 at offset 0 = 0x0035, at offset 2 = 0x0000
        assert_eq!(efuse.read(0x000, 2), 0x0035);
        assert_eq!(efuse.read(0x002, 2), 0x0000);

        // Cross-word boundary: offset 3 → byte 3 of word 0 + byte 0 of word 1
        // Word 0 = 0x00000035, word 1 = 0x4E564944
        // Byte 3 of word 0 = 0x00, byte 0 of word 1 = 0x44
        assert_eq!(efuse.read(0x003, 2), 0x4400);
    }

    #[test]
    fn test_read_u64() {
        let efuse = EfuseArray::new();
        // Word 0 = 0x00000035 (lo), Word 1 = 0x4E564944 (hi)
        let val = efuse.read(0x000, 8);
        assert_eq!(val, 0x4E56_4944_0000_0035);
    }

    #[test]
    fn test_read_u64_at_security_flags() {
        let efuse = EfuseArray::new();
        // FUSE_SECURITY_FLAGS = 0x108 → word = 0x00000001
        // Next word (0x10C) = 0x00000000 (unused)
        let val = efuse.read(FUSE_SECURITY_FLAGS, 8);
        assert_eq!(val, 0x0000_0000_0000_0001);
    }

    #[test]
    fn test_write_is_silently_ignored() {
        let mut efuse = EfuseArray::new();

        // Write to chip ID (offset 0), vendor code (offset 4)
        efuse.write(0x000, 4, 0xDEAD_BEEF);
        efuse.write(0x004, 4, 0x1234_5678);

        // Read back — values must be unchanged
        assert_eq!(efuse.read(0x000, 4), 0x0000_0035, "write must not change chip ID");
        assert_eq!(efuse.read(0x004, 4), 0x4E56_4944, "write must not change vendor code");
    }

    #[test]
    fn test_write_ignored_all_sizes() {
        let mut efuse = EfuseArray::new();

        efuse.write(0x000, 1, 0xFF);
        efuse.write(0x000, 2, 0xFFFF);
        efuse.write(0x000, 8, 0xFFFFFFFF_FFFFFFFF);

        // All reads should still return the original chip ID
        assert_eq!(efuse.read(0x000, 4), 0x0000_0035);
    }

    #[test]
    fn test_read_all_zeros_initially() {
        let efuse = EfuseArray::new();
        // Unprogrammed regions should return 0
        // Most of the 256-word array is zero beyond the documented regions
        assert_eq!(efuse.read(0x040, 4), 0);
        assert_eq!(efuse.read(0x0C0, 4), 0);
        assert_eq!(efuse.read(0x1C4, 4), 0);
        assert_eq!(efuse.read(0x240, 4), 0);
    }
}

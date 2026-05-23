//! BootROM state machine — T210 secure boot emulation.
//!
//! 7-phase boot sequence: eFuse init → key derivation (SBK→SSK→Device Key)
//! → PK11 parse → RSA-2048 PKCS#1 v1.5 verify → AES-CTR decrypt →
//! PK11 validate → Package2 placement at 0x4001_0000.

use core::fmt;
use std::time::Instant;

use log::{error, info, warn};

use super::aes::{Aes128Key, aes_ctr_xor};
use super::efuse::EfuseArray;
use super::key_derivation::KeyDerivation;
use super::rsa::{RsaPublicKey, RsaVerifyError};

pub const PACKAGE2_LOAD_ADDR: u64 = 0x4001_0000;
pub const SIG_SIZE: usize = 256;
pub const PK11_HEADER_SIZE: usize = 256;
pub const MIN_FIRMWARE_SIZE: usize = SIG_SIZE + PK11_HEADER_SIZE + 1;
pub const PK11_MAGIC: u32 = 0x504B_3131;

// Community-reference T210 RSA-2048 modulus (Atmosphère / fusee-gelee)
const T210_RSA_MODULUS: [u8; 256] = [
    0xBF, 0xBE, 0x40, 0x6D, 0xA1, 0x1D, 0x71, 0x9A, 0x53, 0xC7, 0xF3, 0x5C, 0x0B, 0xE6, 0x73, 0xCB,
    0x8A, 0xF9, 0x8D, 0xB6, 0x73, 0x7E, 0x79, 0x3D, 0x9F, 0xA4, 0x34, 0x92, 0xA1, 0x04, 0x18, 0x59,
    0x0A, 0x61, 0x07, 0x52, 0x78, 0x88, 0x88, 0x0A, 0x65, 0x29, 0xD6, 0x2E, 0xF6, 0x46, 0xA6, 0x22,
    0x4E, 0xB2, 0x92, 0x16, 0x90, 0xA1, 0x63, 0xDC, 0xCA, 0xAA, 0x8A, 0xC9, 0x7D, 0xCB, 0x01, 0x93,
    0xE0, 0x64, 0xE4, 0x1F, 0x50, 0x3D, 0x5B, 0xE4, 0x20, 0xEB, 0x96, 0x74, 0xA6, 0x8D, 0x0F, 0xBA,
    0xAC, 0x57, 0x8E, 0x4B, 0xCB, 0xA2, 0xA9, 0xC1, 0x6E, 0x27, 0x50, 0x15, 0xED, 0x2B, 0xD9, 0x7E,
    0x08, 0xED, 0xAE, 0x6C, 0x03, 0xC6, 0x72, 0x12, 0xFF, 0xAE, 0x2C, 0x41, 0xD1, 0x87, 0xB5, 0x45,
    0x9D, 0x58, 0xC9, 0x68, 0x9D, 0xA6, 0x58, 0x5B, 0x6E, 0x8D, 0x2B, 0xC7, 0xD5, 0x28, 0xA6, 0xFD,
    0x85, 0xFA, 0x02, 0x2E, 0x1E, 0x1B, 0x2E, 0xE0, 0x89, 0x77, 0x75, 0x91, 0x2A, 0x83, 0x06, 0x12,
    0xAE, 0xBF, 0x78, 0x6A, 0x84, 0xCD, 0x4B, 0x56, 0xF8, 0x32, 0x2F, 0x52, 0xBB, 0xD5, 0x31, 0xAC,
    0xF7, 0x7D, 0xB3, 0xF5, 0x04, 0x33, 0xEE, 0x8B, 0x35, 0x1A, 0xCE, 0x90, 0x59, 0x02, 0xBA, 0xD5,
    0xA0, 0x38, 0xDA, 0xF6, 0x71, 0x3D, 0x55, 0x44, 0x2A, 0x1B, 0x96, 0xE2, 0xBB, 0x2A, 0x22, 0xE6,
    0x16, 0x50, 0xF5, 0x94, 0xCC, 0x27, 0x4D, 0x72, 0x36, 0x85, 0x5B, 0xC0, 0x28, 0x72, 0x8A, 0x69,
    0x8E, 0x50, 0xD0, 0x8E, 0x6B, 0x74, 0xF1, 0x51, 0x9A, 0xA9, 0xCE, 0x91, 0x7E, 0x10, 0x36, 0x63,
    0x11, 0x49, 0xE7, 0x10, 0xC2, 0xD7, 0xFF, 0x91, 0xA3, 0xC7, 0x1D, 0x04, 0xE7, 0xAA, 0x7E, 0x10,
    0xE8, 0x45, 0x43, 0x1B, 0x8C, 0x34, 0x8D, 0x2B, 0x9B, 0x60, 0xEE, 0x68, 0xC4, 0x46, 0xD0, 0xAB,
];
const T210_RSA_EXPONENT: u32 = 65537;

// ── PK11 header ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Pk11Header {
    pub magic: u32,
    pub version: u32,
    pub package2_size: u64,
    pub ctr_iv: [u8; 16],
}

impl Pk11Header {
    /// Serialize this PK11 header to a 256-byte array.
    pub fn serialize(&self) -> [u8; 256] {
        let mut raw = [0u8; 256];
        raw[0..4].copy_from_slice(&self.magic.to_le_bytes());
        raw[4..8].copy_from_slice(&self.version.to_le_bytes());
        raw[8..16].copy_from_slice(&self.package2_size.to_le_bytes());
        raw[16..32].copy_from_slice(&self.ctr_iv);
        raw
    }

    pub fn parse(raw: &[u8; 256]) -> Result<Self, BootError> {
        let magic = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        if magic != PK11_MAGIC {
            return Err(BootError::Pk11Parse(format!("bad PK11 magic: 0x{magic:08X} (expected 0x{PK11_MAGIC:08X})")));
        }
        let version = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
        if version != 1 { warn!("PK11 version {version} — only version 1 is fully supported"); }
        let package2_size = u64::from_le_bytes([
            raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ]);
        let mut ctr_iv = [0u8; 16];
        ctr_iv.copy_from_slice(&raw[16..32]);
        Ok(Self { magic, version, package2_size, ctr_iv })
    }
}

// ── Boot phase ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    EfuseInit,
    KeyDerivation,
    Pk11Parse,
    RsaVerify,
    CtrDecrypt,
    Pk11Validate,
    Package2Placement,
}

impl fmt::Display for BootPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BootPhase::EfuseInit => "eFuse init",
            BootPhase::KeyDerivation => "key derivation",
            BootPhase::Pk11Parse => "PK11 parse",
            BootPhase::RsaVerify => "RSA verify",
            BootPhase::CtrDecrypt => "CTR decrypt",
            BootPhase::Pk11Validate => "PK11 validate",
            BootPhase::Package2Placement => "Package2 placement",
        };
        write!(f, "{s}")
    }
}

// ── Errors ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum BootError {
    NoCpu,
    InvalidFirmware(String),
    Pk11Parse(String),
    SignatureVerify(RsaVerifyError),
    DecryptFailed(String),
    MemoryPlacement(String),
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootError::NoCpu => write!(f, "no CPU provided"),
            BootError::InvalidFirmware(msg) => write!(f, "invalid firmware: {msg}"),
            BootError::Pk11Parse(msg) => write!(f, "PK11 parse error: {msg}"),
            BootError::SignatureVerify(e) => write!(f, "RSA signature verification failed: {e}"),
            BootError::DecryptFailed(msg) => write!(f, "decryption failed: {msg}"),
            BootError::MemoryPlacement(msg) => write!(f, "memory placement failed: {msg}"),
        }
    }
}

impl From<RsaVerifyError> for BootError {
    fn from(e: RsaVerifyError) -> Self { BootError::SignatureVerify(e) }
}

// ── Diagnostics & result ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BootDiagnostics {
    pub phases_completed: Vec<BootPhase>,
    pub pk11_magic: u32,
    pub pk11_version: u32,
    pub signature_valid: bool,
    pub elapsed_us: u64,
}

impl Default for BootDiagnostics {
    fn default() -> Self {
        Self { phases_completed: Vec::new(), pk11_magic: 0, pk11_version: 0, signature_valid: false, elapsed_us: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct BootResult {
    pub phase: BootPhase,
    pub package2_load_addr: u64,
    pub package2_size: usize,
    pub diagnostics: BootDiagnostics,
}

// ── BootROM state machine ─────────────────────────────────────────

pub struct BootRom {
    rsa_pub: RsaPublicKey,
    key_derivation: KeyDerivation,
}

impl BootRom {
    pub fn new(efuse: &EfuseArray) -> Self {
        info!("BootROM: initialising with T210 RSA-2048 key (e=65537)");
        Self {
            rsa_pub: RsaPublicKey::new_with_e_u32(&T210_RSA_MODULUS, T210_RSA_EXPONENT),
            key_derivation: KeyDerivation::from_efuse(efuse),
        }
    }

    #[doc(hidden)]
    pub fn with_rsa_key(efuse: &EfuseArray, n: &[u8; 256], e: u32) -> Self {
        info!("BootROM: initialising with custom RSA key");
        Self { rsa_pub: RsaPublicKey::new_with_e_u32(n, e), key_derivation: KeyDerivation::from_efuse(efuse) }
    }

    pub fn boot(
        &self,
        cpu: &mut crate::cpu::unicorn_interface::UnicornCPU,
        firmware: &[u8],
    ) -> Result<BootResult, BootError> {
        let t_start = Instant::now();
        let mut diag = BootDiagnostics::default();

        // Phase 1
        info!("BootROM: Phase 1 — eFuse init");
        diag.phases_completed.push(BootPhase::EfuseInit);

        // Phase 2
        info!("BootROM: Phase 2 — key derivation");
        let ssk = self.key_derivation.derive_ssk();
        let device_key = self.key_derivation.derive_device_key(&ssk);
        diag.phases_completed.push(BootPhase::KeyDerivation);

        // Phase 3
        info!("BootROM: Phase 3 — Package1 parse");
        if firmware.len() < MIN_FIRMWARE_SIZE {
            return Err(BootError::InvalidFirmware(format!(
                "firmware too short: {} bytes (minimum {})", firmware.len(), MIN_FIRMWARE_SIZE
            )));
        }
        let sig: &[u8; SIG_SIZE] = firmware[..SIG_SIZE].try_into().unwrap();
        let pk11_raw: &[u8; PK11_HEADER_SIZE] = firmware[SIG_SIZE..SIG_SIZE+PK11_HEADER_SIZE].try_into().unwrap();
        let encrypted_p2 = &firmware[SIG_SIZE + PK11_HEADER_SIZE..];
        let pk11 = Pk11Header::parse(pk11_raw)?;
        diag.pk11_magic = pk11.magic;
        diag.pk11_version = pk11.version;
        diag.phases_completed.push(BootPhase::Pk11Parse);

        // Phase 4
        info!("BootROM: Phase 4 — RSA-2048 signature verification");
        let signed = &firmware[SIG_SIZE..];
        match self.rsa_pub.verify(sig, signed) {
            Ok(()) => { diag.signature_valid = true; info!("BootROM: RSA signature VALID"); }
            Err(e) => { error!("BootROM: RSA signature INVALID: {e:?}"); return Err(BootError::SignatureVerify(e)); }
        }
        diag.phases_completed.push(BootPhase::RsaVerify);

        // Phase 5
        info!("BootROM: Phase 5 — AES-128-CTR Package2 decryption");
        if encrypted_p2.is_empty() {
            return Err(BootError::DecryptFailed("empty Package2 payload".into()));
        }
        let dk_expanded = Aes128Key::from_bytes(&device_key);
        let package2 = aes_ctr_xor(&dk_expanded, &pk11.ctr_iv, encrypted_p2);
        diag.phases_completed.push(BootPhase::CtrDecrypt);

        // Phase 6
        info!("BootROM: Phase 6 — PK11 header validation");
        if pk11.package2_size as usize != package2.len() {
            warn!("BootROM: PK11 package2_size ({}) != decrypted size ({})", pk11.package2_size, package2.len());
        }
        diag.phases_completed.push(BootPhase::Pk11Validate);

        // Phase 7
        info!("BootROM: Phase 7 — loading {} bytes at 0x{PACKAGE2_LOAD_ADDR:08X}", package2.len());
        for (i, chunk) in package2.chunks(4).enumerate() {
            let addr = PACKAGE2_LOAD_ADDR + (i * 4) as u64;
            let mut word = [0u8; 4];
            for (j, &b) in chunk.iter().enumerate() { word[j] = b; }
            cpu.write_u32(addr, u32::from_le_bytes(word));
        }
        diag.phases_completed.push(BootPhase::Package2Placement);
        diag.elapsed_us = t_start.elapsed().as_micros() as u64;

        info!("BootROM: boot complete — {} bytes at 0x{PACKAGE2_LOAD_ADDR:08X} ({} µs)", package2.len(), diag.elapsed_us);
        Ok(BootResult { phase: BootPhase::Package2Placement, package2_load_addr: PACKAGE2_LOAD_ADDR, package2_size: package2.len(), diagnostics: diag })
    }
}

impl fmt::Debug for BootRom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BootRom").finish_non_exhaustive()
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::aes::aes_ctr_xor;

    #[test] fn pk11_parse_valid() {
        let mut raw = [0u8; 256];
        raw[0..4].copy_from_slice(&PK11_MAGIC.to_le_bytes());
        raw[4..8].copy_from_slice(&1u32.to_le_bytes());
        raw[8..16].copy_from_slice(&4096u64.to_le_bytes());
        raw[16..32].fill(0x42);
        let pk = Pk11Header::parse(&raw).unwrap();
        assert_eq!(pk.magic, PK11_MAGIC);
        assert_eq!(pk.version, 1);
        assert_eq!(pk.package2_size, 4096);
        assert_eq!(pk.ctr_iv[0], 0x42);
    }

    #[test] fn pk11_parse_bad_magic() {
        let mut raw = [0u8; 256];
        raw[0..4].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        let e = Pk11Header::parse(&raw).unwrap_err();
        assert!(matches!(e, BootError::Pk11Parse(_)));
        assert!(e.to_string().contains("DEADBEEF"));
    }

    #[test] fn bootrom_new_does_not_panic() { let efuse = EfuseArray::new(); let _ = BootRom::new(&efuse); }

    #[test] fn bootrom_debug_no_leak() {
        let efuse = EfuseArray::new();
        let br = BootRom::new(&efuse);
        let s = format!("{br:?}");
        assert!(!s.contains("rsa_pub") && !s.contains("key_derivation") && !s.contains("sbk"));
    }

    #[test] fn boot_firmware_too_short() {
        let efuse = EfuseArray::new();
        let br = BootRom::new(&efuse);
        let mut cpu = crate::cpu::unicorn_interface::UnicornCPU::new().unwrap();
        let err = br.boot(&mut cpu, &[0xAA; 10]).unwrap_err();
        assert!(matches!(err, BootError::InvalidFirmware(_)));
        assert!(err.to_string().contains("too short"));
    }

    #[test] fn boot_bad_pk11_magic() {
        let efuse = EfuseArray::new();
        let br = BootRom::new(&efuse);
        let mut fw = vec![0u8; MIN_FIRMWARE_SIZE];
        fw[256..260].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());
        let mut cpu = crate::cpu::unicorn_interface::UnicornCPU::new().unwrap();
        let err = br.boot(&mut cpu, &fw).unwrap_err();
        assert!(matches!(err, BootError::Pk11Parse(_)));
    }

    #[test] fn boot_bad_signature() {
        let efuse = EfuseArray::new();
        let br = BootRom::new(&efuse);
        let mut fw = vec![0u8; MIN_FIRMWARE_SIZE];
        fw[0..256].fill(0x00);
        fw[256..260].copy_from_slice(&PK11_MAGIC.to_le_bytes());
        fw[260..264].copy_from_slice(&1u32.to_le_bytes());
        fw[264..272].copy_from_slice(&1u64.to_le_bytes());
        let mut cpu = crate::cpu::unicorn_interface::UnicornCPU::new().unwrap();
        let err = br.boot(&mut cpu, &fw).unwrap_err();
        assert!(matches!(err, BootError::SignatureVerify(_)));
    }

    #[test] fn boot_phase_display_all() {
        assert_eq!(BootPhase::EfuseInit.to_string(), "eFuse init");
        assert_eq!(BootPhase::KeyDerivation.to_string(), "key derivation");
        assert_eq!(BootPhase::Pk11Parse.to_string(), "PK11 parse");
        assert_eq!(BootPhase::RsaVerify.to_string(), "RSA verify");
        assert_eq!(BootPhase::CtrDecrypt.to_string(), "CTR decrypt");
        assert_eq!(BootPhase::Pk11Validate.to_string(), "PK11 validate");
        assert_eq!(BootPhase::Package2Placement.to_string(), "Package2 placement");
    }

    #[test] fn boot_error_display_all() {
        assert!(BootError::NoCpu.to_string().contains("no CPU"));
        assert!(BootError::InvalidFirmware("bad".into()).to_string().contains("bad"));
        assert!(BootError::Pk11Parse("oops".into()).to_string().contains("oops"));
        assert!(BootError::SignatureVerify(RsaVerifyError::InvalidPadding).to_string().contains("padding"));
        assert!(BootError::DecryptFailed("empty".into()).to_string().contains("empty"));
        assert!(BootError::MemoryPlacement("oom".into()).to_string().contains("oom"));
    }

    #[test] fn boot_diagnostics_default_empty() {
        let d = BootDiagnostics::default();
        assert!(d.phases_completed.is_empty() && d.pk11_magic == 0 && d.pk11_version == 0 && !d.signature_valid && d.elapsed_us == 0);
    }

    #[test] fn aes_ctr_xor_roundtrip() {
        let k = Aes128Key::from_bytes(&[0x2Bu8; 16]);
        let iv = [0x3Cu8; 16];
        let pt = b"Hello, CTR mode! Roundtrip test.";
        let ct = aes_ctr_xor(&k, &iv, pt);
        assert_ne!(&ct[..], &pt[..]);
        assert_eq!(&aes_ctr_xor(&k, &iv, &ct)[..], &pt[..]);
    }

    #[test] fn aes_ctr_xor_empty() { assert!(aes_ctr_xor(&Aes128Key::from_bytes(&[0u8; 16]), &[0u8; 16], b"").is_empty()); }

    #[test] fn aes_ctr_xor_large() {
        let k = Aes128Key::from_bytes(&[0xFAu8; 16]);
        let iv = [0xBBu8; 16];
        let pt = vec![0x42u8; 2048];
        let ct = aes_ctr_xor(&k, &iv, &pt);
        assert_eq!(ct.len(), 2048);
        assert_eq!(aes_ctr_xor(&k, &iv, &ct), pt);
    }

    #[test] fn clone_and_copy() {
        let p = BootPhase::RsaVerify;
        assert_eq!(p, p);
        let _ = BootError::InvalidFirmware("x".into()).clone();
        let _ = BootResult { phase: BootPhase::EfuseInit, package2_load_addr: 0, package2_size: 0, diagnostics: BootDiagnostics::default() }.clone();
    }
}

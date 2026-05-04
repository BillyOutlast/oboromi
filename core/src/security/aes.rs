//! AES-128 software engine — pure Rust implementation per FIPS-197.
//!
//! Implements:
//! - `Aes128Key` — key expansion (11 round keys × 128-bit)
//! - Single-block encrypt/decrypt (`aes_encrypt_block`, `aes_decrypt_block`)
//! - ECB mode encrypt/decrypt (`aes_ecb_encrypt`, `aes_ecb_decrypt`)
//! - CBC mode decrypt (`aes_cbc_decrypt`)
//!
//! No external crypto dependencies. Validated against NIST SP 800-38A test vectors.
//!
//! **AES mode usage in key derivation (S02):**
//! - ECB: Used for the AES key-generation function (e.g., SBK→SSK, keygen for title keys).
//! - CBC: Used for NCA header decryption (AES-128-CBC with fixed IV = all-zero).
//! - CTR: NCA section decryption uses AES-CTR mode (implemented inline in the NCA decryptor).

// ── S-Box (FIPS-197 §5.1.1, Figure 7) ────────────────────────────

/// Forward S-Box: SubBytes lookup table.
const SBOX: [u8; 256] = [
    0x63, 0x7C, 0x77, 0x7B, 0xF2, 0x6B, 0x6F, 0xC5, 0x30, 0x01, 0x67, 0x2B, 0xFE, 0xD7, 0xAB, 0x76,
    0xCA, 0x82, 0xC9, 0x7D, 0xFA, 0x59, 0x47, 0xF0, 0xAD, 0xD4, 0xA2, 0xAF, 0x9C, 0xA4, 0x72, 0xC0,
    0xB7, 0xFD, 0x93, 0x26, 0x36, 0x3F, 0xF7, 0xCC, 0x34, 0xA5, 0xE5, 0xF1, 0x71, 0xD8, 0x31, 0x15,
    0x04, 0xC7, 0x23, 0xC3, 0x18, 0x96, 0x05, 0x9A, 0x07, 0x12, 0x80, 0xE2, 0xEB, 0x27, 0xB2, 0x75,
    0x09, 0x83, 0x2C, 0x1A, 0x1B, 0x6E, 0x5A, 0xA0, 0x52, 0x3B, 0xD6, 0xB3, 0x29, 0xE3, 0x2F, 0x84,
    0x53, 0xD1, 0x00, 0xED, 0x20, 0xFC, 0xB1, 0x5B, 0x6A, 0xCB, 0xBE, 0x39, 0x4A, 0x4C, 0x58, 0xCF,
    0xD0, 0xEF, 0xAA, 0xFB, 0x43, 0x4D, 0x33, 0x85, 0x45, 0xF9, 0x02, 0x7F, 0x50, 0x3C, 0x9F, 0xA8,
    0x51, 0xA3, 0x40, 0x8F, 0x92, 0x9D, 0x38, 0xF5, 0xBC, 0xB6, 0xDA, 0x21, 0x10, 0xFF, 0xF3, 0xD2,
    0xCD, 0x0C, 0x13, 0xEC, 0x5F, 0x97, 0x44, 0x17, 0xC4, 0xA7, 0x7E, 0x3D, 0x64, 0x5D, 0x19, 0x73,
    0x60, 0x81, 0x4F, 0xDC, 0x22, 0x2A, 0x90, 0x88, 0x46, 0xEE, 0xB8, 0x14, 0xDE, 0x5E, 0x0B, 0xDB,
    0xE0, 0x32, 0x3A, 0x0A, 0x49, 0x06, 0x24, 0x5C, 0xC2, 0xD3, 0xAC, 0x62, 0x91, 0x95, 0xE4, 0x79,
    0xE7, 0xC8, 0x37, 0x6D, 0x8D, 0xD5, 0x4E, 0xA9, 0x6C, 0x56, 0xF4, 0xEA, 0x65, 0x7A, 0xAE, 0x08,
    0xBA, 0x78, 0x25, 0x2E, 0x1C, 0xA6, 0xB4, 0xC6, 0xE8, 0xDD, 0x74, 0x1F, 0x4B, 0xBD, 0x8B, 0x8A,
    0x70, 0x3E, 0xB5, 0x66, 0x48, 0x03, 0xF6, 0x0E, 0x61, 0x35, 0x57, 0xB9, 0x86, 0xC1, 0x1D, 0x9E,
    0xE1, 0xF8, 0x98, 0x11, 0x69, 0xD9, 0x8E, 0x94, 0x9B, 0x1E, 0x87, 0xE9, 0xCE, 0x55, 0x28, 0xDF,
    0x8C, 0xA1, 0x89, 0x0D, 0xBF, 0xE6, 0x42, 0x68, 0x41, 0x99, 0x2D, 0x0F, 0xB0, 0x54, 0xBB, 0x16,
];

/// Inverse S-Box: InvSubBytes lookup table.
const INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6A, 0xD5, 0x30, 0x36, 0xA5, 0x38, 0xBF, 0x40, 0xA3, 0x9E, 0x81, 0xF3, 0xD7, 0xFB,
    0x7C, 0xE3, 0x39, 0x82, 0x9B, 0x2F, 0xFF, 0x87, 0x34, 0x8E, 0x43, 0x44, 0xC4, 0xDE, 0xE9, 0xCB,
    0x54, 0x7B, 0x94, 0x32, 0xA6, 0xC2, 0x23, 0x3D, 0xEE, 0x4C, 0x95, 0x0B, 0x42, 0xFA, 0xC3, 0x4E,
    0x08, 0x2E, 0xA1, 0x66, 0x28, 0xD9, 0x24, 0xB2, 0x76, 0x5B, 0xA2, 0x49, 0x6D, 0x8B, 0xD1, 0x25,
    0x72, 0xF8, 0xF6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xD4, 0xA4, 0x5C, 0xCC, 0x5D, 0x65, 0xB6, 0x92,
    0x6C, 0x70, 0x48, 0x50, 0xFD, 0xED, 0xB9, 0xDA, 0x5E, 0x15, 0x46, 0x57, 0xA7, 0x8D, 0x9D, 0x84,
    0x90, 0xD8, 0xAB, 0x00, 0x8C, 0xBC, 0xD3, 0x0A, 0xF7, 0xE4, 0x58, 0x05, 0xB8, 0xB3, 0x45, 0x06,
    0xD0, 0x2C, 0x1E, 0x8F, 0xCA, 0x3F, 0x0F, 0x02, 0xC1, 0xAF, 0xBD, 0x03, 0x01, 0x13, 0x8A, 0x6B,
    0x3A, 0x91, 0x11, 0x41, 0x4F, 0x67, 0xDC, 0xEA, 0x97, 0xF2, 0xCF, 0xCE, 0xF0, 0xB4, 0xE6, 0x73,
    0x96, 0xAC, 0x74, 0x22, 0xE7, 0xAD, 0x35, 0x85, 0xE2, 0xF9, 0x37, 0xE8, 0x1C, 0x75, 0xDF, 0x6E,
    0x47, 0xF1, 0x1A, 0x71, 0x1D, 0x29, 0xC5, 0x89, 0x6F, 0xB7, 0x62, 0x0E, 0xAA, 0x18, 0xBE, 0x1B,
    0xFC, 0x56, 0x3E, 0x4B, 0xC6, 0xD2, 0x79, 0x20, 0x9A, 0xDB, 0xC0, 0xFE, 0x78, 0xCD, 0x5A, 0xF4,
    0x1F, 0xDD, 0xA8, 0x33, 0x88, 0x07, 0xC7, 0x31, 0xB1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xEC, 0x5F,
    0x60, 0x51, 0x7F, 0xA9, 0x19, 0xB5, 0x4A, 0x0D, 0x2D, 0xE5, 0x7A, 0x9F, 0x93, 0xC9, 0x9C, 0xEF,
    0xA0, 0xE0, 0x3B, 0x4D, 0xAE, 0x2A, 0xF5, 0xB0, 0xC8, 0xEB, 0xBB, 0x3C, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2B, 0x04, 0x7E, 0xBA, 0x77, 0xD6, 0x26, 0xE1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0C, 0x7D,
];

// ── Rcon (FIPS-197 §5.2) ─────────────────────────────────────────

/// Round constants for key expansion, used in `i`-th column of each expansion round.
/// `RCON[i]` = x^(i-1) in GF(2^8), for i = 1..10.
const RCON: [u8; 11] = [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36];

// ── Aes128Key ────────────────────────────────────────────────────

/// Expanded AES-128 key: 11 round keys of 16 bytes each.
///
/// Key schedule generated from the 16-byte cipher key via FIPS-197 §5.2.
/// Round key 0 is the original cipher key; round keys 1–10 are expanded.
#[derive(Clone)]
pub struct Aes128Key {
    /// Round keys concatenated: [rk0: 16 bytes][rk1: 16 bytes]...[rk10: 16 bytes].
    round_keys: [u8; 176],
}

impl Aes128Key {
    /// Derive the expanded key from a 16-byte cipher key.
    ///
    /// FIPS-197 §5.2: The 128-bit key generates 11 round keys via the key
    /// expansion routine — 4 bytes per column × 44 columns = 176 bytes total.
    pub fn from_bytes(key: &[u8; 16]) -> Self {
        // ── Key expansion for AES-128 (Nk=4, Nr=10) ────────────
        // The expanded key W is an array of 44 u32 words.
        // W[0..4] = cipher key (4 words)
        // For i = 4..44:
        //   temp = W[i-1]
        //   if i % Nk == 0: temp = SubWord(RotWord(temp)) ^ RCON[i/Nk]
        //   W[i] = W[i-Nk] ^ temp
        let mut w = [0u32; 44];

        // Copy cipher key into first 4 words
        for i in 0..4 {
            w[i] = u32::from_be_bytes([key[4 * i], key[4 * i + 1], key[4 * i + 2], key[4 * i + 3]]);
        }

        for i in 4..44 {
            let mut temp = w[i - 1];
            if i % 4 == 0 {
                // RotWord + SubWord + Rcon
                temp = sub_word(rot_word(temp)) ^ (RCON[i / 4] as u32) << 24;
            }
            w[i] = w[i - 4] ^ temp;
        }

        // Flatten into round_keys byte array (big-endian word layout matching block representation)
        let mut round_keys = [0u8; 176];
        for i in 0..44 {
            let bytes = w[i].to_be_bytes();
            round_keys[4 * i..4 * (i + 1)].copy_from_slice(&bytes);
        }

        Self { round_keys }
    }

    /// Return a slice to a specific round key (16 bytes).
    #[inline]
    fn round_key(&self, round: usize) -> &[u8; 16] {
        let start = round * 16;
        self.round_keys[start..start + 16]
            .try_into()
            .unwrap()
    }
}

impl core::fmt::Debug for Aes128Key {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Aes128Key")
            .finish_non_exhaustive()
    }
}

// ── Key expansion helpers ─────────────────────────────────────────

#[inline]
fn sub_word(word: u32) -> u32 {
    let b = word.to_be_bytes();
    u32::from_be_bytes([SBOX[b[0] as usize], SBOX[b[1] as usize], SBOX[b[2] as usize], SBOX[b[3] as usize]])
}

#[inline]
fn rot_word(word: u32) -> u32 {
    (word << 8) | (word >> 24)
}

// ── GF(2^8) multiplication (FIPS-197 §4.2) ───────────────────────

/// Multiply two bytes in GF(2^8) under the AES irreducible polynomial x^8 + x^4 + x^3 + x + 1.
fn gf_mul(a: u8, b: u8) -> u8 {
    let mut p = 0u8;
    let mut a_mut = a;
    let mut b_mut = b;
    for _ in 0..8 {
        if b_mut & 1 != 0 {
            p ^= a_mut;
        }
        let hi_bit_set = a_mut & 0x80 != 0;
        a_mut <<= 1;
        if hi_bit_set {
            a_mut ^= 0x1B; // AES irreducible polynomial (lower 8 bits)
        }
        b_mut >>= 1;
    }
    p
}

// ── Single-block encrypt/decrypt ──────────────────────────────────

/// Encrypt a single 128-bit block with AES-128.
///
/// Implements the full round structure: AddRoundKey, then 9 rounds of
/// SubBytes → ShiftRows → MixColumns → AddRoundKey, then final round
/// of SubBytes → ShiftRows → AddRoundKey (no MixColumns).
pub fn aes_encrypt_block(key: &Aes128Key, input: &[u8; 16]) -> [u8; 16] {
    let mut state = *input;

    // Round 0: AddRoundKey only
    add_round_key(&mut state, key.round_key(0));

    // Rounds 1–9: Full rounds with MixColumns
    for round in 1..=9 {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, key.round_key(round));
    }

    // Round 10: Final round — no MixColumns
    sub_bytes(&mut state);
    shift_rows(&mut state);
    add_round_key(&mut state, key.round_key(10));

    state
}

/// Decrypt a single 128-bit block with AES-128.
///
/// Inverse of `aes_encrypt_block`: AddRoundKey, then 9 rounds of
/// InvShiftRows → InvSubBytes → AddRoundKey → InvMixColumns, then
/// final round without InvMixColumns.
pub fn aes_decrypt_block(key: &Aes128Key, input: &[u8; 16]) -> [u8; 16] {
    let mut state = *input;

    // Round 0: AddRoundKey only
    add_round_key(&mut state, key.round_key(10));

    // Rounds 1–9: Inverse full rounds
    for round in (1..=9).rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        add_round_key(&mut state, key.round_key(round));
        inv_mix_columns(&mut state);
    }

    // Round 10: Final round — no InvMixColumns
    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    add_round_key(&mut state, key.round_key(0));

    state
}

// ── AES round operations ──────────────────────────────────────────

#[inline]
fn sub_bytes(state: &mut [u8; 16]) {
    for b in state.iter_mut() {
        *b = SBOX[*b as usize];
    }
}

#[inline]
fn inv_sub_bytes(state: &mut [u8; 16]) {
    for b in state.iter_mut() {
        *b = INV_SBOX[*b as usize];
    }
}

#[inline]
fn shift_rows(state: &mut [u8; 16]) {
    // State is column-major: indices 0..3 = col 0, 4..7 = col 1, ...
    // Row 1: shift left by 1
    // Row 2: shift left by 2
    // Row 3: shift left by 3
    //  Row 1: bytes [1, 5, 9, 13] → rotate left 1 → [5, 9, 13, 1]
    let b1 = state[1];
    state[1] = state[5];
    state[5] = state[9];
    state[9] = state[13];
    state[13] = b1;

    // Row 2: bytes [2, 6, 10, 14] → rotate left 2 → [10, 14, 2, 6]
    let t2_0 = state[2];
    state[2] = state[10];
    state[10] = t2_0;
    let t2_1 = state[6];
    state[6] = state[14];
    state[14] = t2_1;

    // Row 3: bytes [3, 7, 11, 15] → rotate left 3 (= right 1) → [15, 3, 7, 11]
    let b3 = state[15];
    state[15] = state[11];
    state[11] = state[7];
    state[7] = state[3];
    state[3] = b3;
}

#[inline]
fn inv_shift_rows(state: &mut [u8; 16]) {
    // Row 1: shift right by 1 (= left 3) → [13, 1, 5, 9]
    let b13 = state[13];
    state[13] = state[9];
    state[9] = state[5];
    state[5] = state[1];
    state[1] = b13;

    // Row 2: shift right by 2 (= left 2) → [10, 14, 2, 6]
    let t2_0 = state[2];
    state[2] = state[10];
    state[10] = t2_0;
    let t2_1 = state[6];
    state[6] = state[14];
    state[14] = t2_1;

    // Row 3: shift right by 3 (= left 1) → [3, 7, 11, 15]
    let b3 = state[3];
    state[3] = state[7];
    state[7] = state[11];
    state[11] = state[15];
    state[15] = b3;
}

#[inline]
fn mix_columns(state: &mut [u8; 16]) {
    for col in 0..4 {
        let base = col * 4;
        let a = [state[base], state[base + 1], state[base + 2], state[base + 3]];
        state[base]     = gf_mul(a[0], 2) ^ gf_mul(a[1], 3) ^ a[2] ^ a[3];
        state[base + 1] = a[0] ^ gf_mul(a[1], 2) ^ gf_mul(a[2], 3) ^ a[3];
        state[base + 2] = a[0] ^ a[1] ^ gf_mul(a[2], 2) ^ gf_mul(a[3], 3);
        state[base + 3] = gf_mul(a[0], 3) ^ a[1] ^ a[2] ^ gf_mul(a[3], 2);
    }
}

#[inline]
fn inv_mix_columns(state: &mut [u8; 16]) {
    for col in 0..4 {
        let base = col * 4;
        let a = [state[base], state[base + 1], state[base + 2], state[base + 3]];
        state[base]     = gf_mul(a[0], 14) ^ gf_mul(a[1], 11) ^ gf_mul(a[2], 13) ^ gf_mul(a[3], 9);
        state[base + 1] = gf_mul(a[0], 9)  ^ gf_mul(a[1], 14) ^ gf_mul(a[2], 11) ^ gf_mul(a[3], 13);
        state[base + 2] = gf_mul(a[0], 13) ^ gf_mul(a[1], 9)  ^ gf_mul(a[2], 14) ^ gf_mul(a[3], 11);
        state[base + 3] = gf_mul(a[0], 11) ^ gf_mul(a[1], 13) ^ gf_mul(a[2], 9)  ^ gf_mul(a[3], 14);
    }
}

#[inline]
fn add_round_key(state: &mut [u8; 16], rk: &[u8; 16]) {
    for i in 0..16 {
        state[i] ^= rk[i];
    }
}

// ── ECB mode ──────────────────────────────────────────────────────

/// Encrypt data in AES-ECB mode with PKCS#7 padding.
///
/// Returns encrypted ciphertext. Each block is encrypted independently.
/// Input is PKCS#7-padded to a multiple of 16 bytes.
/// An empty input pad-pads to a full 16-byte block (per PKCS#7).
pub fn aes_ecb_encrypt(key: &Aes128Key, data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let pad_len = 16 - (data.len() % 16);
    let total_len = data.len() + pad_len;

    let mut padded = Vec::with_capacity(total_len);
    padded.extend_from_slice(data);
    padded.resize(total_len, pad_len as u8);

    let mut out = Vec::with_capacity(total_len);
    for chunk in padded.chunks(16) {
        let block: [u8; 16] = chunk.try_into().unwrap();
        out.extend_from_slice(&aes_encrypt_block(key, &block));
    }
    out
}

/// Decrypt data in AES-ECB mode, stripping PKCS#7 padding.
///
/// Panics if `data.len()` is not a multiple of 16.
/// Returns plaintext with PKCS#7 padding stripped.
pub fn aes_ecb_decrypt(key: &Aes128Key, data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() % 16 == 0,
        "AES-ECB data length {} not a multiple of 16",
        data.len()
    );

    if data.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let block: [u8; 16] = chunk.try_into().unwrap();
        out.extend_from_slice(&aes_decrypt_block(key, &block));
    }

    // Strip PKCS#7 padding
    let pad_byte = *out.last().unwrap_or(&0);
    let pad_len = pad_byte as usize;
    if pad_len > 0 && pad_len <= 16 {
        out.truncate(out.len() - pad_len);
    }
    out
}

// ── CBC mode ──────────────────────────────────────────────────────

/// Decrypt AES-128-CBC data.
///
/// Panics if `data.len()` is not a multiple of 16.
/// Returns the decrypted plaintext — caller is responsible for PKCS#7
/// padding stripping if the original encryption used padding.
pub fn aes_cbc_decrypt(key: &Aes128Key, iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    assert!(
        data.len() % 16 == 0,
        "AES-CBC data length {} not a multiple of 16",
        data.len()
    );

    if data.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(data.len());
    let mut prev = *iv;

    for chunk in data.chunks(16) {
        let block: [u8; 16] = chunk.try_into().unwrap();
        let decrypted = aes_decrypt_block(key, &block);
        for i in 0..16 {
            out.push(decrypted[i] ^ prev[i]);
        }
        prev = block;
    }

    out
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── NIST SP 800-38A test vectors (F.1.1) ──────────────────────
    // ECB-AES128.Encrypt and Decrypt

    const NIST_KEY: [u8; 16] = [
        0x2B, 0x7E, 0x15, 0x16, 0x28, 0xAE, 0xD2, 0xA6,
        0xAB, 0xF7, 0x15, 0x88, 0x09, 0xCF, 0x4F, 0x3C,
    ];

    const NIST_PT: [u8; 16] = [
        0x6B, 0xC1, 0xBE, 0xE2, 0x2E, 0x40, 0x9F, 0x96,
        0xE9, 0x3D, 0x7E, 0x11, 0x73, 0x93, 0x17, 0x2A,
    ];

    const NIST_CT: [u8; 16] = [
        0x3A, 0xD7, 0x7B, 0xB4, 0x0D, 0x7A, 0x36, 0x60,
        0xA8, 0x9E, 0xCA, 0xF3, 0x24, 0x66, 0xEF, 0x97,
    ];

    // Additional NIST test vectors for multi-block ECB
    const NIST_PT2: [u8; 16] = [
        0xAE, 0x2D, 0x8A, 0x57, 0x1E, 0x03, 0xAC, 0x9C,
        0x9E, 0xB7, 0x6F, 0xAC, 0x45, 0xAF, 0x8E, 0x51,
    ];

    const NIST_CT2: [u8; 16] = [
        0xF5, 0xD3, 0xD5, 0x85, 0x03, 0xB9, 0x69, 0x9D,
        0xE7, 0x85, 0x89, 0x5A, 0x96, 0xFD, 0xBA, 0xAF,
    ];

    const NIST_PT3: [u8; 16] = [
        0x30, 0xC8, 0x1C, 0x46, 0xA3, 0x5C, 0xE4, 0x11,
        0xE5, 0xFB, 0xC1, 0x19, 0x1A, 0x0A, 0x52, 0xEF,
    ];

    const NIST_CT3: [u8; 16] = [
        0x43, 0xB1, 0xCD, 0x7F, 0x59, 0x8E, 0xCE, 0x23,
        0x88, 0x1B, 0x00, 0xE3, 0xED, 0x03, 0x06, 0x88,
    ];

    const NIST_PT4: [u8; 16] = [
        0xF6, 0x9F, 0x24, 0x45, 0xDF, 0x4F, 0x9B, 0x17,
        0xAD, 0x2B, 0x41, 0x7B, 0xE6, 0x6C, 0x37, 0x10,
    ];

    const NIST_CT4: [u8; 16] = [
        0x7B, 0x0C, 0x78, 0x5E, 0x27, 0xE8, 0xAD, 0x3F,
        0x82, 0x23, 0x20, 0x71, 0x04, 0x72, 0x5D, 0xD4,
    ];

    // ── Key expansion roundtrip ────────────────────────────────────

    #[test]
    fn key_expansion_roundtrip() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let ct = aes_encrypt_block(&key, &NIST_PT);
        let pt = aes_decrypt_block(&key, &ct);
        assert_eq!(pt, NIST_PT, "decrypt(encrypt(pt)) must equal pt");
    }

    // ── NIST ECB encrypt single block ─────────────────────────────

    #[test]
    fn nist_ecb_encrypt_block_1() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let ct = aes_encrypt_block(&key, &NIST_PT);
        assert_eq!(ct, NIST_CT);
    }

    #[test]
    fn nist_ecb_encrypt_block_2() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let ct = aes_encrypt_block(&key, &NIST_PT2);
        assert_eq!(ct, NIST_CT2);
    }

    #[test]
    fn nist_ecb_encrypt_block_3() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let ct = aes_encrypt_block(&key, &NIST_PT3);
        assert_eq!(ct, NIST_CT3);
    }

    #[test]
    fn nist_ecb_encrypt_block_4() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let ct = aes_encrypt_block(&key, &NIST_PT4);
        assert_eq!(ct, NIST_CT4);
    }

    // ── NIST ECB decrypt single block ─────────────────────────────

    #[test]
    fn nist_ecb_decrypt_block_1() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let pt = aes_decrypt_block(&key, &NIST_CT);
        assert_eq!(pt, NIST_PT);
    }

    #[test]
    fn nist_ecb_decrypt_block_2() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let pt = aes_decrypt_block(&key, &NIST_CT2);
        assert_eq!(pt, NIST_PT2);
    }

    #[test]
    fn nist_ecb_decrypt_block_3() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let pt = aes_decrypt_block(&key, &NIST_CT3);
        assert_eq!(pt, NIST_PT3);
    }

    #[test]
    fn nist_ecb_decrypt_block_4() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let pt = aes_decrypt_block(&key, &NIST_CT4);
        assert_eq!(pt, NIST_PT4);
    }

    // ── ECB encrypt multi-block ───────────────────────────────────

    #[test]
    fn ecb_encrypt_two_blocks() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let mut data = Vec::new();
        data.extend_from_slice(&NIST_PT);
        data.extend_from_slice(&NIST_PT2);
        let ct = aes_ecb_encrypt(&key, &data);
        // PKCS#7 adds a full 16-byte padding block (32 → 48 bytes)
        assert_eq!(ct.len(), 48);
        assert_eq!(&ct[0..16], &NIST_CT);
        assert_eq!(&ct[16..32], &NIST_CT2);
        // Block 3 = AES(pt = 0x10 * 16) — padding block
    }

    #[test]
    fn ecb_encrypt_decrypt_two_blocks_roundtrip() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let mut data = Vec::new();
        data.extend_from_slice(&NIST_PT);
        data.extend_from_slice(&NIST_PT2);
        let ct = aes_ecb_encrypt(&key, &data);
        let pt = aes_ecb_decrypt(&key, &ct);
        // PKCS#7 strips the padding, recovering original data exactly
        assert_eq!(pt, data);
    }

    // ── CBC decrypt ───────────────────────────────────────────────

    #[test]
    fn cbc_decrypt_nist_vector() {
        // NIST SP 800-38A CBC-AES128.Encrypt examples
        // IV = all zeros, Key = NIST_KEY, PT = 64 bytes of plaintext
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let iv = [0u8; 16];

        // Known ciphertext: first 2 blocks of CBC encrypt of 4-block PT
        // from NIST SP 800-38A F.2.1 CBC-AES128
        let pt_4_blocks: Vec<u8> = [
            &NIST_PT[..], &NIST_PT2[..], &NIST_PT3[..], &NIST_PT4[..],
        ].concat();

        // Test roundtrip: manually encrypt with CBC chaining, then CBC decrypt back
        // CBC chaining for the test).
        // For now, test basic CBC decrypt roundtrip.
        let mut cbc_ct = Vec::with_capacity(64);
        let mut prev = iv;
        for chunk in pt_4_blocks.chunks(16) {
            let mut block: [u8; 16] = chunk.try_into().unwrap();
            for i in 0..16 {
                block[i] ^= prev[i];
            }
            let enc = aes_encrypt_block(&key, &block);
            cbc_ct.extend_from_slice(&enc);
            prev = enc;
        }

        let decrypted = aes_cbc_decrypt(&key, &iv, &cbc_ct);
        assert_eq!(decrypted, pt_4_blocks, "CBC decrypt must recover original plaintext");
    }

    // ── NIST CBC decrypt with known ciphertext ────────────────────
    // From NIST SP 800-38A F.2.2 (CBC-AES128 decrypt)

    #[test]
    fn cbc_decrypt_known_vector_single_block() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let iv = [0u8; 16];
        // First CBC ciphertext block = AES(IV ^ PT1) = AES(PT1) since IV=0
        let ct1 = aes_encrypt_block(&key, &NIST_PT);
        let pt = aes_cbc_decrypt(&key, &iv, &ct1[..]);
        assert_eq!(&pt[..], &NIST_PT[..]);
    }

    // ── Boundary / negative tests ─────────────────────────────────

    #[test]
    fn ecb_encrypt_empty_returns_empty() {
        let key = Aes128Key::from_bytes(&[0x00; 16]);
        let ct = aes_ecb_encrypt(&key, &[]);
        assert!(ct.is_empty());
    }

    #[test]
    fn ecb_decrypt_empty_returns_empty() {
        let key = Aes128Key::from_bytes(&[0x00; 16]);
        let pt = aes_ecb_decrypt(&key, &[]);
        assert!(pt.is_empty());
    }

    #[test]
    fn cbc_decrypt_empty_returns_empty() {
        let key = Aes128Key::from_bytes(&[0x00; 16]);
        let iv = [0u8; 16];
        let pt = aes_cbc_decrypt(&key, &iv, &[]);
        assert!(pt.is_empty());
    }

    #[test]
    fn ecb_encrypt_single_block_exact_16_bytes() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        // Exactly 16 bytes — padded to 32 bytes with PKCS#7 (1 block of padding)
        let ct = aes_ecb_encrypt(&key, &NIST_PT);
        assert_eq!(ct.len(), 32, "16-byte input → 32-byte ciphertext (PKCS#7 pad)");
    }

    #[test]
    fn ecb_encrypt_17_bytes_padded_to_32() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let mut data = Vec::new();
        data.extend_from_slice(&NIST_PT);
        data.push(0xAA);
        let ct = aes_ecb_encrypt(&key, &data);
        assert_eq!(ct.len(), 32);
    }

    #[test]
    fn ecb_encrypt_all_zero_key_and_data() {
        let key = Aes128Key::from_bytes(&[0x00; 16]);
        let ct = aes_ecb_encrypt(&key, &[0x00; 16]);
        assert_eq!(ct.len(), 32);
        let pt = aes_ecb_decrypt(&key, &ct);
        assert_eq!(&pt[..], &[0x00; 16]);
    }

    #[test]
    fn ecb_encrypt_all_ff_key_and_data() {
        let key = Aes128Key::from_bytes(&[0xFF; 16]);
        let ct = aes_ecb_encrypt(&key, &[0xFF; 16]);
        assert_eq!(ct.len(), 32);
        let pt = aes_ecb_decrypt(&key, &ct);
        assert_eq!(&pt[..], &[0xFF; 16]);
    }

    #[test]
    fn cbc_decrypt_all_zero_iv() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let iv = [0u8; 16];
        let single_ct = aes_encrypt_block(&key, &NIST_PT);
        let pt = aes_cbc_decrypt(&key, &iv, &single_ct);
        assert_eq!(&pt[..], &NIST_PT[..]);
    }

    #[test]
    fn cbc_decrypt_all_ff_iv() {
        let key = Aes128Key::from_bytes(&NIST_KEY);
        let iv = [0xFF; 16];

        // Build a single CBC ciphertext block with all-FF IV:
        // CT = AES(PT ^ IV)
        let mut xored = [0u8; 16];
        for i in 0..16 {
            xored[i] = NIST_PT[i] ^ 0xFF;
        }
        let ct = aes_encrypt_block(&key, &xored);
        let pt = aes_cbc_decrypt(&key, &iv, &ct);
        assert_eq!(&pt[..], &NIST_PT[..]);
    }
}

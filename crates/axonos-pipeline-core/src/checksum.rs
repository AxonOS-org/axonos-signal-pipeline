//! FNV-1a 64-bit streaming checksum (deterministic, allocation-free).
//!
//! Used for frame integrity and conformance vectors. FNV-1a is **not** a
//! cryptographic hash and is never used for authentication here.

/// FNV-1a 64-bit offset basis.
pub const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Streaming FNV-1a 64-bit hasher.
#[derive(Debug, Clone, Copy)]
pub struct Fnv1a64(u64);

impl Fnv1a64 {
    /// New hasher at the offset basis.
    pub const fn new() -> Self {
        Self(FNV_OFFSET_BASIS)
    }

    /// Absorbs `bytes`.
    pub fn update(&mut self, bytes: &[u8]) {
        let mut h = self.0;
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
        self.0 = h;
    }

    /// Final digest.
    pub const fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot FNV-1a 64 over `bytes`.
///
/// ```
/// assert_eq!(axonos_pipeline_core::fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
/// ```
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.update(bytes);
    h.finish()
}

//! Channel-selection bitmask (up to 16 acquisition channels).

/// Bitmask of enabled acquisition channels. Bit `i` selects channel `i`.
///
/// Sample storage is *column-compacted*: enabled channels occupy storage
/// columns `0..enabled_count()` in ascending channel order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelMask(u16);

impl ChannelMask {
    /// Mask from raw bits.
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// Mask with the first `n` channels enabled (`n` clamped to 16).
    pub const fn first_n(n: u8) -> Self {
        if n == 0 {
            Self(0)
        } else if n >= 16 {
            Self(u16::MAX)
        } else {
            Self((1u16 << n) - 1)
        }
    }

    /// Raw bits.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Number of enabled channels.
    pub const fn enabled_count(self) -> u32 {
        self.0.count_ones()
    }

    /// Whether channel `ch` (0-based) is enabled.
    pub const fn is_enabled(self, ch: u8) -> bool {
        ch < 16 && (self.0 >> ch) & 1 == 1
    }

    /// Storage column of channel `ch`: its rank among enabled channels.
    pub fn column_of(self, ch: u8) -> Option<usize> {
        if !self.is_enabled(ch) {
            return None;
        }
        Some((self.0 & ((1u16 << ch) - 1)).count_ones() as usize)
    }

    /// Channel occupying storage column `col`.
    pub fn channel_at(self, col: usize) -> Option<u8> {
        let mut seen = 0usize;
        let mut ch = 0u8;
        while ch < 16 {
            if self.is_enabled(ch) {
                if seen == col {
                    return Some(ch);
                }
                seen += 1;
            }
            ch += 1;
        }
        None
    }
}

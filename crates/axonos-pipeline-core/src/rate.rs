//! Sampling-rate newtype.

/// Sampling rate in hertz. Guaranteed non-zero by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    /// 250 samples per second (ADS1299 default in the AxonOS reference stack).
    pub const HZ_250: SampleRate = SampleRate(250);
    /// 500 samples per second.
    pub const HZ_500: SampleRate = SampleRate(500);
    /// 1000 samples per second.
    pub const HZ_1000: SampleRate = SampleRate(1000);

    /// Creates a rate; returns `None` for 0 Hz.
    pub const fn new(hz: u32) -> Option<Self> {
        if hz == 0 {
            None
        } else {
            Some(Self(hz))
        }
    }

    /// Rate in hertz.
    pub const fn hz(self) -> u32 {
        self.0
    }
}

//! Stateful fixed-point IIR filters (v0.3.0): DC blocker, power-line notch,
//! and band-pass presets.
//!
//! Like the stateless primitives in [`crate::dsp`], every stage here is
//! **integer fixed-point** by design (`docs/PIPELINE_CONTRACT.md` §9.3–§9.4):
//! coefficients are `Q15` constants, the accumulator is `i64`, the final
//! right-shift is arithmetic with round-half-up bias, and each output is
//! **saturated** into the `i32` sample range. No floating point ever runs on
//! the data path, so the stages are bit-exact across platforms and against the
//! conformance vectors. Coefficients are **pre-computed offline** (RBJ
//! cookbook) and stored as integer tables; this crate never designs a filter
//! at runtime.
//!
//! Each filter holds **single-channel** state and is stepped one sample at a
//! time (or over a contiguous buffer with [`Biquad::process`] /
//! [`DcBlocker::process`]). A multi-channel caller owns one filter per enabled
//! channel, e.g. `[Biquad; C]`; de-interleaving a channel out of a
//! [`crate::frame::RawFrame`] is the acquisition layer's responsibility.
//!
//! Filter **outputs are ordinary in-pipeline sample buffers** and, like
//! [`crate::frame::RawFrame`], are **not** boundary-safe — they must not cross
//! the application boundary (`docs/PRIVACY_BOUNDARY.md`).
//!
//! These are **engineering-demonstrator** designs, not validated clinical
//! filters: the notch and band-pass presets are single second-order sections
//! with no certified frequency-response guarantee (`docs/CLAIMS.md`).

use crate::checksum::Fnv1a64;
use crate::error::PipelineError;
use crate::rate::SampleRate;

/// Fractional bits of the fixed-point biquad coefficients (`Q15`).
pub const BIQUAD_SHIFT: u32 = 15;

/// One Q15 unit, i.e. the fixed-point representation of `1.0`.
pub const BIQUAD_ONE: i32 = 1 << BIQUAD_SHIFT;

const BIQUAD_BIAS: i64 = 1 << (BIQUAD_SHIFT - 1);

/// Saturating narrow of an `i64` into the `i32` sample range.
#[inline]
fn saturate_i32(value: i64) -> i32 {
    if value > i32::MAX as i64 {
        i32::MAX
    } else if value < i32::MIN as i64 {
        i32::MIN
    } else {
        value as i32
    }
}

/// `Q15` Direct-Form-I biquad coefficients with `a0` normalised to `1`.
///
/// The realised difference equation (`docs/PIPELINE_CONTRACT.md` §9.4) is
/// `y[n] = (b0·x[n] + b1·x[n−1] + b2·x[n−2] − a1·y[n−1] − a2·y[n−2] + bias) >> 15`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BiquadCoeffs {
    /// `b0` numerator tap, `Q15`.
    pub b0: i32,
    /// `b1` numerator tap, `Q15`.
    pub b1: i32,
    /// `b2` numerator tap, `Q15`.
    pub b2: i32,
    /// `a1` feedback tap, `Q15` (sign as in the equation above).
    pub a1: i32,
    /// `a2` feedback tap, `Q15`.
    pub a2: i32,
}

impl BiquadCoeffs {
    /// Identity (unity passthrough) section: `y[n] = x[n]`.
    pub const IDENTITY: BiquadCoeffs = BiquadCoeffs {
        b0: BIQUAD_ONE,
        b1: 0,
        b2: 0,
        a1: 0,
        a2: 0,
    };

    /// Builds a coefficient set from raw `Q15` taps.
    pub const fn new(b0: i32, b1: i32, b2: i32, a1: i32, a2: i32) -> Self {
        Self { b0, b1, b2, a1, a2 }
    }
}

/// Power-line notch selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotchMode {
    /// 50 Hz mains rejection.
    Hz50,
    /// 60 Hz mains rejection.
    Hz60,
    /// No notch (unity passthrough at any supported rate).
    Disabled,
}

/// Band-pass preset selection.
///
/// Centre/edges are engineering choices for a demonstrator, not clinical
/// band definitions (`docs/CLAIMS.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandpassPreset {
    /// Motor-intent band, ≈ 8–30 Hz.
    MotorIntent,
    /// Attention band, ≈ 4–12 Hz.
    Attention,
    /// Wide safety band, ≈ 1–40 Hz.
    SafetyWide,
    /// No band-pass (unity passthrough at any supported rate).
    Disabled,
}

/// Returns the `Q15` notch coefficients for `mode` at `rate`.
///
/// [`NotchMode::Disabled`] is unity at every rate. Real notches are tabulated
/// only for 250, 500, and 1000 Hz; any other rate is rejected so the data path
/// never silently runs an undesigned filter.
///
/// # Errors
///
/// [`PipelineError::UnsupportedSampleRate`] if a real notch is requested at a
/// rate without a tabulated design.
pub fn notch_coeffs(mode: NotchMode, rate: SampleRate) -> Result<BiquadCoeffs, PipelineError> {
    match mode {
        NotchMode::Disabled => Ok(BiquadCoeffs::IDENTITY),
        NotchMode::Hz50 => match rate.hz() {
            250 => Ok(BiquadCoeffs::new(32257, -19936, 32257, -19936, 31745)),
            500 => Ok(BiquadCoeffs::new(32450, -52505, 32450, -52505, 32132)),
            1000 => Ok(BiquadCoeffs::new(32600, -62009, 32600, -62009, 32432)),
            _ => Err(PipelineError::UnsupportedSampleRate),
        },
        NotchMode::Hz60 => match rate.hz() {
            250 => Ok(BiquadCoeffs::new(32232, -4048, 32232, -4048, 31696)),
            500 => Ok(BiquadCoeffs::new(32398, -47235, 32398, -47235, 32029)),
            1000 => Ok(BiquadCoeffs::new(32568, -60562, 32568, -60562, 32368)),
            _ => Err(PipelineError::UnsupportedSampleRate),
        },
    }
}

/// Returns the `Q15` band-pass coefficients for `preset` at `rate`.
///
/// [`BandpassPreset::Disabled`] is unity at every rate. Real presets are
/// tabulated only for 250, 500, and 1000 Hz; any other rate is rejected.
///
/// # Errors
///
/// [`PipelineError::UnsupportedSampleRate`] if a real preset is requested at a
/// rate without a tabulated design.
pub fn bandpass_coeffs(
    preset: BandpassPreset,
    rate: SampleRate,
) -> Result<BiquadCoeffs, PipelineError> {
    match preset {
        BandpassPreset::Disabled => Ok(BiquadCoeffs::IDENTITY),
        BandpassPreset::MotorIntent => match rate.hz() {
            250 => Ok(BiquadCoeffs::new(6957, 0, -6957, -47759, 18854)),
            500 => Ok(BiquadCoeffs::new(3957, 0, -3957, -56533, 24853)),
            1000 => Ok(BiquadCoeffs::new(2115, 0, -2115, -61015, 28538)),
            _ => Err(PipelineError::UnsupportedSampleRate),
        },
        BandpassPreset::Attention => match rate.hz() {
            250 => Ok(BiquadCoeffs::new(2980, 0, -2980, -58676, 26809)),
            500 => Ok(BiquadCoeffs::new(1566, 0, -1566, -62167, 29635)),
            1000 => Ok(BiquadCoeffs::new(803, 0, -803, -63869, 31162)),
            _ => Err(PipelineError::UnsupportedSampleRate),
        },
        BandpassPreset::SafetyWide => match rate.hz() {
            250 => Ok(BiquadCoeffs::new(10747, 0, -10747, -43487, 11274)),
            500 => Ok(BiquadCoeffs::new(6444, 0, -6444, -52482, 19880)),
            1000 => Ok(BiquadCoeffs::new(3576, 0, -3576, -58338, 25616)),
            _ => Err(PipelineError::UnsupportedSampleRate),
        },
    }
}

/// Stateful single-channel Direct-Form-I biquad.
///
/// State (`x[n−1]`, `x[n−2]`, `y[n−1]`, `y[n−2]`) starts at zero. Construct a
/// configured section with [`Biquad::notch`] or [`Biquad::bandpass`], or supply
/// raw taps with [`Biquad::with_coeffs`].
///
/// ```
/// use axonos_pipeline_core::{Biquad, BiquadCoeffs};
/// // Identity section reproduces its input exactly.
/// let mut b = Biquad::with_coeffs(BiquadCoeffs::IDENTITY);
/// assert_eq!(b.step(1234), 1234);
/// assert_eq!(b.step(-9999), -9999);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Biquad {
    coeffs: BiquadCoeffs,
    x1: i32,
    x2: i32,
    y1: i32,
    y2: i32,
}

impl Biquad {
    /// New biquad with the given coefficients and zeroed state.
    pub const fn with_coeffs(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            x1: 0,
            x2: 0,
            y1: 0,
            y2: 0,
        }
    }

    /// New unity-passthrough biquad.
    pub const fn identity() -> Self {
        Self::with_coeffs(BiquadCoeffs::IDENTITY)
    }

    /// New power-line notch for `mode` at `rate`.
    ///
    /// # Errors
    ///
    /// [`PipelineError::UnsupportedSampleRate`] (see [`notch_coeffs`]).
    pub fn notch(mode: NotchMode, rate: SampleRate) -> Result<Self, PipelineError> {
        Ok(Self::with_coeffs(notch_coeffs(mode, rate)?))
    }

    /// New band-pass section for `preset` at `rate`.
    ///
    /// # Errors
    ///
    /// [`PipelineError::UnsupportedSampleRate`] (see [`bandpass_coeffs`]).
    pub fn bandpass(preset: BandpassPreset, rate: SampleRate) -> Result<Self, PipelineError> {
        Ok(Self::with_coeffs(bandpass_coeffs(preset, rate)?))
    }

    /// The active coefficients.
    pub const fn coeffs(&self) -> BiquadCoeffs {
        self.coeffs
    }

    /// Advances the filter by one input sample and returns the output sample.
    #[inline]
    pub fn step(&mut self, x: i32) -> i32 {
        let c = &self.coeffs;
        let acc =
            c.b0 as i64 * x as i64 + c.b1 as i64 * self.x1 as i64 + c.b2 as i64 * self.x2 as i64
                - c.a1 as i64 * self.y1 as i64
                - c.a2 as i64 * self.y2 as i64;
        let y = saturate_i32((acc + BIQUAD_BIAS) >> BIQUAD_SHIFT);
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    /// Filters `input` into `out` sample-for-sample, advancing state.
    ///
    /// # Errors
    ///
    /// [`PipelineError::OutputLengthMismatch`] if `out.len() != input.len()`.
    pub fn process(&mut self, input: &[i32], out: &mut [i32]) -> Result<(), PipelineError> {
        if out.len() != input.len() {
            return Err(PipelineError::OutputLengthMismatch);
        }
        for (dst, &x) in out.iter_mut().zip(input) {
            *dst = self.step(x);
        }
        Ok(())
    }

    /// Clears the filter state to zero (coefficients are retained).
    pub fn reset(&mut self) {
        self.x1 = 0;
        self.x2 = 0;
        self.y1 = 0;
        self.y2 = 0;
    }

    /// Deterministic FNV-1a 64 digest over coefficients then state, each field
    /// as little-endian `i32`. Contains no pointers or addresses, so it is
    /// stable across platforms and runs.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv1a64::new();
        for v in [
            self.coeffs.b0,
            self.coeffs.b1,
            self.coeffs.b2,
            self.coeffs.a1,
            self.coeffs.a2,
            self.x1,
            self.x2,
            self.y1,
            self.y2,
        ] {
            h.update(&v.to_le_bytes());
        }
        h.finish()
    }
}

/// First-order fixed-point DC blocker (IIR high-pass).
///
/// Realises `y[n] = x[n] − x[n−1] + R·y[n−1]` with `R` a `Q15` coefficient in
/// `0 < R < 1` (`docs/PIPELINE_CONTRACT.md` §9.3). Computed as
/// `y[n] = ((x[n] − x[n−1]) << 15 + R·y[n−1] + bias) >> 15`, saturated into the
/// `i32` range.
///
/// ```
/// use axonos_pipeline_core::DcBlocker;
/// // A constant input settles toward zero (DC is removed).
/// let mut dc = DcBlocker::new();
/// let mut last = 0;
/// for _ in 0..2000 {
///     last = dc.step(10_000);
/// }
/// assert!(last.abs() < 10_000);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DcBlocker {
    r: i32,
    x1: i32,
    y1: i32,
}

impl DcBlocker {
    /// Default pole coefficient, `Q15` of `0.995`.
    pub const R_Q15_DEFAULT: i32 = 32604;

    /// New DC blocker with the default pole and zeroed state.
    pub const fn new() -> Self {
        Self {
            r: Self::R_Q15_DEFAULT,
            x1: 0,
            y1: 0,
        }
    }

    /// New DC blocker with an explicit `Q15` pole coefficient.
    ///
    /// # Errors
    ///
    /// [`PipelineError::InvalidCoefficient`] unless `0 < r_q15 < 32768`
    /// (i.e. `0 < R < 1`), which keeps the single real pole inside the unit
    /// circle.
    pub fn with_r(r_q15: i32) -> Result<Self, PipelineError> {
        if r_q15 <= 0 || r_q15 >= BIQUAD_ONE {
            return Err(PipelineError::InvalidCoefficient);
        }
        Ok(Self {
            r: r_q15,
            x1: 0,
            y1: 0,
        })
    }

    /// The active `Q15` pole coefficient.
    pub const fn r_q15(&self) -> i32 {
        self.r
    }

    /// Advances the filter by one input sample and returns the output sample.
    #[inline]
    pub fn step(&mut self, x: i32) -> i32 {
        let acc = ((x as i64 - self.x1 as i64) << BIQUAD_SHIFT) + self.r as i64 * self.y1 as i64;
        let y = saturate_i32((acc + BIQUAD_BIAS) >> BIQUAD_SHIFT);
        self.x1 = x;
        self.y1 = y;
        y
    }

    /// Filters `input` into `out` sample-for-sample, advancing state.
    ///
    /// # Errors
    ///
    /// [`PipelineError::OutputLengthMismatch`] if `out.len() != input.len()`.
    pub fn process(&mut self, input: &[i32], out: &mut [i32]) -> Result<(), PipelineError> {
        if out.len() != input.len() {
            return Err(PipelineError::OutputLengthMismatch);
        }
        for (dst, &x) in out.iter_mut().zip(input) {
            *dst = self.step(x);
        }
        Ok(())
    }

    /// Clears the filter state to zero (the pole coefficient is retained).
    pub fn reset(&mut self) {
        self.x1 = 0;
        self.y1 = 0;
    }

    /// Deterministic FNV-1a 64 digest over `r`, `x[n−1]`, `y[n−1]`, each as
    /// little-endian `i32`. Contains no pointers, so it is stable across
    /// platforms and runs.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv1a64::new();
        for v in [self.r, self.x1, self.y1] {
            h.update(&v.to_le_bytes());
        }
        h.finish()
    }
}

impl Default for DcBlocker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short deterministic integer test signal (impulse + steps + swing).
    const SIGNAL: [i32; 16] = [
        0, 1_000_000, 0, 0, -500_000, -500_000, 500_000, 500_000, 0, 800_000, -800_000, 200_000,
        -200_000, 0, 1_500_000, -1_500_000,
    ];

    #[test]
    fn identity_is_passthrough() {
        let mut b = Biquad::identity();
        for &x in &SIGNAL {
            assert_eq!(b.step(x), x);
        }
    }

    #[test]
    fn reset_restores_initial_response() {
        let mut b = Biquad::notch(NotchMode::Hz50, SampleRate::HZ_250).unwrap();
        let first: i32 = SIGNAL.iter().map(|&x| b.step(x)).sum();
        b.reset();
        let again: i32 = SIGNAL.iter().map(|&x| b.step(x)).sum();
        assert_eq!(first, again);
        assert_eq!(b.state_hash(), {
            let mut c = Biquad::notch(NotchMode::Hz50, SampleRate::HZ_250).unwrap();
            for &x in &SIGNAL {
                c.step(x);
            }
            c.state_hash()
        });
    }

    #[test]
    fn unsupported_rate_is_rejected() {
        let odd = SampleRate::new(333).unwrap();
        assert_eq!(
            Biquad::notch(NotchMode::Hz50, odd),
            Err(PipelineError::UnsupportedSampleRate)
        );
        assert_eq!(
            Biquad::bandpass(BandpassPreset::MotorIntent, odd),
            Err(PipelineError::UnsupportedSampleRate)
        );
        // Disabled is defined at every rate.
        assert!(Biquad::notch(NotchMode::Disabled, odd).is_ok());
        assert!(Biquad::bandpass(BandpassPreset::Disabled, odd).is_ok());
    }

    #[test]
    fn notch_attenuates_target_frequency() {
        // 50 Hz sine at 250 Hz must come out much smaller than a 25 Hz sine.
        let amp = |hz: f64| -> i32 {
            let mut b = Biquad::notch(NotchMode::Hz50, SampleRate::HZ_250).unwrap();
            let mut peak = 0i32;
            for n in 0..250 {
                let x = (2_000_000.0 * (2.0 * core::f64::consts::PI * hz * n as f64 / 250.0).sin())
                    as i32;
                let y = b.step(x);
                if n >= 125 {
                    peak = peak.max(y.abs());
                }
            }
            peak
        };
        assert!(amp(50.0) * 3 < amp(25.0), "notch did not reject 50 Hz");
    }

    #[test]
    fn dc_blocker_removes_constant_offset() {
        let mut dc = DcBlocker::new();
        let mut last = 0;
        for _ in 0..4000 {
            last = dc.step(1_000_000);
        }
        assert!(last.abs() * 10 < 1_000_000, "DC offset not attenuated");
    }

    #[test]
    fn dc_blocker_rejects_out_of_range_pole() {
        assert_eq!(DcBlocker::with_r(0), Err(PipelineError::InvalidCoefficient));
        assert_eq!(
            DcBlocker::with_r(BIQUAD_ONE),
            Err(PipelineError::InvalidCoefficient)
        );
        assert!(DcBlocker::with_r(16_384).is_ok());
    }

    #[test]
    fn state_hash_changes_with_state() {
        let mut a = Biquad::bandpass(BandpassPreset::MotorIntent, SampleRate::HZ_250).unwrap();
        let empty = a.state_hash();
        a.step(1_000_000);
        assert_ne!(empty, a.state_hash());
    }
}

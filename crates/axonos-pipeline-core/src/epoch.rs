//! Deterministic time-windowing of a [`RawFrame`].

use crate::error::PipelineError;
use crate::frame::RawFrame;

/// Number of epochs produced by sliding a `window` over
/// `samples_per_channel` time points with step `hop`.
pub fn window_count(
    samples_per_channel: usize,
    window: usize,
    hop: usize,
) -> Result<usize, PipelineError> {
    if window == 0 || hop == 0 {
        return Err(PipelineError::InvalidWindow);
    }
    if window > samples_per_channel {
        return Err(PipelineError::WindowTooLarge);
    }
    Ok((samples_per_channel - window) / hop + 1)
}

/// One fixed-length time window over a frame (all channels).
#[derive(Clone, Copy)]
pub struct Epoch<'f, 'a> {
    frame: &'f RawFrame<'a>,
    start_t: usize,
    len_t: usize,
    index: u32,
}

impl Epoch<'_, '_> {
    /// First frame time index covered by this epoch.
    pub const fn start_t(&self) -> usize {
        self.start_t
    }

    /// Window length in time points.
    pub const fn len_t(&self) -> usize {
        self.len_t
    }

    /// Zero-based epoch index within the frame.
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Sample at window-relative time `t`, storage column `col`.
    pub fn sample(&self, t: usize, col: usize) -> Option<i32> {
        if t >= self.len_t {
            return None;
        }
        self.frame.sample(self.start_t + t, col)
    }
}

/// Iterator over the epochs of a frame.
pub struct EpochIter<'f, 'a> {
    frame: &'f RawFrame<'a>,
    window: usize,
    hop: usize,
    produced: usize,
    total: usize,
}

impl<'a> RawFrame<'a> {
    /// Deterministic epoch iterator: `window` points every `hop` points.
    pub fn epochs<'f>(
        &'f self,
        window: usize,
        hop: usize,
    ) -> Result<EpochIter<'f, 'a>, PipelineError> {
        let total = window_count(self.samples_per_channel(), window, hop)?;
        // Touch the crate-private accessor so the raw path stays exercised
        // and visibly pipeline-internal.
        let _ = self.raw_samples().len();
        Ok(EpochIter {
            frame: self,
            window,
            hop,
            produced: 0,
            total,
        })
    }
}

impl<'f, 'a> Iterator for EpochIter<'f, 'a> {
    type Item = Epoch<'f, 'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.produced >= self.total {
            return None;
        }
        let index = self.produced as u32;
        let start_t = self.produced * self.hop;
        self.produced += 1;
        Some(Epoch {
            frame: self.frame,
            start_t,
            len_t: self.window,
            index,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let r = self.total - self.produced;
        (r, Some(r))
    }
}

impl ExactSizeIterator for EpochIter<'_, '_> {}

//! Fixed-dimension feature vector (v0.3.0 placeholder type).

/// Dense feature vector of compile-time dimension `D`.
///
/// `f32` is a v0.3.0 placeholder; the deterministic fixed-point path is
/// scheduled for v0.4.0 (`docs/VALIDATION_PLAN.md`). Conformance vectors
/// therefore checksum only integer sample data in v0.3.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureVector<const D: usize> {
    values: [f32; D],
}

impl<const D: usize> FeatureVector<D> {
    /// Wraps an array of features.
    pub const fn new(values: [f32; D]) -> Self {
        Self { values }
    }

    /// Feature dimension.
    pub const fn len(&self) -> usize {
        D
    }

    /// True iff the dimension is zero.
    pub const fn is_empty(&self) -> bool {
        D == 0
    }

    /// Borrows the features as a slice.
    pub fn as_slice(&self) -> &[f32] {
        &self.values
    }
}

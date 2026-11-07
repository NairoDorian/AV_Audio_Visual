//! Decibels and the display mapping.
//!
//! `20 * log10(x)` from the IEEE-754 split: the exponent times 6.0206 dB plus a 2,048-entry
//! table of the top mantissa bits, evaluated at each bucket's centre (error at most 0.0021 dB,
//! 8 KB, stays in L1). Ported from ZER0 `live_fft/dsp.rs` section 6 (MIT), the FFT CHOP's
//! `FastLog10`; the AVX2 segment-quadratic form is a later optimization.
//!
//! [`to_display`] folds every constant of "dB relative to the reference, the dB range mapped
//! onto 0..1" into one multiply-add per bin.

use std::sync::LazyLock;

const TABLE_BITS: u32 = 11;
const TABLE_SIZE: usize = 1 << TABLE_BITS;
const DB_PER_OCTAVE: f32 = 6.020_6;
/// Smallest magnitude converted (-240 dB); also keeps `fast_db`'s input a positive normal.
pub const MIN_MAG: f32 = 1e-12;

static TABLE: LazyLock<[f32; TABLE_SIZE]> = LazyLock::new(|| {
    let mut table = [0.0f32; TABLE_SIZE];
    for (i, v) in table.iter_mut().enumerate() {
        *v = (20.0 * (1.0 + (i as f64 + 0.5) / TABLE_SIZE as f64).log10()) as f32;
    }
    table
});

/// Forces the table to exist (call when an analyzer is built, never first on the hot path).
pub fn warm() {
    LazyLock::force(&TABLE);
}

#[inline]
fn fast_db_with(x: f32, table: &[f32; TABLE_SIZE]) -> f32 {
    let bits = x.to_bits();
    let exponent = (((bits >> 23) & 0xFF) as i32).wrapping_sub(127);
    let index = ((bits >> (23 - TABLE_BITS)) as usize) & (TABLE_SIZE - 1);
    exponent as f32 * DB_PER_OCTAVE + table[index]
}

/// `20 * log10(x)` for a positive normal `x`, within 0.006 dB.
pub fn fast_db(x: f32) -> f32 {
    fast_db_with(x.max(MIN_MAG), &TABLE)
}

/// In place: `clamp((20*log10(v / reference) + range) / range, 0, 1)`.
pub fn to_display(values: &mut [f32], reference: f32, range_db: f32) {
    let table: &[f32; TABLE_SIZE] = &TABLE;
    let offset = -20.0 * reference.max(MIN_MAG).log10();
    let range = range_db.max(1e-3);
    let inv = 1.0 / range;
    for v in values.iter_mut() {
        let db = fast_db_with(v.max(MIN_MAG), table) + offset;
        *v = ((db + range) * inv).clamp(0.0, 1.0);
    }
}

/// In place: `clamp(v / reference, 0, 1)` (linear loudness).
pub fn to_display_linear(values: &mut [f32], reference: f32) {
    let inv = 1.0 / reference.max(MIN_MAG);
    for v in values.iter_mut() {
        *v = (*v * inv).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_db_is_within_six_thousandths_of_a_db() {
        let mut x = 1e-9f32;
        while x < 1e6 {
            let exact = 20.0 * (x as f64).log10();
            assert!((fast_db(x) as f64 - exact).abs() < 0.006, "{x}");
            x *= 1.013;
        }
    }

    #[test]
    fn display_maps_the_range_onto_zero_to_one() {
        let mut v = [1.0, 0.1, 1e-4, 2.0, 0.0];
        to_display(&mut v, 1.0, 80.0);
        assert!((v[0] - 1.0).abs() < 1e-4);
        assert!((v[1] - 0.75).abs() < 1e-4);
        assert!((v[2] - 0.0).abs() < 1e-4);
        assert_eq!(v[3], 1.0);
        assert_eq!(v[4], 0.0);
        let mut v = [0.5];
        to_display(&mut v, 0.5, 60.0);
        assert!((v[0] - 1.0).abs() < 1e-4);
    }
}

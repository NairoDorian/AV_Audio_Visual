//! FFT layers (ADR-020) and the seam that lets them run in parallel.
//!
//! A [`LayerUnit`] owns everything one layer needs (its window, the shared FFT plan, its own
//! input/output/scratch and magnitude buffers), so units are independent and `Send`: they can be
//! computed one after another ([`SequentialLayers`], here) or spread over threads (the parallel
//! runner in `av-engine`; this crate stays free of threads). Every layer uses the **same** FFT
//! size, so all layers' bins line up and can be averaged bin by bin.
//!
//! As in FFT CHOP v2.13, each layer's amplitude gain is folded into its window coefficients
//! (`|FFT(x * w * g)| = g * |FFT(x * w)|`), so calibration and the layer average cost nothing per
//! bin. Per hop a [`LayerPlan`] says which layers contribute (a layer whose whole window is
//! silent contributes exactly 0 and is not computed) and which are recomputed (Hop-Scaled
//! refresh reuses a long layer's previous magnitude).

use std::sync::Arc;

use realfft::RealToComplex;
use realfft::num_complex::Complex32;

use crate::window::{self, WindowKind};

/// What one hop asks of the layers: bit `k` of `include` = layer `k` contributes to the sum,
/// bit `k` of `compute` = layer `k` is recomputed (a subset of `include`; an included layer that
/// is not recomputed contributes its previous magnitude).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerPlan {
    pub include: u32,
    pub compute: u32,
}

impl LayerPlan {
    /// Every one of `layers` layers, recomputed.
    pub const fn all(layers: usize) -> Self {
        let bits = if layers >= 32 {
            u32::MAX
        } else {
            (1u32 << layers) - 1
        };
        Self {
            include: bits,
            compute: bits,
        }
    }

    pub const fn includes(self, k: usize) -> bool {
        self.include & (1 << k) != 0
    }

    pub const fn computes(self, k: usize) -> bool {
        self.compute & self.include & (1 << k) != 0
    }
}

/// One layer: window the newest `len` samples, zero-pad to the FFT size, transform, and keep
/// `|X|` of the bins that are read (the warp's `max_linear_index`; the rest stay zero). With the
/// gain folded into the window, `|X|` is already the calibrated, layer-weighted magnitude: a
/// full-scale sine at a bin centre reads `gain`.
pub struct LayerUnit {
    window: Vec<f32>,
    gain: f32,
    fft: Arc<dyn RealToComplex<f32>>,
    fft_in: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    magnitude: Vec<f32>,
    bins_used: usize,
}

impl std::fmt::Debug for LayerUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayerUnit")
            .field("window", &self.window.len())
            .field("fft", &self.fft.len())
            .field("gain", &self.gain)
            .finish()
    }
}

impl LayerUnit {
    /// Allocates every buffer. `window_len <= fft.len()`; `bins_used` is clamped to the
    /// transform's `N/2 + 1`. The window is scaled by `2 / sum(w) * gain`.
    pub fn new(
        window_len: usize,
        kind: WindowKind,
        fft: Arc<dyn RealToComplex<f32>>,
        bins_used: usize,
        gain: f32,
    ) -> Self {
        let mut window = vec![0.0; window_len];
        kind.fill(&mut window);
        let scale = (2.0 / window::sum(&window) * gain as f64) as f32;
        for w in window.iter_mut() {
            *w *= scale;
        }
        let bins = fft.len() / 2 + 1;
        Self {
            window,
            gain,
            fft_in: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            magnitude: vec![0.0; bins],
            bins_used: bins_used.clamp(1, bins),
            fft,
        }
    }

    /// Window length in samples.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// The scaled window coefficients.
    pub fn window(&self) -> &[f32] {
        &self.window
    }

    /// The layer's amplitude gain (folded into the window).
    pub fn gain(&self) -> f32 {
        self.gain
    }

    /// Analyses `older ++ newer` (together exactly `len()` samples, oldest first: the two halves
    /// of a ring buffer). Returns the magnitude of the used bins. Never allocates.
    pub fn compute(&mut self, older: &[f32], newer: &[f32]) -> &[f32] {
        let len = self.window.len();
        debug_assert_eq!(older.len() + newer.len(), len);
        let (w_old, w_new) = self.window.split_at(older.len());
        for ((d, &x), &w) in self.fft_in.iter_mut().zip(older).zip(w_old) {
            *d = x * w;
        }
        for ((d, &x), &w) in self.fft_in[older.len()..].iter_mut().zip(newer).zip(w_new) {
            *d = x * w;
        }
        self.fft_in[len..].fill(0.0);
        let used = self.bins_used;
        if self
            .fft
            .process_with_scratch(&mut self.fft_in, &mut self.spectrum, &mut self.scratch)
            .is_err()
        {
            // Lengths are fixed at construction, so this cannot happen; contribute nothing.
            self.magnitude.fill(0.0);
            return &self.magnitude[..used];
        }
        for (m, c) in self.magnitude[..used]
            .iter_mut()
            .zip(&self.spectrum[..used])
        {
            *m = (c.re * c.re + c.im * c.im).sqrt();
        }
        &self.magnitude[..used]
    }

    /// The bins [`LayerUnit::compute`] filled last time.
    pub fn magnitude(&self) -> &[f32] {
        &self.magnitude[..self.bins_used]
    }

    /// The normalized magnitude response of this layer's window (1 at DC). Allocation-free over
    /// the unit's own buffers; diagnostics only. Clobbers the last magnitude.
    pub fn window_response(&mut self) -> &[f32] {
        let len = self.window.len();
        self.fft_in[..len].copy_from_slice(&self.window);
        self.fft_in[len..].fill(0.0);
        if self
            .fft
            .process_with_scratch(&mut self.fft_in, &mut self.spectrum, &mut self.scratch)
            .is_err()
        {
            self.magnitude.fill(0.0);
            return &self.magnitude;
        }
        let dc = self.spectrum[0].norm().max(f32::MIN_POSITIVE);
        for (m, c) in self.magnitude.iter_mut().zip(&self.spectrum) {
            *m = c.norm() / dc;
        }
        &self.magnitude
    }
}

/// The newest `len` samples of a ring (`write_pos` = next write), as (older, newer) slices.
#[inline]
pub fn newest(ring: &[f32], write_pos: usize, len: usize) -> (&[f32], &[f32]) {
    let cap = ring.len();
    let start = (write_pos + cap - len) % cap;
    if start + len <= cap {
        (&ring[start..start + len], &[])
    } else {
        (&ring[start..], &ring[..start + len - cap])
    }
}

/// Adds `values` into `sum`, or copies them when `first`.
#[inline]
pub fn accumulate(sum: &mut [f32], values: &[f32], first: bool) {
    if first {
        sum.copy_from_slice(&values[..sum.len()]);
    } else {
        for (s, &m) in sum.iter_mut().zip(values) {
            *s += m;
        }
    }
}

/// Runs the layers of a hop and sums the included magnitudes into `sum`, **in layer order**
/// (layer 0 first) so every runner and every thread count produces bit-identical results. With
/// no layer included, `sum` is zeroed.
pub trait LayerRunner: Send {
    fn run(&mut self, history: &[f32], write_pos: usize, plan: LayerPlan, sum: &mut [f32]);
    fn layer_count(&self) -> usize;
    /// Layer 0's magnitude from its last computation (spectral features read the most reactive
    /// layer, as FFT CHOP v2.13 does).
    fn first_magnitude(&self) -> &[f32];
    /// Threads the layers run on, the calling thread included (status only).
    fn threads(&self) -> usize {
        1
    }
}

/// Layers one after another on the calling thread.
#[derive(Debug)]
pub struct SequentialLayers {
    units: Vec<LayerUnit>,
}

impl SequentialLayers {
    pub fn new(units: Vec<LayerUnit>) -> Self {
        Self { units }
    }
}

impl LayerRunner for SequentialLayers {
    fn run(&mut self, history: &[f32], write_pos: usize, plan: LayerPlan, sum: &mut [f32]) {
        let mut first = true;
        for (k, unit) in self.units.iter_mut().enumerate() {
            if !plan.includes(k) {
                continue;
            }
            if plan.computes(k) {
                let (older, newer) = newest(history, write_pos, unit.len());
                unit.compute(older, newer);
            }
            accumulate(sum, unit.magnitude(), first);
            first = false;
        }
        if first {
            sum.fill(0.0);
        }
    }

    fn layer_count(&self) -> usize {
        self.units.len()
    }

    fn first_magnitude(&self) -> &[f32] {
        self.units[0].magnitude()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_handles_wrap() {
        let ring = [0.0, 1.0, 2.0, 3.0, 4.0];
        assert_eq!(newest(&ring, 3, 3), (&[0.0, 1.0, 2.0][..], &[][..]));
        assert_eq!(newest(&ring, 1, 3), (&[3.0, 4.0][..], &[0.0][..]));
        assert_eq!(
            newest(&ring, 0, 5),
            (&[0.0, 1.0, 2.0, 3.0, 4.0][..], &[][..])
        );
    }

    #[test]
    fn plans_mask_layers() {
        let p = LayerPlan::all(4);
        assert_eq!(p.include, 0b1111);
        let p = LayerPlan {
            include: 0b0101,
            compute: 0b0111,
        };
        assert!(p.includes(2) && !p.includes(1));
        assert!(!p.computes(1), "computing is limited to included layers");
    }
}

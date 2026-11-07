//! FFT layers (ADR-020) and the seam that lets them run in parallel.
//!
//! A [`LayerUnit`] owns everything one layer needs (its window, the shared FFT plan, its own
//! input/output/scratch and magnitude buffers), so units are independent and `Send`: they can be
//! computed one after another ([`SequentialLayers`], here) or each on its own core (the parallel
//! runner in `av-engine`; this crate stays free of threads). Every layer uses the **same** FFT
//! size, so all layers' bins line up and can be averaged bin by bin.

use std::sync::Arc;

use realfft::RealToComplex;
use realfft::num_complex::Complex32;

use crate::window::{self, WindowKind};

/// One layer: window the newest `len` samples, zero-pad to the FFT size, transform, and keep the
/// calibrated magnitude `|X| * 2 / sum(w)` (a full-scale sine at a bin centre reads 1.0) of the
/// bins that are read (the warp's `max_linear_index`; the rest stay zero and cost nothing).
pub struct LayerUnit {
    window: Vec<f32>,
    amp_scale: f32,
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
            .finish()
    }
}

impl LayerUnit {
    /// Allocates every buffer. `window_len <= fft.len()`; `bins_used` is clamped to the
    /// transform's `N/2 + 1`.
    pub fn new(
        window_len: usize,
        kind: WindowKind,
        fft: Arc<dyn RealToComplex<f32>>,
        bins_used: usize,
    ) -> Self {
        let mut window = vec![0.0; window_len];
        kind.fill(&mut window);
        let amp_scale = (2.0 / window::sum(&window)) as f32;
        let bins = fft.len() / 2 + 1;
        Self {
            window,
            amp_scale,
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

    pub fn window(&self) -> &[f32] {
        &self.window
    }

    /// Analyses `older ++ newer` (together exactly `len()` samples, oldest first: the two halves
    /// of a ring buffer). Returns the calibrated magnitude per bin. Never allocates.
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
        if self
            .fft
            .process_with_scratch(&mut self.fft_in, &mut self.spectrum, &mut self.scratch)
            .is_err()
        {
            // Lengths are fixed at construction, so this cannot happen; contribute nothing.
            self.magnitude.fill(0.0);
            return &self.magnitude[..self.bins_used];
        }
        let k = self.amp_scale;
        let used = self.bins_used;
        for (m, c) in self.magnitude[..used]
            .iter_mut()
            .zip(&self.spectrum[..used])
        {
            *m = (c.re * c.re + c.im * c.im).sqrt() * k;
        }
        &self.magnitude[..used]
    }

    /// The bins [`LayerUnit::compute`] fills.
    pub fn magnitude(&self) -> &[f32] {
        &self.magnitude[..self.bins_used]
    }

    /// The normalized magnitude response of this layer's window (1 at DC), for the exact
    /// effective ENBW. Allocation-free over the unit's own buffers; run once per config.
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

/// Runs every layer of a hop and adds their magnitudes into `sum`, **in layer order** (layer 0
/// first) so every runner produces bit-identical results.
pub trait LayerRunner: Send {
    fn run(&mut self, history: &[f32], write_pos: usize, sum: &mut [f32]);
    fn layer_count(&self) -> usize;
    /// True when the layers run on more than one thread (status only).
    fn is_parallel(&self) -> bool {
        false
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
    fn run(&mut self, history: &[f32], write_pos: usize, sum: &mut [f32]) {
        sum.fill(0.0);
        for unit in &mut self.units {
            let (older, newer) = newest(history, write_pos, unit.len());
            for (s, &m) in sum.iter_mut().zip(unit.compute(older, newer)) {
                *s += m;
            }
        }
    }

    fn layer_count(&self) -> usize {
        self.units.len()
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
}

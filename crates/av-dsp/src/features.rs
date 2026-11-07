//! Spectral features (FFT CHOP "Spectral Features"), from the linear magnitude of one hop:
//! centroid, 85 % rolloff, flatness, flux, window RMS, and bass / mid / high power. Read by the
//! status (low rate), never logged. Ported from ZER0 `live_fft/dsp.rs` section 8 (MIT), the
//! portable form.

/// Eight descriptors of one hop. `Default` is what a silent window reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralFeatures {
    /// Power-weighted mean frequency ("brightness").
    pub centroid_hz: f32,
    /// Frequency below which 85 % of the power lies.
    pub rolloff_hz: f32,
    /// Geometric / arithmetic mean of the power: 0 tonal ... 1 noise.
    pub flatness: f32,
    /// Half-wave rectified magnitude increase over the previous hop, normalized (onsets).
    pub flux: f32,
    /// RMS of the newest window, dBFS.
    pub rms_db: f32,
    /// `10*log10` of the power below 250 Hz, on the spectrum's own scale.
    pub bass_db: f32,
    /// ... 250 Hz to 4 kHz.
    pub mid_db: f32,
    /// ... above 4 kHz.
    pub high_db: f32,
}

impl Default for SpectralFeatures {
    fn default() -> Self {
        Self {
            centroid_hz: 0.0,
            rolloff_hz: 0.0,
            flatness: 0.0,
            flux: 0.0,
            rms_db: -120.0,
            bass_db: -120.0,
            mid_db: -120.0,
            high_db: -120.0,
        }
    }
}

impl SpectralFeatures {
    /// The wire order (`FEATURE_COUNT` = 8).
    pub fn to_array(self) -> [f32; 8] {
        [
            self.centroid_hz,
            self.rolloff_hz,
            self.flatness,
            self.flux,
            self.rms_db,
            self.bass_db,
            self.mid_db,
            self.high_db,
        ]
    }
}

fn db(p: f64) -> f32 {
    if p > 1e-24 {
        (10.0 * p.log10()) as f32
    } else {
        -120.0
    }
}

/// Features of `mag` (bin `k` at `k * bin_hz`). `time` holds the newest window in pieces.
/// `gain` rescales `mag` for the band powers only (a layer that carries `1/L` of the average
/// passes `L`); every other feature is scale-free. `prev` is the previous hop's magnitude (same
/// length, preallocated) and is overwritten.
pub fn compute(
    mag: &[f32],
    bin_hz: f64,
    time: [&[f32]; 2],
    gain: f32,
    prev: &mut [f32],
    have_prev: bool,
) -> SpectralFeatures {
    let mut f = SpectralFeatures::default();
    let n = mag.len();
    if n == 0 || bin_hz <= 0.0 {
        return f;
    }
    let b250 = n.min((250.0 / bin_hz) as usize + 1);
    let b4k = n.min((4000.0 / bin_hz) as usize + 1);
    let (mut bands, mut weighted, mut sum_mag, mut log_sum, mut rise) =
        ([0.0f64; 3], 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for (k, &v) in mag.iter().enumerate() {
        let v64 = v as f64;
        let pw = v64 * v64;
        bands[(k >= b250) as usize + (k >= b4k) as usize] += pw;
        weighted += pw * k as f64;
        sum_mag += v64;
        log_sum += crate::db::fast_db(v) as f64;
        if have_prev {
            rise += (v - prev[k]).max(0.0) as f64;
        }
    }
    let total: f64 = bands.iter().sum();
    if total > 0.0 {
        f.centroid_hz = (weighted / total * bin_hz) as f32;
        let target = 0.85 * total;
        let mut acc = 0.0;
        let mut k = 0;
        while k < n {
            acc += mag[k] as f64 * mag[k] as f64;
            if acc >= target {
                break;
            }
            k += 1;
        }
        f.rolloff_hz = (k.min(n - 1) as f64 * bin_hz) as f32;
        let geo = 10f64.powf((log_sum / n as f64) / 10.0);
        f.flatness = (geo / (total / n as f64)).clamp(0.0, 1.0) as f32;
    }
    let g2 = gain as f64 * gain as f64;
    f.bass_db = db(bands[0] * g2);
    f.mid_db = db(bands[1] * g2);
    f.high_db = db(bands[2] * g2);
    f.flux = if have_prev && sum_mag > 0.0 {
        (rise / sum_mag) as f32
    } else {
        0.0
    };
    let n_time = time[0].len() + time[1].len();
    if n_time > 0 {
        let s2: f64 = time
            .iter()
            .flat_map(|part| part.iter())
            .map(|&x| x as f64 * x as f64)
            .sum();
        let rms = (s2 / n_time as f64).sqrt();
        f.rms_db = if rms > 1e-6 {
            (20.0 * rms.log10()) as f32
        } else {
            -120.0
        };
    }
    prev[..n].copy_from_slice(mag);
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_partial_sets_centroid_and_rolloff_and_reads_tonal() {
        let mut mag = vec![0.0f32; 1025];
        mag[100] = 1.0;
        let mut prev = vec![0.0; 1025];
        let x = [0.5f32; 64];
        let f = compute(&mag, 10.0, [&x, &[]], 1.0, &mut prev, false);
        assert!((f.centroid_hz - 1000.0).abs() < 1e-3);
        assert!((f.rolloff_hz - 1000.0).abs() < 1e-3);
        assert!(f.flatness < 1e-6);
        assert!((f.rms_db - -6.0206).abs() < 1e-3);
        assert!(f.mid_db > -1.0 && f.bass_db == -120.0);
        // Same frame again: no flux.
        let g = compute(&mag, 10.0, [&x, &[]], 1.0, &mut prev, true);
        // A layer carrying 1/4 of the average reads the same band power with gain 4.
        let quarter: Vec<f32> = mag.iter().map(|v| v / 4.0).collect();
        let h = compute(&quarter, 10.0, [&x, &[]], 4.0, &mut prev, false);
        assert!((h.mid_db - f.mid_db).abs() < 1e-4);
        assert_eq!(g.flux, 0.0);
    }

    #[test]
    fn white_spectrum_is_flat() {
        let mag = vec![0.3f32; 513];
        let mut prev = vec![0.0; 513];
        let f = compute(&mag, 46.875, [&[], &[]], 1.0, &mut prev, false);
        assert!((f.flatness - 1.0).abs() < 1e-4, "{}", f.flatness);
    }
}

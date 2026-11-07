//! The raw audio scope (docs/super-fft.md section 4): the newest `N` mono samples multiplied by a
//! same-length half sine `sin(pi * n / (N - 1))`, 0 -> 1 -> 0, so the trace fades in and out at
//! its ends instead of jumping. At 48 kHz and 60 fps one frame is 800 samples, the default.
//!
//! The samples are the raw input (before the DC blocker and the EQ that colour the spectrum), so
//! the scope shows the signal as it is.

/// Fills `taper` with the half-sine window of its length (1 for a single sample).
pub fn half_sine(taper: &mut [f32]) {
    let n = taper.len();
    if n == 1 {
        taper[0] = 1.0;
        return;
    }
    let denom = (n - 1) as f64;
    for (i, t) in taper.iter_mut().enumerate() {
        *t = (std::f64::consts::PI * i as f64 / denom).sin() as f32;
    }
}

/// A ring of the newest raw mono samples and the tapered copy published with each frame.
/// Allocates in `new` only.
#[derive(Debug, Clone)]
pub struct Scope {
    ring: Vec<f32>,
    write: usize,
    taper: Vec<f32>,
    out: Vec<f32>,
}

impl Scope {
    /// `len` samples per frame (0 = off).
    pub fn new(len: usize) -> Self {
        let mut taper = vec![0.0; len];
        if len > 0 {
            half_sine(&mut taper);
        }
        Self {
            ring: vec![0.0; len],
            write: 0,
            taper,
            out: vec![0.0; len],
        }
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        if self.ring.is_empty() {
            return;
        }
        self.ring[self.write] = x;
        self.write = (self.write + 1) % self.ring.len();
    }

    pub fn clear(&mut self) {
        self.ring.fill(0.0);
        self.write = 0;
    }

    /// The newest `len` samples, oldest first, tapered. `None` when the scope is off.
    pub fn frame(&mut self) -> Option<&[f32]> {
        if self.ring.is_empty() {
            return None;
        }
        let (newer, older) = self.ring.split_at(self.write);
        let ordered = older.iter().chain(newer);
        for ((o, &x), &t) in self.out.iter_mut().zip(ordered).zip(&self.taper) {
            *o = x * t;
        }
        Some(&self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_taper_is_a_half_sine() {
        let mut t = vec![0.0; 801];
        half_sine(&mut t);
        assert_eq!(t[0], 0.0);
        assert!((t[400] - 1.0).abs() < 1e-7);
        assert!(t[800].abs() < 1e-6);
        assert!((t[200] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn frames_are_the_newest_samples_oldest_first() {
        let mut s = Scope::new(4);
        for x in 1..=6 {
            s.push(x as f32);
        }
        // Newest four: 3, 4, 5, 6; taper sin(0), sin(pi/3), sin(2pi/3), sin(pi).
        let f = s.frame().unwrap().to_vec();
        let k = (std::f64::consts::PI / 3.0).sin() as f32;
        assert_eq!(f[0], 0.0);
        assert!((f[1] - 4.0 * k).abs() < 1e-5 && (f[2] - 5.0 * k).abs() < 1e-5);
        assert!(f[3].abs() < 1e-5);
        assert!(Scope::new(0).frame().is_none());
    }
}

//! Colours: `#rrggbb[aa]` parsing and the palette gradient. Values stay in the sRGB-encoded
//! space the palette is written in; the target is non-sRGB UNORM, so they reach the screen as
//! typed, like the canvas preview.

/// A straight (not premultiplied) colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Multiplies the alpha by `k`.
    pub fn with_alpha(self, k: f32) -> Self {
        Self {
            a: self.a * k.clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn premultiplied(self) -> [f32; 4] {
        let a = self.a.clamp(0.0, 1.0);
        [
            self.r.clamp(0.0, 1.0) * a,
            self.g.clamp(0.0, 1.0) * a,
            self.b.clamp(0.0, 1.0) * a,
            a,
        ]
    }

    pub fn lerp(self, o: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            r: self.r + (o.r - self.r) * t,
            g: self.g + (o.g - self.g) * t,
            b: self.b + (o.b - self.b) * t,
            a: self.a + (o.a - self.a) * t,
        }
    }
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` (the scene pipeline already normalized it).
pub fn parse(s: &str) -> Option<Rgba> {
    let h = s.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    let f = |v: u8| v as f32 / 255.0;
    match h.len() {
        3 => {
            let d = |i: usize| {
                u8::from_str_radix(h.get(i..i + 1)?, 16)
                    .ok()
                    .map(|v| v * 17)
            };
            Some(Rgba::rgb(f(d(0)?), f(d(1)?), f(d(2)?)))
        }
        6 => Some(Rgba::rgb(f(byte(0)?), f(byte(2)?), f(byte(4)?))),
        8 => Some(Rgba {
            r: f(byte(0)?),
            g: f(byte(2)?),
            b: f(byte(4)?),
            a: f(byte(6)?),
        }),
        _ => None,
    }
}

/// Evenly spaced gradient stops.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    stops: Vec<Rgba>,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            stops: vec![Rgba::rgb(0.24, 0.42, 1.0)],
        }
    }
}

impl Palette {
    pub fn new(colors: &[String]) -> Self {
        let stops: Vec<Rgba> = colors.iter().filter_map(|c| parse(c)).collect();
        if stops.is_empty() {
            Self::default()
        } else {
            Self { stops }
        }
    }

    /// The gradient at `t` in 0..1 (first stop at 0, last at 1).
    pub fn at(&self, t: f32) -> Rgba {
        let n = self.stops.len();
        if n == 1 {
            return self.stops[0];
        }
        let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (x as usize).min(n - 2);
        self.stops[i].lerp(self.stops[i + 1], x - i as f32)
    }

    pub fn last(&self) -> Rgba {
        *self.stops.last().expect("never empty")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_form() {
        assert_eq!(parse("#fff"), Some(Rgba::WHITE));
        assert_eq!(parse("#ff0000"), Some(Rgba::rgb(1.0, 0.0, 0.0)));
        let c = parse("#00ff0080").unwrap();
        assert!((c.a - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(parse("red"), None);
        assert_eq!(parse("#12345"), None);
    }

    #[test]
    fn the_gradient_interpolates_between_stops() {
        let p = Palette::new(&["#000000".into(), "#ffffff".into(), "#ff0000".into()]);
        assert_eq!(p.at(0.0), Rgba::rgb(0.0, 0.0, 0.0));
        assert_eq!(p.at(0.5), Rgba::WHITE);
        assert!((p.at(0.25).r - 0.5).abs() < 1e-6);
        assert_eq!(p.at(1.0), Rgba::rgb(1.0, 0.0, 0.0));
        assert_eq!(p.last(), Rgba::rgb(1.0, 0.0, 0.0));
    }

    #[test]
    fn premultiplication_scales_colour_by_alpha() {
        let c = Rgba::rgb(1.0, 0.5, 0.25).with_alpha(0.5).premultiplied();
        assert_eq!(c, [0.5, 0.25, 0.125, 0.5]);
    }
}

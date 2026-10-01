//! Бассейны Ньютона для z³ − 1: z ← z − (z³ − 1)/(3z²).
//!
//! В отличие от escape-time фракталов, здесь точка почти всегда сходится.
//! Раскрашиваем по номеру корня (канал `shade`) и по скорости сходимости
//! (канал `value`) — получается характерная «витражная» картинка.

use super::{Fractal, Home, Pixel};

const EPS: f64 = 1e-9;
const ROOTS: [(f64, f64); 3] = [
    (1.0, 0.0),
    (-0.5, 0.8660254037844386),
    (-0.5, -0.8660254037844386),
];

pub struct Newton {
    max_iter: u32,
}

impl Newton {
    pub fn new() -> Self {
        Newton { max_iter: 64 }
    }
}

impl Default for Newton {
    fn default() -> Self {
        Self::new()
    }
}

impl Fractal for Newton {
    fn id(&self) -> &'static str {
        "newton"
    }

    fn name(&self) -> &'static str {
        "Newton z³−1"
    }

    fn max_iter(&self) -> u32 {
        self.max_iter
    }

    fn set_max_iter(&mut self, iter: u32) {
        // Ньютон сходится за единицы итераций: огромный бюджет только тратит время.
        self.max_iter = iter.clamp(16, 256);
    }

    fn home(&self) -> Home {
        Home {
            x: 0.0,
            y: 0.0,
            zoom: 1.6,
        }
    }

    fn sample(&self, x: f64, y: f64) -> Pixel {
        let mut zx = x;
        let mut zy = y;

        for iter in 0..self.max_iter {
            // z³
            let zx2 = zx * zx - zy * zy;
            let zy2 = 2.0 * zx * zy;
            let z3x = zx2 * zx - zy2 * zy;
            let z3y = zx2 * zy + zy2 * zx;

            // 3z²
            let dx = 3.0 * zx2;
            let dy = 3.0 * zy2;
            let den = dx * dx + dy * dy;
            if den < 1e-300 {
                // Попали ровно в ноль производной — точка «застряла».
                return Pixel::INSIDE;
            }

            // (z³ − 1) / 3z²
            let nx = z3x - 1.0;
            let ny = z3y;
            let qx = (nx * dx + ny * dy) / den;
            let qy = (ny * dx - nx * dy) / den;

            zx -= qx;
            zy -= qy;

            if qx * qx + qy * qy < EPS * EPS {
                for (root_idx, (rx, ry)) in ROOTS.iter().enumerate() {
                    if (zx - rx).abs() < 1e-4 && (zy - ry).abs() < 1e-4 {
                        return Pixel::escaped(iter as f64, root_idx as f64 / ROOTS.len() as f64);
                    }
                }
                return Pixel::escaped(iter as f64, 0.0);
            }
        }

        Pixel::INSIDE
    }

    /// Число итераций почти не зависит от масштаба — бассейны самоподобны.
    fn iters_for_zoom(&self, _zoom: f64, base: u32) -> u32 {
        base.clamp(16, 256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_converge_to_themselves() {
        let n = Newton::new();
        for (i, (rx, ry)) in ROOTS.iter().enumerate() {
            let p = n.sample(*rx, *ry);
            assert!(!p.inside, "корень {i} должен сойтись");
            let expected = i as f64 / 3.0;
            assert!((p.shade - expected).abs() < 1e-9, "корень {i}: {p:?}");
        }
    }

    #[test]
    fn origin_is_degenerate() {
        // z = 0 — ноль производной, точка никуда не сходится.
        assert!(Newton::new().sample(0.0, 0.0).inside);
    }
}

//! z ← z² + c при фиксированном c и z₀ = точка экрана.
//!
//! Параметр `c` медленно едет по окружности радиуса 0.7885 — по ней проходят
//! самые зрелищные формы, поэтому множество непрерывно перетекает само в себя.

use super::{Fractal, Home, Pixel, smooth_iter};

const ESCAPE: f64 = 65536.0;
const RADIUS: f64 = 0.7885;

pub struct Julia {
    max_iter: u32,
    cx: f64,
    cy: f64,
    phase: f64,
    /// Скорость движения `c` в радианах в секунду. 0 — заморозить.
    pub drift: f64,
}

impl Julia {
    pub fn new() -> Self {
        let mut j = Julia {
            max_iter: 200,
            cx: 0.0,
            cy: 0.0,
            phase: 2.2,
            drift: 0.06,
        };
        j.recompute_c();
        j
    }

    pub fn c(&self) -> (f64, f64) {
        (self.cx, self.cy)
    }

    fn recompute_c(&mut self) {
        self.cx = RADIUS * self.phase.cos();
        self.cy = RADIUS * self.phase.sin();
    }
}

impl Default for Julia {
    fn default() -> Self {
        Self::new()
    }
}

impl Fractal for Julia {
    fn id(&self) -> &'static str {
        "julia"
    }

    fn name(&self) -> &'static str {
        "Julia"
    }

    fn max_iter(&self) -> u32 {
        self.max_iter
    }

    fn set_max_iter(&mut self, iter: u32) {
        self.max_iter = iter.max(16);
    }

    fn home(&self) -> Home {
        Home {
            x: 0.0,
            y: 0.0,
            zoom: 1.7,
        }
    }

    fn tick(&mut self, dt: f64) {
        if self.drift != 0.0 {
            self.phase += dt * self.drift;
            self.recompute_c();
        }
    }

    fn info(&self) -> String {
        let (cx, cy) = self.c();
        format!("c = {cx:+.4} {cy:+.4}i")
    }

    fn sample(&self, x: f64, y: f64) -> Pixel {
        let mut zx = x;
        let mut zy = y;
        let mut iter = 0u32;

        while zx * zx + zy * zy <= ESCAPE && iter < self.max_iter {
            let t = zx * zx - zy * zy + self.cx;
            zy = 2.0 * zx * zy + self.cy;
            zx = t;
            iter += 1;
        }

        if iter >= self.max_iter {
            Pixel::INSIDE
        } else {
            Pixel::escaped(smooth_iter(iter, zx, zy), 0.0)
        }
    }

    /// Жюлиа не «уплотняется» при увеличении так же сильно, как Мандельброт.
    fn iters_for_zoom(&self, zoom: f64, base: u32) -> u32 {
        let extra = (2.0 / zoom.max(1e-16)).log2().max(0.0) * 14.0;
        (base as f64 + extra).min(3000.0) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_stays_on_circle() {
        let mut j = Julia::new();
        for _ in 0..200 {
            j.tick(0.1);
            let (cx, cy) = j.c();
            assert!(((cx * cx + cy * cy).sqrt() - RADIUS).abs() < 1e-12);
        }
    }

    #[test]
    fn outside_points_escape() {
        let j = Julia::new();
        let p = j.sample(3.0, 3.0);
        assert!(!p.inside);
    }

    #[test]
    fn frozen_c_does_not_move() {
        let mut j = Julia::new();
        j.drift = 0.0;
        let before = j.c();
        j.tick(5.0);
        assert_eq!(before, j.c());
    }
}

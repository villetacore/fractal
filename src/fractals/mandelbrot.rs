//! z ← z² + c, z₀ = 0.

use super::{Fractal, Home, Pixel, smooth_iter};

const ESCAPE: f64 = 65536.0;

pub struct Mandelbrot {
    max_iter: u32,
}

impl Mandelbrot {
    pub fn new() -> Self {
        Mandelbrot { max_iter: 200 }
    }
}

impl Default for Mandelbrot {
    fn default() -> Self {
        Self::new()
    }
}

impl Fractal for Mandelbrot {
    fn id(&self) -> &'static str {
        "mandelbrot"
    }

    fn name(&self) -> &'static str {
        "Mandelbrot"
    }

    fn max_iter(&self) -> u32 {
        self.max_iter
    }

    fn set_max_iter(&mut self, iter: u32) {
        self.max_iter = iter.max(16);
    }

    fn home(&self) -> Home {
        Home {
            x: -0.6,
            y: 0.0,
            zoom: 1.8,
        }
    }

    fn sample(&self, cx: f64, cy: f64) -> Pixel {
        // Кардиоида и главная почка проверяются аналитически: это самые
        // дорогие области кадра, и без проверки они съедают весь бюджет.
        let q = (cx - 0.25) * (cx - 0.25) + cy * cy;
        if q * (q + (cx - 0.25)) <= 0.25 * cy * cy {
            return Pixel::INSIDE;
        }
        if (cx + 1.0) * (cx + 1.0) + cy * cy <= 0.0625 {
            return Pixel::INSIDE;
        }

        let mut zx = 0.0f64;
        let mut zy = 0.0f64;
        let mut zx2 = 0.0f64;
        let mut zy2 = 0.0f64;
        let mut iter = 0u32;

        // Детектор цикла (метод Брента): точки внутри множества
        // обнаруживаются задолго до исчерпания итераций.
        let mut px = 0.0f64;
        let mut py = 0.0f64;
        let mut period = 1u32;
        let mut steps = 0u32;

        while zx2 + zy2 <= ESCAPE && iter < self.max_iter {
            zy = 2.0 * zx * zy + cy;
            zx = zx2 - zy2 + cx;
            zx2 = zx * zx;
            zy2 = zy * zy;
            iter += 1;

            if (zx - px).abs() < 1e-14 && (zy - py).abs() < 1e-14 {
                return Pixel::INSIDE;
            }
            steps += 1;
            if steps == period {
                px = zx;
                py = zy;
                steps = 0;
                period *= 2;
            }
        }

        if iter >= self.max_iter {
            Pixel::INSIDE
        } else {
            Pixel::escaped(smooth_iter(iter, zx, zy), 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_is_inside() {
        assert!(Mandelbrot::new().sample(0.0, 0.0).inside);
        assert!(Mandelbrot::new().sample(-1.0, 0.0).inside);
    }

    #[test]
    fn far_point_escapes_fast() {
        let p = Mandelbrot::new().sample(2.0, 2.0);
        assert!(!p.inside);
        assert!(p.value < 5.0, "value = {}", p.value);
    }

    #[test]
    fn smooth_value_is_monotonic_near_boundary() {
        let m = Mandelbrot::new();
        let a = m.sample(0.30, 0.0).value;
        let b = m.sample(0.40, 0.0).value;
        assert!(
            a > b,
            "ближе к границе итераций должно быть больше: {a} vs {b}"
        );
    }
}

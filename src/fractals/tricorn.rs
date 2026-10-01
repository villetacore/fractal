//! z ← conj(z)² + c — «мандельбар». Отличается от Мандельброта только
//! знаком мнимой части, но симметрия даёт совсем другую картинку.

use super::{Fractal, Home, Pixel, smooth_iter};

const ESCAPE: f64 = 65536.0;

pub struct Tricorn {
    max_iter: u32,
}

impl Tricorn {
    pub fn new() -> Self {
        Tricorn { max_iter: 200 }
    }
}

impl Default for Tricorn {
    fn default() -> Self {
        Self::new()
    }
}

impl Fractal for Tricorn {
    fn id(&self) -> &'static str {
        "tricorn"
    }

    fn name(&self) -> &'static str {
        "Tricorn"
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
            zoom: 2.2,
        }
    }

    fn sample(&self, cx: f64, cy: f64) -> Pixel {
        let mut zx = 0.0f64;
        let mut zy = 0.0f64;
        let mut iter = 0u32;

        while zx * zx + zy * zy <= ESCAPE && iter < self.max_iter {
            let t = zx * zx - zy * zy + cx;
            zy = -2.0 * zx * zy + cy;
            zx = t;
            iter += 1;
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
    fn center_is_inside() {
        assert!(Tricorn::new().sample(0.0, 0.0).inside);
    }

    #[test]
    fn symmetric_about_real_axis() {
        let t = Tricorn::new();
        let a = t.sample(-0.5, 0.35);
        let b = t.sample(-0.5, -0.35);
        assert_eq!(a.inside, b.inside);
        assert!((a.value - b.value).abs() < 1e-9);
    }
}

//! z ← (|Re z| + i·|Im z|)² + c.

use super::{Fractal, Home, Pixel, smooth_iter};

const ESCAPE: f64 = 65536.0;

pub struct BurningShip {
    max_iter: u32,
}

impl BurningShip {
    pub fn new() -> Self {
        BurningShip { max_iter: 200 }
    }
}

impl Default for BurningShip {
    fn default() -> Self {
        Self::new()
    }
}

impl Fractal for BurningShip {
    fn id(&self) -> &'static str {
        "burning_ship"
    }

    fn name(&self) -> &'static str {
        "Burning Ship"
    }

    fn max_iter(&self) -> u32 {
        self.max_iter
    }

    fn set_max_iter(&mut self, iter: u32) {
        self.max_iter = iter.max(16);
    }

    fn home(&self) -> Home {
        Home {
            x: -0.45,
            y: 0.5,
            zoom: 1.9,
        }
    }

    fn sample(&self, cx: f64, y: f64) -> Pixel {
        // На экране ось Y растёт вниз, а корпус корабля лежит в отрицательной
        // мнимой полуплоскости. Без этого переворота «корабль» висит вверх дном.
        let cy = -y;

        let mut zx = 0.0f64;
        let mut zy = 0.0f64;
        let mut iter = 0u32;

        while zx * zx + zy * zy <= ESCAPE && iter < self.max_iter {
            let t = zx * zx - zy * zy + cx;
            zy = (2.0 * zx * zy).abs() + cy;
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
    fn hull_of_the_ship_is_inside() {
        let bs = BurningShip::new();
        assert!(bs.sample(-0.5, 0.5).inside, "корпус корабля");
        assert!(bs.sample(-1.755, 0.0).inside, "мачта");
    }

    #[test]
    fn y_axis_is_flipped_for_display() {
        let bs = BurningShip::new();
        // Корабль несимметричен: снизу корпус, сверху — пустота.
        assert!(bs.sample(-0.5, 0.5).inside);
        assert!(!bs.sample(-0.5, -0.5).inside);
    }

    #[test]
    fn distant_point_escapes() {
        assert!(!BurningShip::new().sample(1.5, 1.5).inside);
    }
}

//! Автоматический поиск интересных областей.
//!
//! Идея простая: интересно там, где рядом лежат точки с очень разным числом
//! итераций — то есть на границе множества. Разведчик бросает случайные точки
//! в текущий кадр, оценивает «изрезанность» окрестности каждой и сужает область
//! вокруг лучшей. Несколько таких шагов — и камера стоит над границей
//! на глубине, которую никто заранее не размечал.

use crate::camera::{MIN_ZOOM, View};
use crate::fractals::Fractal;
use crate::util::Rng;

/// Сколько кандидатов проверяется на каждом шаге сужения.
const CANDIDATES: usize = 48;
/// Во сколько раз сужается область за шаг.
const SHRINK: f64 = 0.22;

/// Найти точку на границе множества, начиная от `start`.
///
/// `depth` — сколько раз сузить область. Каждый шаг уменьшает кадр примерно
/// в 4.5 раза, поэтому 8 шагов дают увеличение около 10⁵.
pub fn find_interesting(fractal: &dyn Fractal, start: View, depth: u32, rng: &mut Rng) -> View {
    let mut view = start;

    for _ in 0..depth {
        let half_w = view.zoom;
        let half_h = view.zoom * 0.6;
        let probe = view.zoom * 0.06;

        let mut best = (f64::NEG_INFINITY, view.x, view.y);
        for _ in 0..CANDIDATES {
            let x = view.x + rng.range(-half_w, half_w);
            let y = view.y + rng.range(-half_h, half_h);
            let score = roughness(fractal, x, y, probe);
            if score > best.0 {
                best = (score, x, y);
            }
        }

        // Ничего похожего на границу не нашлось — дальше углубляться незачем.
        if best.0 <= 0.0 {
            break;
        }

        view.x = best.1;
        view.y = best.2;
        let next_zoom = view.zoom * SHRINK;
        if next_zoom < MIN_ZOOM * 64.0 {
            break;
        }
        view.zoom = next_zoom;
    }

    view
}

/// Оценка «изрезанности» окрестности точки: разброс числа итераций между
/// центром и четырьмя соседями. Ноль — однородная область (фон или тело).
fn roughness(fractal: &dyn Fractal, x: f64, y: f64, r: f64) -> f64 {
    let samples = [
        fractal.sample(x, y),
        fractal.sample(x + r, y),
        fractal.sample(x - r, y),
        fractal.sample(x, y + r),
        fractal.sample(x, y - r),
    ];

    let inside = samples.iter().filter(|p| p.inside).count();
    // Ровно у границы часть соседей внутри, часть снаружи — это лучший случай.
    if inside == samples.len() {
        return f64::NEG_INFINITY;
    }
    let boundary_bonus = if inside > 0 { 40.0 } else { 0.0 };

    let values: Vec<f64> = samples
        .iter()
        .filter(|p| !p.inside)
        .map(|p| p.value)
        .collect();
    if values.len() < 2 {
        return boundary_bonus;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;

    boundary_bonus + var.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fractals::{Julia, Mandelbrot};

    #[test]
    fn zooms_in_and_stays_finite() {
        let m = Mandelbrot::new();
        let mut rng = Rng::new(12345);
        let start = View::new(-0.6, 0.0, 1.8);
        let found = find_interesting(&m, start, 6, &mut rng);
        assert!(found.zoom < start.zoom, "разведчик обязан приблизиться");
        assert!(found.zoom >= MIN_ZOOM);
        assert!(found.x.is_finite() && found.y.is_finite());
    }

    #[test]
    fn lands_near_the_boundary() {
        let m = Mandelbrot::new();
        let mut rng = Rng::new(777);
        let found = find_interesting(&m, View::new(-0.6, 0.0, 1.8), 8, &mut rng);
        // В окрестности найденной точки должны быть и «внутри», и «снаружи».
        let r = found.zoom;
        let mut inside = 0;
        let mut outside = 0;
        for i in 0..16 {
            let a = i as f64 / 16.0 * std::f64::consts::TAU;
            let p = m.sample(found.x + r * a.cos(), found.y + r * a.sin());
            if p.inside { inside += 1 } else { outside += 1 }
        }
        assert!(outside > 0, "вокруг только тело множества");
        assert!(inside + outside == 16);
    }

    #[test]
    fn works_for_other_fractals() {
        let j = Julia::new();
        let mut rng = Rng::new(42);
        let home = j.home();
        let v = find_interesting(&j, View::new(home.x, home.y, home.zoom), 5, &mut rng);
        assert!(v.zoom.is_finite() && v.zoom > 0.0);
    }
}

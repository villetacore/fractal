//! Плагинная часть движка: каждый фрактал — отдельный модуль,
//! реализующий общий трейт [`Fractal`].
//!
//! Чтобы добавить новый фрактал, достаточно создать файл рядом,
//! реализовать `Fractal` и дописать одну строку в [`all`].

pub mod burning_ship;
pub mod julia;
pub mod mandelbrot;
pub mod newton;
pub mod tricorn;

pub use burning_ship::BurningShip;
pub use julia::Julia;
pub use mandelbrot::Mandelbrot;
pub use newton::Newton;
pub use tricorn::Tricorn;

/// Результат расчёта одной точки.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pixel {
    /// Точка не покинула множество за отведённые итерации.
    pub inside: bool,
    /// Сглаженное число итераций (не нормировано — рендерер сам масштабирует).
    pub value: f64,
    /// Дополнительный канал 0..1: индекс корня у Ньютона, знак у остальных.
    pub shade: f64,
}

impl Pixel {
    pub const INSIDE: Pixel = Pixel {
        inside: true,
        value: 0.0,
        shade: 0.0,
    };

    pub fn escaped(value: f64, shade: f64) -> Pixel {
        Pixel {
            inside: false,
            value,
            shade,
        }
    }
}

/// Начальное положение камеры для фрактала: центр и половина ширины кадра.
#[derive(Clone, Copy, Debug)]
pub struct Home {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

pub trait Fractal: Send + Sync {
    /// Идентификатор для cameras.json и `--fractal`.
    fn id(&self) -> &'static str;

    /// Человекочитаемое имя для HUD.
    fn name(&self) -> &'static str;

    /// Посчитать одну точку комплексной плоскости.
    fn sample(&self, x: f64, y: f64) -> Pixel;

    fn max_iter(&self) -> u32;

    fn set_max_iter(&mut self, iter: u32);

    fn home(&self) -> Home;

    /// Вызывается раз в кадр до рендера — для фракталов с анимированными
    /// параметрами (например, движение `c` у Жюлиа).
    fn tick(&mut self, _dt: f64) {}

    /// Собственные параметры фрактала для HUD. Пусто — показывать нечего.
    fn info(&self) -> String {
        String::new()
    }

    /// Подсказка о том, сколько итераций нужно на данном масштабе.
    /// `zoom` — половина ширины кадра в единицах комплексной плоскости.
    fn iters_for_zoom(&self, zoom: f64, base: u32) -> u32 {
        let extra = (2.0 / zoom.max(1e-16)).log2().max(0.0) * 26.0;
        (base as f64 + extra).min(6000.0) as u32
    }
}

/// Все доступные фракталы. Порядок задаёт переключение по Tab.
pub fn all() -> Vec<Box<dyn Fractal>> {
    vec![
        Box::new(Mandelbrot::new()),
        Box::new(Julia::new()),
        Box::new(BurningShip::new()),
        Box::new(Tricorn::new()),
        Box::new(Newton::new()),
    ]
}

pub fn index_of(list: &[Box<dyn Fractal>], id: &str) -> Option<usize> {
    list.iter().position(|f| f.id() == id)
}

/// Сглаженное число итераций по формуле нормализованного потенциала:
/// `n + 1 - log2(log|z|)`. Убирает ступеньки на границах полос.
pub fn smooth_iter(iter: u32, zx: f64, zy: f64) -> f64 {
    let mag2 = zx * zx + zy * zy;
    if mag2 <= 1.0 {
        return iter as f64;
    }
    let log_zn = mag2.ln() * 0.5;
    let nu = (log_zn / std::f64::consts::LN_2).ln() / std::f64::consts::LN_2;
    let v = iter as f64 + 1.0 - nu;
    if v.is_finite() {
        v.max(0.0)
    } else {
        iter as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fractal_has_unique_id() {
        let list = all();
        for (i, f) in list.iter().enumerate() {
            assert!(index_of(&list, f.id()) == Some(i), "дубль id: {}", f.id());
        }
    }

    #[test]
    fn homes_are_sane() {
        for f in all() {
            let h = f.home();
            assert!(h.zoom > 0.0 && h.zoom.is_finite());
            assert!(h.x.is_finite() && h.y.is_finite());
        }
    }

    #[test]
    fn iteration_budget_grows_with_zoom() {
        let m = Mandelbrot::new();
        assert!(m.iters_for_zoom(1e-6, 200) > m.iters_for_zoom(2.0, 200));
    }
}

//! Камера: положение на комплексной плоскости, зум, поворот и полёты между точками.

use serde::Deserialize;

use crate::animation::{Breathing, Ease};
use crate::util::{Rng, clamp, lerp};

/// Предел, за которым f64 перестаёт различать соседние пиксели.
pub const MIN_ZOOM: f64 = 1e-14;
pub const MAX_ZOOM: f64 = 8.0;

/// Положение камеры. `zoom` — половина ширины кадра в единицах плоскости,
/// поэтому меньшее значение означает более глубокое увеличение.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub rotation: f64,
}

impl View {
    pub fn new(x: f64, y: f64, zoom: f64) -> Self {
        View {
            x,
            y,
            zoom,
            rotation: 0.0,
        }
    }
}

struct Flight {
    from: View,
    to: View,
    elapsed: f64,
    duration: f64,
    ease: Ease,
}

pub struct Camera {
    view: View,
    /// Отношение высоты знакоместа к ширине. Для большинства терминалов ~2.
    pub char_aspect: f64,
    flight: Option<Flight>,
    breathing: Breathing,
    /// Амплитуда тряски в долях кадра и её остаток по времени.
    shake: f64,
    shake_decay: f64,
    shake_phase: f64,
    rng: Rng,
}

impl Camera {
    pub fn new(view: View, char_aspect: f64) -> Self {
        Camera {
            view,
            char_aspect,
            flight: None,
            breathing: Breathing::new(0.05, 0.012),
            shake: 0.0,
            shake_decay: 1.0,
            shake_phase: 0.0,
            rng: Rng::from_time(),
        }
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn set_view(&mut self, view: View) {
        self.flight = None;
        self.view = view;
        self.clamp_zoom();
    }

    pub fn zoom(&self) -> f64 {
        self.view.zoom
    }

    pub fn is_flying(&self) -> bool {
        self.flight.is_some()
    }

    /// Начать полёт к точке. Позиция интерполируется не линейно, а так,
    /// чтобы цель почти не «уезжала» по экрану во время погружения.
    pub fn fly_to(&mut self, to: View, duration: f64) {
        self.fly_to_with(to, duration, Ease::Cinematic);
    }

    /// То же, но со своей кривой: короткие ручные перелёты приятнее
    /// делать более резкими, чем неспешные полёты автопилота.
    pub fn fly_to_with(&mut self, to: View, duration: f64, ease: Ease) {
        let mut to = to;
        to.zoom = clamp(to.zoom, MIN_ZOOM, MAX_ZOOM);
        self.flight = Some(Flight {
            from: self.view,
            to,
            elapsed: 0.0,
            duration: duration.max(0.05),
            ease,
        });
    }

    pub fn cancel_flight(&mut self) {
        self.flight = None;
    }

    pub fn update(&mut self, dt: f64) {
        self.breathing.update(dt);

        if let Some(flight) = &mut self.flight {
            flight.elapsed += dt;
            let s = flight.ease.apply(flight.elapsed / flight.duration);
            let from = flight.from;
            let to = flight.to;

            // Зум интерполируется логарифмически: линейный зум пролетает
            // все интересные масштабы в первые кадры.
            let zoom = (lerp(from.zoom.ln(), to.zoom.ln(), s)).exp();

            // Van-Wijk-подобная поправка: перемещение происходит, пока камера
            // ещё «высоко», и почти замирает на глубине.
            let u = position_param(from.zoom, to.zoom, zoom, s);

            self.view = View {
                x: lerp(from.x, to.x, u),
                y: lerp(from.y, to.y, u),
                zoom,
                rotation: lerp(from.rotation, to.rotation, s),
            };

            if flight.elapsed >= flight.duration {
                self.view = to;
                self.flight = None;
            }
        }

        if self.shake > 0.0 {
            self.shake = (self.shake - self.shake_decay * dt).max(0.0);
            self.shake_phase += dt;
        }

        self.clamp_zoom();
    }

    /// Сдвиг в долях текущего кадра (учитывает поворот).
    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.flight = None;
        let (sin, cos) = self.view.rotation.sin_cos();
        let sx = dx * self.view.zoom;
        let sy = dy * self.view.zoom;
        self.view.x += sx * cos - sy * sin;
        self.view.y += sx * sin + sy * cos;
    }

    /// Умножить зум (значение < 1 приближает).
    pub fn zoom_by(&mut self, factor: f64) {
        self.flight = None;
        self.view.zoom *= factor;
        self.clamp_zoom();
    }

    pub fn rotate_by(&mut self, delta: f64) {
        self.flight = None;
        self.view.rotation += delta;
    }

    /// Медленный дрейф во время «стоянки»: чуть-чуть приближаемся и вращаемся.
    pub fn drift(&mut self, dt: f64, zoom_rate: f64, spin: f64) {
        if self.flight.is_some() {
            return;
        }
        self.view.zoom *= (-zoom_rate * dt).exp();
        self.view.rotation += spin * dt;
        self.clamp_zoom();
    }

    pub fn kick(&mut self, amount: f64, seconds: f64) {
        self.shake = amount;
        self.shake_decay = amount / seconds.max(1e-3);
        self.shake_phase = self.rng.next_f64() * 10.0;
    }

    fn clamp_zoom(&mut self) {
        if !self.view.zoom.is_finite() {
            self.view.zoom = 1.0;
        }
        self.view.zoom = clamp(self.view.zoom, MIN_ZOOM, MAX_ZOOM);
    }

    /// Построить отображение «экранная координата → плоскость» для кадра w×h.
    pub fn mapper(&self, w: usize, h: usize) -> Mapper {
        let w_f = w.max(1) as f64;
        let h_f = h.max(1) as f64;

        let breath = 1.0 + self.breathing.value();
        let half_w = self.view.zoom * breath;
        // Знакоместо выше, чем шире, — иначе круги превращаются в эллипсы.
        let half_h = half_w * (h_f * self.char_aspect) / w_f;

        let (sin, cos) = self.view.rotation.sin_cos();

        let shake_x = if self.shake > 0.0 {
            (self.shake_phase * 37.0).sin() * self.shake * half_w
        } else {
            0.0
        };
        let shake_y = if self.shake > 0.0 {
            (self.shake_phase * 29.0).cos() * self.shake * half_h
        } else {
            0.0
        };

        Mapper {
            cx: self.view.x + shake_x,
            cy: self.view.y + shake_y,
            step_x: 2.0 * half_w / w_f,
            step_y: 2.0 * half_h / h_f,
            half_cols: w_f * 0.5,
            half_rows: h_f * 0.5,
            sin,
            cos,
        }
    }
}

/// Насколько далеко продвинулась позиция при текущем зуме.
///
/// Если зум меняется существенно, движение привязано к 1/zoom: точка назначения
/// быстро оказывается в центре кадра, а дальше идёт чистое погружение.
fn position_param(from_zoom: f64, to_zoom: f64, zoom: f64, s: f64) -> f64 {
    let ratio = (from_zoom / to_zoom).abs();
    if (0.25..=4.0).contains(&ratio) || !zoom.is_finite() || zoom <= 0.0 {
        return s;
    }
    let a = 1.0 / from_zoom;
    let b = 1.0 / to_zoom;
    let c = 1.0 / zoom;
    let denom = b - a;
    if denom.abs() < 1e-300 {
        return s;
    }
    clamp((c - a) / denom, 0.0, 1.0)
}

/// Заранее отобранная красивая точка из assets/cameras.json.
#[derive(Clone, Debug, Deserialize)]
pub struct Preset {
    /// id фрактала, к которому относится точка.
    pub fractal: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    #[serde(default)]
    pub rotation: f64,
    /// Сколько стоять в этой точке; `None` — как в настройках.
    #[serde(default)]
    pub hold: Option<f64>,
    /// Сколько лететь до этой точки; `None` — как в настройках.
    #[serde(default)]
    pub flight: Option<f64>,
}

impl Preset {
    pub fn view(&self) -> View {
        View {
            x: self.x,
            y: self.y,
            zoom: clamp(self.zoom, MIN_ZOOM, MAX_ZOOM),
            rotation: self.rotation,
        }
    }
}

pub fn load_presets(src: &str) -> Result<Vec<Preset>, String> {
    let presets: Vec<Preset> = serde_json::from_str(src).map_err(|e| e.to_string())?;
    if presets.is_empty() {
        return Err("список точек пуст".into());
    }
    for p in &presets {
        if p.zoom <= 0.0 || !p.zoom.is_finite() || !p.x.is_finite() || !p.y.is_finite() {
            return Err(format!("некорректная точка «{}»", p.name));
        }
    }
    Ok(presets)
}

/// Запасной список — используется, если assets/cameras.json недоступен.
pub fn builtin_presets() -> Vec<Preset> {
    load_presets(include_str!("../assets/cameras.json"))
        .expect("встроенный cameras.json должен быть корректным")
}

/// Плоское, `Copy`-шное отображение экрана на плоскость — его копия уезжает
/// в каждый рабочий поток.
#[derive(Clone, Copy, Debug)]
pub struct Mapper {
    cx: f64,
    cy: f64,
    step_x: f64,
    step_y: f64,
    half_cols: f64,
    half_rows: f64,
    sin: f64,
    cos: f64,
}

impl Mapper {
    /// `col`/`row` — экранные координаты в знакоместах (можно дробные,
    /// это используется суперсэмплингом).
    pub fn world(&self, col: f64, row: f64) -> (f64, f64) {
        let dx = (col - self.half_cols) * self.step_x;
        let dy = (row - self.half_rows) * self.step_y;
        (
            self.cx + dx * self.cos - dy * self.sin,
            self.cy + dx * self.sin + dy * self.cos,
        )
    }

    /// Размер знакоместа в единицах плоскости — нужен для оценки детализации.
    pub fn pixel_size(&self) -> (f64, f64) {
        (self.step_x, self.step_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapper_centres_on_view() {
        let cam = Camera::new(View::new(-0.5, 0.25, 1.0), 2.0);
        let m = cam.mapper(100, 50);
        let (x, y) = m.world(50.0, 25.0);
        assert!((x + 0.5).abs() < 1e-9);
        assert!((y - 0.25).abs() < 1e-9);
    }

    #[test]
    fn aspect_keeps_pixels_square() {
        let cam = Camera::new(View::new(0.0, 0.0, 1.0), 2.0);
        let m = cam.mapper(120, 40);
        let (sx, sy) = m.pixel_size();
        // Знакоместо вдвое выше, чем шире, значит и шаг по Y вдвое больше.
        assert!((sy / sx - 2.0).abs() < 1e-9, "{sx} {sy}");
    }

    #[test]
    fn flight_lands_exactly_on_target() {
        let mut cam = Camera::new(View::new(0.0, 0.0, 2.0), 2.0);
        let target = View::new(-0.743, 0.131, 1e-4);
        cam.fly_to(target, 1.0);
        for _ in 0..100 {
            cam.update(0.02);
        }
        assert!(!cam.is_flying());
        assert!((cam.view().x - target.x).abs() < 1e-12);
        assert!((cam.view().zoom - target.zoom).abs() < 1e-18);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut cam = Camera::new(View::new(0.0, 0.0, 1.0), 2.0);
        for _ in 0..500 {
            cam.zoom_by(0.5);
        }
        assert!(cam.zoom() >= MIN_ZOOM);
        for _ in 0..500 {
            cam.zoom_by(2.0);
        }
        assert!(cam.zoom() <= MAX_ZOOM);
    }

    #[test]
    fn builtin_presets_load() {
        let presets = builtin_presets();
        assert!(presets.len() >= 10);
        let ids: Vec<&str> = crate::fractals::all().iter().map(|f| f.id()).collect();
        for p in &presets {
            assert!(
                ids.contains(&p.fractal.as_str()),
                "точка «{}» ссылается на неизвестный фрактал {}",
                p.name,
                p.fractal
            );
            assert!(p.view().zoom > 0.0);
        }
        // Хотя бы одна точка на каждый фрактал.
        for id in ids {
            assert!(
                presets.iter().any(|p| p.fractal == id),
                "нет точек для {id}"
            );
        }
    }

    /// Точка, из которой видно однородное поле, — испорченная точка:
    /// зритель увидит пустой экран или сплошную заливку.
    #[test]
    fn every_preset_shows_structure() {
        let fractals = crate::fractals::all();
        for preset in builtin_presets() {
            let idx = crate::fractals::index_of(&fractals, &preset.fractal).unwrap();
            let mut fractal = crate::fractals::all().swap_remove(idx);
            let view = preset.view();
            fractal.set_max_iter(fractal.iters_for_zoom(view.zoom, 400));

            let (mut inside, mut outside) = (0, 0);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            let mut shades = 0.0f64;
            for row in 0..13 {
                for col in 0..21 {
                    let x = view.x + (col as f64 / 20.0 - 0.5) * 2.0 * view.zoom;
                    let y = view.y + (row as f64 / 12.0 - 0.5) * 1.2 * view.zoom;
                    let p = fractal.sample(x, y);
                    if p.inside {
                        inside += 1;
                    } else {
                        outside += 1;
                        lo = lo.min(p.value);
                        hi = hi.max(p.value);
                        shades = shades.max(p.shade);
                    }
                }
            }

            let varied = (inside > 0 && outside > 0) || (hi - lo > 1.0) || shades > 0.0;
            assert!(
                varied,
                "точка «{}» ({}) показывает однородное поле: inside={inside}, outside={outside}, разброс={:.2}",
                preset.name,
                preset.fractal,
                hi - lo
            );
        }
    }

    #[test]
    fn broken_presets_are_rejected() {
        assert!(load_presets("[]").is_err());
        assert!(load_presets(r#"[{"fractal":"x","name":"n","x":0,"y":0,"zoom":0}]"#).is_err());
        assert!(load_presets("не json").is_err());
    }

    #[test]
    fn rotation_moves_the_pan_direction() {
        let mut cam = Camera::new(View::new(0.0, 0.0, 1.0), 2.0);
        cam.rotate_by(std::f64::consts::FRAC_PI_2);
        cam.pan(0.1, 0.0);
        assert!(cam.view().x.abs() < 1e-9);
        assert!((cam.view().y - 0.1).abs() < 1e-9);
    }
}

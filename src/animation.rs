//! Кривые сглаживания и простые эффекты: fade, вспышка, "дыхание", крутилки.

use crate::util::{clamp01, lerp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    /// Быстрый старт и торможение — для коротких, «отзывчивых» движений.
    InOutCubic,
    InOutSine,
    /// Долгий разгон, долгое торможение — «кинематографический» полёт.
    Cinematic,
}

impl Ease {
    pub fn apply(self, t: f64) -> f64 {
        let t = clamp01(t);
        match self {
            Ease::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Ease::InOutSine => 0.5 - 0.5 * (std::f64::consts::PI * t).cos(),
            // smootherstep: нулевые первая и вторая производные на концах.
            Ease::Cinematic => t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
        }
    }
}

/// Одномерная анимация значения за фиксированное время.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f64,
    to: f64,
    elapsed: f64,
    duration: f64,
    ease: Ease,
}

impl Tween {
    pub fn new(from: f64, to: f64, duration: f64, ease: Ease) -> Self {
        Tween {
            from,
            to,
            elapsed: 0.0,
            duration: duration.max(1e-6),
            ease,
        }
    }

    pub fn update(&mut self, dt: f64) -> f64 {
        self.elapsed = (self.elapsed + dt).min(self.duration);
        self.value()
    }

    pub fn value(&self) -> f64 {
        lerp(
            self.from,
            self.to,
            self.ease.apply(self.elapsed / self.duration),
        )
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Плавное затемнение/появление кадра. 1.0 — полная яркость, 0.0 — чёрный экран.
#[derive(Clone, Copy, Debug)]
pub struct Fade {
    level: f64,
    target: f64,
    speed: f64,
}

impl Fade {
    pub fn new(level: f64) -> Self {
        Fade {
            level,
            target: level,
            speed: 2.0,
        }
    }

    /// Затемнить за `seconds` секунд.
    pub fn to(&mut self, target: f64, seconds: f64) {
        self.target = clamp01(target);
        self.speed = 1.0 / seconds.max(1e-3);
    }

    pub fn update(&mut self, dt: f64) {
        let step = self.speed * dt;
        if (self.target - self.level).abs() <= step {
            self.level = self.target;
        } else {
            self.level += step.copysign(self.target - self.level);
        }
    }

    pub fn level(&self) -> f64 {
        self.level
    }

    pub fn at_target(&self) -> bool {
        (self.level - self.target).abs() < 1e-6
    }
}

/// Короткая белая вспышка — например, при смене фрактала.
#[derive(Clone, Copy, Debug, Default)]
pub struct Flash {
    level: f64,
    decay: f64,
}

impl Flash {
    pub fn new() -> Self {
        Flash {
            level: 0.0,
            decay: 3.0,
        }
    }

    pub fn trigger(&mut self, strength: f64, seconds: f64) {
        self.level = clamp01(strength);
        self.decay = 1.0 / seconds.max(1e-3);
    }

    pub fn update(&mut self, dt: f64) {
        self.level = (self.level - self.decay * dt).max(0.0);
    }

    pub fn level(&self) -> f64 {
        // Резкий спад выглядит естественнее линейного.
        self.level * self.level
    }
}

/// Медленное «дыхание» — синус, который добавляет живости зуму и палитре.
#[derive(Clone, Copy, Debug)]
pub struct Breathing {
    phase: f64,
    freq: f64,
    amp: f64,
}

impl Breathing {
    pub fn new(freq: f64, amp: f64) -> Self {
        Breathing {
            phase: 0.0,
            freq,
            amp,
        }
    }

    pub fn update(&mut self, dt: f64) -> f64 {
        self.phase += dt * self.freq * std::f64::consts::TAU;
        if self.phase > std::f64::consts::TAU {
            self.phase -= std::f64::consts::TAU;
        }
        self.value()
    }

    pub fn value(&self) -> f64 {
        self.phase.sin() * self.amp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_hit_endpoints() {
        for ease in [Ease::InOutCubic, Ease::InOutSine, Ease::Cinematic] {
            assert!((ease.apply(0.0) - 0.0).abs() < 1e-9, "{ease:?}");
            assert!((ease.apply(1.0) - 1.0).abs() < 1e-9, "{ease:?}");
        }
    }

    #[test]
    fn tween_reaches_target() {
        // У всех кривых движка середина пути приходится ровно на середину времени.
        let mut t = Tween::new(0.0, 10.0, 1.0, Ease::InOutCubic);
        t.update(0.5);
        assert!((t.value() - 5.0).abs() < 1e-9);
        t.update(0.6);
        assert!(t.finished());
        assert!((t.value() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn fade_converges() {
        let mut f = Fade::new(1.0);
        f.to(0.0, 0.5);
        for _ in 0..100 {
            f.update(0.016);
        }
        assert!(f.at_target());
        assert_eq!(f.level(), 0.0);
    }
}

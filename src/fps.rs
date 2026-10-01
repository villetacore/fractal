//! Замер времени кадра и его частей.

use std::time::{Duration, Instant};

/// Экспоненциальное сглаживание: мгновенный FPS слишком дёргается,
/// чтобы его можно было читать в HUD.
const SMOOTHING: f64 = 0.1;

pub struct Fps {
    last: Instant,
    frame_ms: f64,
    compute_ms: f64,
    present_ms: f64,
    frames: u64,
}

impl Fps {
    pub fn new() -> Self {
        Fps {
            last: Instant::now(),
            frame_ms: 16.0,
            compute_ms: 0.0,
            present_ms: 0.0,
            frames: 0,
        }
    }

    /// Начать новый кадр. Возвращает dt в секундах, ограниченный сверху —
    /// после паузы (например, окно свернули) анимации не должны прыгать.
    pub fn begin_frame(&mut self) -> f64 {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.frames += 1;
        let dt_ms = dt * 1000.0;
        self.frame_ms += (dt_ms - self.frame_ms) * SMOOTHING;
        dt.min(0.25)
    }

    pub fn record_compute(&mut self, d: Duration) {
        let ms = d.as_secs_f64() * 1000.0;
        self.compute_ms += (ms - self.compute_ms) * SMOOTHING;
    }

    pub fn record_present(&mut self, d: Duration) {
        let ms = d.as_secs_f64() * 1000.0;
        self.present_ms += (ms - self.present_ms) * SMOOTHING;
    }

    pub fn fps(&self) -> f64 {
        if self.frame_ms > 0.0 {
            1000.0 / self.frame_ms
        } else {
            0.0
        }
    }

    pub fn frame_ms(&self) -> f64 {
        self.frame_ms
    }

    pub fn compute_ms(&self) -> f64 {
        self.compute_ms
    }

    pub fn present_ms(&self) -> f64 {
        self.present_ms
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }
}

impl Default for Fps {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dt_is_clamped_and_counts_frames() {
        let mut f = Fps::new();
        let dt = f.begin_frame();
        assert!((0.0..=0.25).contains(&dt));
        assert_eq!(f.frames(), 1);
    }

    #[test]
    fn timings_converge() {
        let mut f = Fps::new();
        for _ in 0..500 {
            f.record_compute(Duration::from_millis(8));
        }
        assert!((f.compute_ms() - 8.0).abs() < 0.1, "{}", f.compute_ms());
    }
}

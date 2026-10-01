//! Мелкие математические помощники, цвет и генератор случайных чисел.

/// 24-битный цвет.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    pub fn lerp(a: Rgb, b: Rgb, t: f64) -> Rgb {
        let t = clamp01(t);
        Rgb {
            r: lerp(a.r as f64, b.r as f64, t).round() as u8,
            g: lerp(a.g as f64, b.g as f64, t).round() as u8,
            b: lerp(a.b as f64, b.b as f64, t).round() as u8,
        }
    }

    /// Умножение яркости (используется для fade и затемнения "внутренности").
    pub fn scale(self, k: f64) -> Rgb {
        let k = k.max(0.0);
        Rgb {
            r: (self.r as f64 * k).min(255.0) as u8,
            g: (self.g as f64 * k).min(255.0) as u8,
            b: (self.b as f64 * k).min(255.0) as u8,
        }
    }

    /// Подмешать белый (вспышка).
    pub fn towards_white(self, k: f64) -> Rgb {
        Rgb::lerp(self, Rgb::new(255, 255, 255), k)
    }

    /// Воспринимаемая яркость 0..1 — по ней выбирается символ из рампы.
    pub fn luma(self) -> f64 {
        (0.2126 * self.r as f64 + 0.7152 * self.g as f64 + 0.0722 * self.b as f64) / 255.0
    }

    /// Ближайший цвет из палитры xterm-256 (для терминалов без TrueColor).
    pub fn to_ansi256(self) -> u8 {
        // Серая шкала 232..255 — если канал почти одинаковый.
        let max = self.r.max(self.g).max(self.b) as i32;
        let min = self.r.min(self.g).min(self.b) as i32;
        if max - min < 8 {
            let level = (self.r as i32 + self.g as i32 + self.b as i32) / 3;
            if level < 8 {
                return 16;
            }
            if level > 248 {
                return 231;
            }
            return (232 + (level - 8) * 24 / 240) as u8;
        }
        let q = |c: u8| -> u32 {
            let c = c as u32;
            if c < 48 {
                0
            } else if c < 115 {
                1
            } else {
                (c - 35) / 40
            }
        };
        (16 + 36 * q(self.r) + 6 * q(self.g) + q(self.b)) as u8
    }
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn clamp01(t: f64) -> f64 {
    clamp(t, 0.0, 1.0)
}

pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// Дробная часть в диапазоне [0,1) — корректно работает с отрицательными.
pub fn fract(t: f64) -> f64 {
    let f = t - t.floor();
    if f.is_finite() { f } else { 0.0 }
}

/// xorshift64* — быстрый детерминированный ГПСЧ без зависимостей.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }

    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545F4914F6CDD1D);
        Rng::new(nanos)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Равномерно в [0,1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Равномерно в [lo,hi).
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

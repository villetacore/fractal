//! Палитры на 256 цветов, интерполяция между опорными точками и кроссфейд.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::util::{Rgb, fract};

pub const PALETTE_SIZE: usize = 256;

#[derive(Clone, Debug)]
pub struct Palette {
    pub name: String,
    colors: Vec<Rgb>,
}

impl Palette {
    /// Растянуть список опорных цветов на 256 позиций.
    pub fn from_stops(name: impl Into<String>, stops: &[Rgb]) -> Palette {
        let name = name.into();
        assert!(!stops.is_empty(), "палитра {name} без цветов");
        if stops.len() == 1 {
            return Palette {
                name,
                colors: vec![stops[0]; PALETTE_SIZE],
            };
        }
        let mut colors = Vec::with_capacity(PALETTE_SIZE);
        let segments = stops.len() - 1;
        for i in 0..PALETTE_SIZE {
            let pos = i as f64 / (PALETTE_SIZE - 1) as f64 * segments as f64;
            let idx = (pos.floor() as usize).min(segments - 1);
            colors.push(Rgb::lerp(stops[idx], stops[idx + 1], pos - idx as f64));
        }
        Palette { name, colors }
    }

    /// Цвет по нормированной позиции. Значение зациклено, поэтому смещение
    /// палитры во времени даёт бесконечную «бегущую» раскраску.
    pub fn sample(&self, t: f64) -> Rgb {
        let pos = fract(t) * PALETTE_SIZE as f64;
        let i = pos.floor() as usize % PALETTE_SIZE;
        let j = (i + 1) % PALETTE_SIZE;
        Rgb::lerp(self.colors[i], self.colors[j], pos - pos.floor())
    }

    /// Цвет «внутренности» множества — самый тёмный тон палитры.
    pub fn interior(&self) -> Rgb {
        self.colors[0].scale(0.35)
    }
}

/// Пара палитр и коэффициент перехода между ними: то, что реально видит рендерер.
#[derive(Clone, Copy)]
pub struct Blend<'a> {
    pub from: &'a Palette,
    pub to: &'a Palette,
    /// 0.0 — целиком `from`, 1.0 — целиком `to`.
    pub mix: f64,
}

impl<'a> Blend<'a> {
    pub fn single(p: &'a Palette) -> Blend<'a> {
        Blend {
            from: p,
            to: p,
            mix: 0.0,
        }
    }

    pub fn sample(&self, t: f64) -> Rgb {
        if self.mix <= 0.0 {
            self.from.sample(t)
        } else if self.mix >= 1.0 {
            self.to.sample(t)
        } else {
            Rgb::lerp(self.from.sample(t), self.to.sample(t), self.mix)
        }
    }

    pub fn interior(&self) -> Rgb {
        Rgb::lerp(self.from.interior(), self.to.interior(), self.mix)
    }

    /// Имя ведущей палитры. Живёт столько же, сколько сами палитры,
    /// а не временный `Blend`.
    pub fn name(&self) -> &'a str {
        if self.mix < 0.5 {
            &self.from.name
        } else {
            &self.to.name
        }
    }

    /// «Заморозить» текущее состояние перехода в самостоятельную палитру —
    /// нужно, чтобы начать новый кроссфейд из середины предыдущего.
    pub fn materialize(&self) -> Palette {
        let colors = (0..PALETTE_SIZE)
            .map(|i| self.sample(i as f64 / PALETTE_SIZE as f64))
            .collect();
        Palette {
            name: self.name().to_string(),
            colors,
        }
    }
}

/// Формат assets/palettes.json: `{ "fire": [[r,g,b], ...] }`.
#[derive(Deserialize)]
#[serde(transparent)]
struct PaletteFile(BTreeMap<String, Vec<[u8; 3]>>);

/// Набор палитр, отсортированный по имени — в этом же порядке их листает `P`.
pub fn load_from_json(src: &str) -> Result<Vec<Palette>, String> {
    let parsed: PaletteFile = serde_json::from_str(src).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (name, stops) in parsed.0 {
        if stops.is_empty() {
            return Err(format!("палитра «{name}» не содержит цветов"));
        }
        let stops: Vec<Rgb> = stops.iter().map(|c| Rgb::new(c[0], c[1], c[2])).collect();
        out.push(Palette::from_stops(name, &stops));
    }
    if out.is_empty() {
        return Err("в файле нет ни одной палитры".into());
    }
    Ok(out)
}

/// Запасной набор — используется, если assets/palettes.json недоступен.
pub fn builtin() -> Vec<Palette> {
    load_from_json(include_str!("../assets/palettes.json"))
        .expect("встроенный palettes.json должен быть корректным")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_palettes_parse() {
        let p = builtin();
        assert!(p.len() >= 8);
        // Палитра обязана меняться по длине, иначе это просто заливка.
        for palette in &p {
            let a = palette.sample(0.05);
            let b = palette.sample(0.85);
            assert!(a != b, "палитра «{}» одноцветная", palette.name);
        }
    }

    #[test]
    fn sample_wraps() {
        let p = Palette::from_stops("t", &[Rgb::new(0, 0, 0), Rgb::new(255, 255, 255)]);
        assert_eq!(p.sample(0.25), p.sample(1.25));
        assert_eq!(p.sample(0.25), p.sample(-0.75));
    }

    #[test]
    fn blend_endpoints() {
        let a = Palette::from_stops("a", &[Rgb::new(0, 0, 0)]);
        let b = Palette::from_stops("b", &[Rgb::new(255, 255, 255)]);
        let blend = Blend {
            from: &a,
            to: &b,
            mix: 1.0,
        };
        assert_eq!(blend.sample(0.5), Rgb::new(255, 255, 255));
        assert_eq!(blend.materialize().sample(0.3), Rgb::new(255, 255, 255));
    }
}

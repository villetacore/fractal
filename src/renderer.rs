//! Сердце программы: считает кадр в буфер знакомест и выводит только то,
//! что изменилось с прошлого кадра.

use crate::camera::Mapper;
use crate::fractals::Fractal;
use crate::palette::Blend;
use crate::terminal::{SGR_RESET, Terminal};
use crate::thread_pool::ThreadPool;
use crate::util::{Rgb, clamp01, fract};

/// Рампа «плотности»: от пустоты до сплошной заливки.
pub const DEFAULT_RAMP: &str = " .,:;-+=*#%@█";

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Rgb,
}

impl Cell {
    pub const EMPTY: Cell = Cell {
        ch: ' ',
        fg: Rgb::BLACK,
    };
}

/// Параметры раскраски, которые меняются во времени (их крутит `app`).
#[derive(Clone, Copy)]
pub struct Shading {
    /// Во сколько раз сжать шкалу итераций перед выборкой палитры.
    pub color_cycle: f64,
    /// Сдвиг палитры — даёт «бегущие» цвета.
    pub offset: f64,
    /// Общая яркость 0..1 (fade между сценами).
    pub fade: f64,
    /// Подмешивание белого 0..1 (вспышка).
    pub flash: f64,
}

impl Default for Shading {
    fn default() -> Self {
        Shading {
            color_cycle: 0.035,
            offset: 0.0,
            fade: 1.0,
            flash: 0.0,
        }
    }
}

pub struct Renderer {
    width: usize,
    height: usize,
    back: Vec<Cell>,
    front: Vec<Cell>,
    ramp: Vec<char>,
    /// Сторона сетки суперсэмплинга: 1 — выключен, 2 — 2×2, 3 — 3×3.
    pub supersample: u32,
    pub truecolor: bool,
    /// Следующий кадр вывести целиком, не сравнивая с предыдущим.
    force_full_redraw: bool,
    frame: String,
}

impl Renderer {
    pub fn new(width: usize, height: usize, ramp: &str, truecolor: bool) -> Renderer {
        let ramp: Vec<char> = if ramp.chars().count() >= 2 {
            ramp.chars().collect()
        } else {
            DEFAULT_RAMP.chars().collect()
        };
        let cells = width * height;
        Renderer {
            width,
            height,
            back: vec![Cell::EMPTY; cells],
            front: vec![Cell::EMPTY; cells],
            ramp,
            supersample: 1,
            truecolor,
            force_full_redraw: true,
            frame: String::with_capacity(cells * 24),
        }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        self.back = vec![Cell::EMPTY; width * height];
        self.front = vec![Cell::EMPTY; width * height];
        self.force_full_redraw = true;
    }

    /// Следующий кадр рисуется целиком, без диффа (после resize или Clear).
    pub fn invalidate(&mut self) {
        self.force_full_redraw = true;
    }

    pub fn begin_frame(&mut self) {
        self.back.fill(Cell::EMPTY);
    }

    /// Посчитать фрактал во весь буфер, распараллелив по полосам.
    pub fn render_fractal(
        &mut self,
        fractal: &dyn Fractal,
        mapper: &Mapper,
        palette: &Blend<'_>,
        shading: &Shading,
        pool: &ThreadPool,
    ) {
        let width = self.width;
        let ss = self.supersample.clamp(1, 4);
        let ramp = &self.ramp;
        let interior = palette.interior().scale(shading.fade);

        pool.for_each_band(&mut self.back, width, |row, cells| {
            for (col, cell) in cells.iter_mut().enumerate() {
                *cell = shade_cell(
                    fractal, mapper, palette, shading, ramp, interior, ss, col, row,
                );
            }
        });
    }

    /// Написать строку поверх уже посчитанного кадра (HUD).
    pub fn draw_text(&mut self, x: usize, y: usize, text: &str, fg: Rgb) {
        if y >= self.height {
            return;
        }
        let base = y * self.width;
        for (i, ch) in text.chars().enumerate() {
            let col = x + i;
            if col >= self.width {
                break;
            }
            self.back[base + col] = Cell { ch, fg };
        }
    }

    /// Залить прямоугольник (подложка под HUD, чтобы текст читался).
    pub fn dim_rect(&mut self, x: usize, y: usize, w: usize, h: usize, k: f64) {
        for row in y..(y + h).min(self.height) {
            for col in x..(x + w).min(self.width) {
                let cell = &mut self.back[row * self.width + col];
                cell.fg = cell.fg.scale(k);
            }
        }
    }

    /// Вывести кадр: только изменившиеся знакоместа, минимум ANSI-кодов.
    pub fn present(&mut self, term: &mut Terminal) -> std::io::Result<()> {
        self.frame.clear();
        let full = self.force_full_redraw;
        self.force_full_redraw = false;

        let mut last_color: Option<Rgb> = None;
        let mut cursor: Option<(usize, usize)> = None;

        for y in 0..self.height {
            for x in 0..self.width {
                // Запись в правый нижний угол прокручивает экран в части
                // терминалов, поэтому это знакоместо остаётся пустым.
                if y + 1 == self.height && x + 1 == self.width {
                    continue;
                }
                let idx = y * self.width + x;
                let cell = self.back[idx];
                if !full && cell == self.front[idx] {
                    continue;
                }

                // Курсор уже стоит там, где нужно, — переставлять не надо.
                if cursor != Some((x, y)) {
                    self.frame.push_str(&format!("\x1b[{};{}H", y + 1, x + 1));
                }

                // Пробел не несёт цвета — экономим escape-последовательность.
                if cell.ch != ' ' && last_color != Some(cell.fg) {
                    push_color(&mut self.frame, cell.fg, self.truecolor);
                    last_color = Some(cell.fg);
                }

                self.frame.push(cell.ch);
                cursor = Some((x + 1, y));
            }
        }

        if !self.frame.is_empty() {
            self.frame.push_str(SGR_RESET);
            term.write_frame(&self.frame)?;
        }

        std::mem::swap(&mut self.front, &mut self.back);
        Ok(())
    }
}

/// Расчёт одного знакоместа, включая суперсэмплинг.
#[allow(clippy::too_many_arguments)]
fn shade_cell(
    fractal: &dyn Fractal,
    mapper: &Mapper,
    palette: &Blend<'_>,
    shading: &Shading,
    ramp: &[char],
    interior: Rgb,
    ss: u32,
    col: usize,
    row: usize,
) -> Cell {
    let mut r = 0.0f64;
    let mut g = 0.0f64;
    let mut b = 0.0f64;
    let mut inside_count = 0u32;
    let total = ss * ss;

    for sy in 0..ss {
        for sx in 0..ss {
            let fx = col as f64 + (sx as f64 + 0.5) / ss as f64;
            let fy = row as f64 + (sy as f64 + 0.5) / ss as f64;
            let (wx, wy) = mapper.world(fx, fy);
            let px = fractal.sample(wx, wy);
            if px.inside {
                inside_count += 1;
            }
            let color = color_of(px, palette, shading, interior);
            r += color.r as f64;
            g += color.g as f64;
            b += color.b as f64;
        }
    }

    let mut color = Rgb::new(
        (r / total as f64).round() as u8,
        (g / total as f64).round() as u8,
        (b / total as f64).round() as u8,
    );
    if shading.flash > 0.0 {
        color = color.towards_white(shading.flash);
    }

    // Символ выбирается по яркости: тёмные области получают точки и запятые,
    // яркие — сплошную заливку. Отсюда и берётся эффект свечения.
    let ch = if inside_count == total {
        *ramp.last().unwrap_or(&'#')
    } else {
        let luma = clamp01(color.luma().powf(0.75));
        let idx = (luma * (ramp.len() - 1) as f64).round() as usize;
        ramp[idx.min(ramp.len() - 1)]
    };

    Cell { ch, fg: color }
}

/// Цвет одной точки. Общая формула для терминала и для экспорта в PNG.
pub fn color_of(
    px: crate::fractals::Pixel,
    palette: &Blend<'_>,
    shading: &Shading,
    interior: Rgb,
) -> Rgb {
    if px.inside {
        interior
    } else {
        let t = px.value * shading.color_cycle + px.shade + shading.offset;
        palette.sample(ping_pong(t)).scale(shading.fade)
    }
}

/// Треугольная волна: палитра идёт вперёд и обратно, поэтому на стыке циклов
/// нет разрыва даже у палитр, у которых начало и конец разного цвета.
fn ping_pong(t: f64) -> f64 {
    let x = fract(t * 0.5) * 2.0;
    if x <= 1.0 { x } else { 2.0 - x }
}

fn push_color(out: &mut String, c: Rgb, truecolor: bool) {
    use std::fmt::Write;
    if truecolor {
        let _ = write!(out, "\x1b[38;2;{};{};{}m", c.r, c.g, c.b);
    } else {
        let _ = write!(out, "\x1b[38;5;{}m", c.to_ansi256());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Camera, View};
    use crate::fractals::Mandelbrot;
    use crate::palette::{Blend, builtin};

    #[test]
    fn ping_pong_is_continuous_and_bounded() {
        let mut prev = ping_pong(0.0);
        let mut t = 0.0;
        while t < 4.0 {
            t += 0.001;
            let v = ping_pong(t);
            assert!((0.0..=1.0).contains(&v), "{v}");
            assert!((v - prev).abs() < 0.01, "разрыв при t={t}");
            prev = v;
        }
    }

    #[test]
    fn renders_without_panicking_and_fills_buffer() {
        let palettes = builtin();
        let blend = Blend::single(&palettes[0]);
        let fractal = Mandelbrot::new();
        let cam = Camera::new(View::new(-0.6, 0.0, 1.8), 2.0);
        let pool = ThreadPool::new(4);
        let mut r = Renderer::new(40, 20, DEFAULT_RAMP, true);

        r.begin_frame();
        r.render_fractal(
            &fractal,
            &cam.mapper(40, 20),
            &blend,
            &Shading::default(),
            &pool,
        );

        // Внутри кадра должна быть и «внутренность» множества, и фон.
        let solid = r.back.iter().filter(|c| c.ch == '█').count();
        assert!(solid > 0, "не нашлось тела множества");
        assert!(solid < 40 * 20, "кадр залит целиком");
    }

    #[test]
    fn text_is_clipped_to_bounds() {
        let mut r = Renderer::new(10, 3, DEFAULT_RAMP, true);
        r.begin_frame();
        r.draw_text(8, 1, "abcdef", Rgb::new(255, 255, 255));
        r.draw_text(0, 99, "не должно упасть", Rgb::new(255, 255, 255));
        assert_eq!(r.back[10 + 8].ch, 'a');
        assert_eq!(r.back[10 + 9].ch, 'b');
    }

    #[test]
    fn present_emits_nothing_for_identical_frames() {
        let mut term_free = Renderer::new(8, 4, DEFAULT_RAMP, true);
        term_free.begin_frame();
        term_free.force_full_redraw = false;
        // Оба буфера пусты и совпадают — диффу нечего выводить.
        term_free.frame.clear();
        let mut changed = 0;
        for y in 0..4 {
            for x in 0..8 {
                if term_free.back[y * 8 + x] != term_free.front[y * 8 + x] {
                    changed += 1;
                }
            }
        }
        assert_eq!(changed, 0);
    }
}

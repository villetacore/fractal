//! Тонкая обёртка над crossterm: raw-режим, альтернативный экран, размер,
//! буферизованный вывод. Всё, что знает про конкретный терминал, живёт здесь.

use std::io::{self, BufWriter, Stdout, Write};

use crossterm::{
    cursor, execute, queue,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode, size,
    },
};

/// Сброс цвета и атрибутов. crossterm не всегда сбрасывает фон одинаково
/// на разных платформах, поэтому пишем классический SGR 0 напрямую.
pub const SGR_RESET: &str = "\x1b[0m";

pub struct Terminal {
    out: BufWriter<Stdout>,
    width: usize,
    height: usize,
    active: bool,
}

impl Terminal {
    /// Перейти в альтернативный экран, спрятать курсор, включить raw-режим.
    pub fn enter() -> io::Result<Terminal> {
        enable_raw_mode()?;
        let mut out = BufWriter::with_capacity(1 << 20, io::stdout());
        execute!(
            out,
            EnterAlternateScreen,
            cursor::Hide,
            Clear(ClearType::All)
        )?;
        out.flush()?;
        let (w, h) = size().unwrap_or((80, 24));
        Ok(Terminal {
            out,
            width: (w as usize).max(1),
            height: (h as usize).max(1),
            active: true,
        })
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Перечитать размер окна. Возвращает `true`, если он изменился.
    pub fn refresh_size(&mut self) -> bool {
        let (w, h) = size()
            .map(|(w, h)| ((w as usize).max(1), (h as usize).max(1)))
            .unwrap_or((self.width, self.height));
        if w != self.width || h != self.height {
            self.width = w;
            self.height = h;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) -> io::Result<()> {
        queue!(self.out, Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        self.out.flush()
    }

    /// Записать уже собранный кадр (со всеми ANSI-кодами внутри).
    pub fn write_frame(&mut self, frame: &str) -> io::Result<()> {
        self.out.write_all(frame.as_bytes())?;
        self.out.flush()
    }

    /// Вернуть терминал в исходное состояние. Идемпотентно.
    pub fn leave(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        self.out.write_all(SGR_RESET.as_bytes())?;
        execute!(self.out, cursor::Show, LeaveAlternateScreen)?;
        self.out.flush()?;
        disable_raw_mode()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

/// Аварийное восстановление из panic-хука, когда до `Drop` дело не дойдёт.
pub fn force_restore() {
    let mut out = io::stdout();
    let _ = out.write_all(SGR_RESET.as_bytes());
    let _ = execute!(out, cursor::Show, LeaveAlternateScreen);
    let _ = out.flush();
    let _ = disable_raw_mode();
}

/// Поддерживает ли терминал 24-битный цвет. Виндовый терминал и все
/// современные эмуляторы — да; проверяем переменные окружения как эвристику.
pub fn detect_truecolor() -> bool {
    if let Ok(v) = std::env::var("COLORTERM") {
        let v = v.to_ascii_lowercase();
        if v.contains("truecolor") || v.contains("24bit") {
            return true;
        }
    }
    if let Ok(v) = std::env::var("TERM")
        && (v.contains("256color") || v.contains("direct"))
    {
        return true;
    }
    // Windows Terminal / ConHost c включённым VT — TrueColor работает.
    cfg!(windows)
}

//! Клавиатура → команды приложения.

use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Quit,
    NextFractal,
    PrevFractal,
    NextPalette,
    PrevPalette,
    /// Прыгнуть к следующей точке из cameras.json.
    NextScene,
    /// Найти новое интересное место автоматически.
    Discover,
    ToggleAutopilot,
    ToggleHud,
    ToggleSupersample,
    ToggleTruecolor,
    /// Сдвиг в долях кадра.
    Pan(f64, f64),
    /// Умножение зума: < 1 — приблизить.
    Zoom(f64),
    Rotate(f64),
    IterScale(f64),
    Screenshot,
    Reset,
    Redraw,
    Resized,
}

/// Собрать все события, накопившиеся за `timeout`.
pub fn poll(timeout: Duration) -> std::io::Result<Vec<Action>> {
    let mut actions = Vec::new();
    if !event::poll(timeout)? {
        return Ok(actions);
    }
    while event::poll(Duration::ZERO)? {
        match event::read()? {
            Event::Key(key) => {
                // Windows шлёт и Press, и Release — иначе всё сработает дважды.
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if let Some(action) = map_key(key) {
                    actions.push(action);
                }
            }
            Event::Resize(..) => actions.push(Action::Resized),
            _ => {}
        }
    }
    Ok(actions)
}

fn map_key(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    const STEP: f64 = 0.12;
    const SPIN: f64 = 0.08;

    Some(match key.code {
        KeyCode::Char('c' | 'C') if ctrl => Action::Quit,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => Action::Quit,

        KeyCode::Tab => Action::NextFractal,
        KeyCode::BackTab => Action::PrevFractal,

        KeyCode::Char('w' | 'W') | KeyCode::Up => Action::Pan(0.0, -STEP),
        KeyCode::Char('s' | 'S') | KeyCode::Down => Action::Pan(0.0, STEP),
        KeyCode::Char('a' | 'A') | KeyCode::Left => Action::Pan(-STEP, 0.0),
        KeyCode::Char('d' | 'D') | KeyCode::Right => Action::Pan(STEP, 0.0),

        KeyCode::Char('+' | '=') | KeyCode::PageUp => Action::Zoom(0.8),
        KeyCode::Char('-' | '_') | KeyCode::PageDown => Action::Zoom(1.25),

        KeyCode::Char('e' | 'E') => Action::Rotate(SPIN),
        KeyCode::Char('r') => Action::Rotate(-SPIN),
        KeyCode::Char('R') => Action::Reset,

        KeyCode::Char('p') => Action::NextPalette,
        KeyCode::Char('P') => Action::PrevPalette,

        KeyCode::Char('n' | 'N') => Action::NextScene,
        KeyCode::Char('f' | 'F') => Action::Discover,
        KeyCode::Char(' ') => Action::ToggleAutopilot,

        KeyCode::F(1) | KeyCode::Char('h' | 'H') => Action::ToggleHud,
        KeyCode::Char('x' | 'X') => Action::ToggleSupersample,
        KeyCode::Char('t' | 'T') => Action::ToggleTruecolor,
        KeyCode::Char('o' | 'O') => Action::Screenshot,
        KeyCode::Char('l' | 'L') => Action::Redraw,

        KeyCode::Char('[') => Action::IterScale(1.0 / 1.35),
        KeyCode::Char(']') => Action::IterScale(1.35),

        KeyCode::Char('0') => Action::Reset,
        _ => return None,
    })
}

/// Список подсказок для HUD.
pub const HELP: &str = "WASD пан  ±зум  R/E поворот  Tab фрактал  P палитра  N сцена  F поиск  Space автопилот  X сглаживание  O кадр  F1 HUD  Esc выход";

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn basic_bindings() {
        assert_eq!(map_key(key(KeyCode::Esc)), Some(Action::Quit));
        assert_eq!(map_key(key(KeyCode::Tab)), Some(Action::NextFractal));
        assert_eq!(
            map_key(key(KeyCode::Char(' '))),
            Some(Action::ToggleAutopilot)
        );
        assert_eq!(map_key(key(KeyCode::F(1))), Some(Action::ToggleHud));
        assert!(matches!(
            map_key(key(KeyCode::Char('w'))),
            Some(Action::Pan(_, _))
        ));
        assert!(matches!(map_key(key(KeyCode::Char('+'))), Some(Action::Zoom(z)) if z < 1.0));
        assert!(map_key(key(KeyCode::Char('§'))).is_none());
    }

    #[test]
    fn ctrl_c_quits() {
        let k = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(k), Some(Action::Quit));
    }

    #[test]
    fn case_distinguishes_palette_direction() {
        assert_eq!(map_key(key(KeyCode::Char('p'))), Some(Action::NextPalette));
        assert_eq!(map_key(key(KeyCode::Char('P'))), Some(Action::PrevPalette));
    }
}

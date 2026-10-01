//! Главный цикл: ввод → камера → анимации → расчёт → вывод.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::animation::{Ease, Fade, Flash, Tween};
use crate::camera::{self, Camera, Mapper, Preset, View};
use crate::config::{Cli, Config};
use crate::export;
use crate::fps::Fps;
use crate::fractals::{self, Fractal};
use crate::input::{self, Action};
use crate::palette::{self, Blend, Palette};
use crate::renderer::{Renderer, Shading};
use crate::scout;
use crate::terminal::{self, Terminal};
use crate::thread_pool::ThreadPool;
use crate::util::{Rgb, Rng, clamp, fract};

/// Что сейчас делает автопилот.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// Стоим в точке и медленно дрейфуем.
    Hold,
    /// Летим к следующей точке.
    Flight,
    /// Гасим экран перед сменой фрактала.
    Switching,
}

/// Куда автопилот собирается лететь после затемнения.
struct Pending {
    fractal: usize,
    view: View,
    name: String,
    hold: f64,
    flight: f64,
}

pub struct App {
    cfg: Config,
    term: Terminal,
    renderer: Renderer,
    pool: ThreadPool,

    fractals: Vec<Box<dyn Fractal>>,
    current: usize,

    palettes: Vec<Palette>,
    palette_index: usize,
    palette_from: Palette,
    palette_to: Palette,
    /// Кроссфейд между `palette_from` и `palette_to`.
    palette_mix: Tween,

    camera: Camera,
    presets: Vec<Preset>,
    preset_index: usize,
    scene_name: String,

    phase: Phase,
    pending: Option<Pending>,
    hold_left: f64,

    fade: Fade,
    flash: Flash,
    shading: Shading,
    iter_scale: f64,

    fps: Fps,
    hud: bool,
    autopilot: bool,
    rng: Rng,
    status: Option<(String, f64)>,
    max_frames: Option<u64>,
    shots: u64,
}

impl App {
    pub fn new(cli: Cli) -> Result<App, String> {
        let cfg = cli.config;

        let palettes = load_palettes(&cfg);
        let presets = load_presets(&cfg);

        let mut fractals = fractals::all();
        let current = match &cli.fractal {
            Some(id) => fractals::index_of(&fractals, id)
                .ok_or_else(|| format!("неизвестный фрактал: {id}"))?,
            None => 0,
        };

        let palette_index = match &cli.palette {
            Some(name) => palettes
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("неизвестная палитра: {name}"))?,
            None => 0,
        };

        let term = Terminal::enter().map_err(|e| format!("терминал: {e}"))?;
        let (w, h) = term.size();

        let truecolor = cfg.truecolor.unwrap_or_else(terminal::detect_truecolor);
        let mut renderer = Renderer::new(w, h, &cfg.char_ramp, truecolor);
        renderer.supersample = cfg.supersample.clamp(1, 4);

        let start_view = cli.view.unwrap_or_else(|| {
            let h = fractals[current].home();
            View::new(h.x, h.y, h.zoom)
        });
        let camera = Camera::new(start_view, cfg.char_aspect);

        fractals[current].set_max_iter(cfg.max_iter);

        let shading = Shading {
            color_cycle: cfg.color_cycle,
            ..Shading::default()
        };

        Ok(App {
            palette_from: palettes[palette_index].clone(),
            palette_to: palettes[palette_index].clone(),
            palette_mix: Tween::new(1.0, 1.0, 1.0, Ease::InOutSine),
            palette_index,
            palettes,
            pool: ThreadPool::new(cfg.threads),
            hud: cfg.hud,
            autopilot: cfg.autopilot,
            iter_scale: 1.0,
            hold_left: cfg.scene_seconds,
            max_frames: cli.frames,
            cfg,
            term,
            renderer,
            fractals,
            current,
            camera,
            presets,
            preset_index: 0,
            scene_name: "Home".to_string(),
            phase: Phase::Hold,
            pending: None,
            fade: Fade::new(1.0),
            flash: Flash::new(),
            shading,
            fps: Fps::new(),
            rng: Rng::from_time(),
            status: None,
            shots: 0,
        })
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        let target = if self.cfg.fps == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(1.0 / self.cfg.fps as f64)
        };

        loop {
            let frame_start = Instant::now();
            let dt = self.fps.begin_frame();

            self.update(dt);

            let t0 = Instant::now();
            self.draw();
            self.fps.record_compute(t0.elapsed());

            let t1 = Instant::now();
            self.renderer.present(&mut self.term)?;
            self.fps.record_present(t1.elapsed());

            if self.cfg.record {
                self.write_frame_png();
            }

            if self
                .max_frames
                .is_some_and(|limit| self.fps.frames() >= limit)
            {
                return Ok(());
            }

            // Остаток кадра тратим на ожидание клавиш — вместо «пустого» sleep.
            let budget = target.saturating_sub(frame_start.elapsed());
            let actions = input::poll(budget)?;
            if !self.handle(&actions) {
                return Ok(());
            }
        }
    }

    // ---- обновление состояния -------------------------------------------

    fn update(&mut self, dt: f64) {
        self.fade.update(dt);
        self.flash.update(dt);
        self.camera.update(dt);
        self.fractals[self.current].tick(dt);

        if !self.palette_mix.finished() {
            self.palette_mix.update(dt);
        }

        self.shading.offset = fract(self.shading.offset + self.cfg.palette_speed * dt);
        self.shading.fade = self.fade.level();
        self.shading.flash = self.flash.level();

        if let Some((_, left)) = &mut self.status {
            *left -= dt;
            if *left <= 0.0 {
                self.status = None;
            }
        }

        // Бюджет итераций зависит от глубины: у поверхности он не нужен,
        // а на глубине без него множество превращается в чёрное пятно.
        let base = (self.cfg.max_iter as f64 * self.iter_scale).clamp(16.0, 20000.0) as u32;
        let zoom = self.camera.zoom();
        let iters = self.fractals[self.current].iters_for_zoom(zoom, base);
        self.fractals[self.current].set_max_iter(iters);

        if self.autopilot {
            self.update_autopilot(dt);
        }
    }

    fn update_autopilot(&mut self, dt: f64) {
        match self.phase {
            Phase::Hold => {
                self.camera
                    .drift(dt, self.cfg.drift_zoom, self.cfg.drift_spin);
                self.hold_left -= dt;
                if self.hold_left <= 0.0 {
                    self.begin_next_scene();
                }
            }
            Phase::Flight => {
                if !self.camera.is_flying() {
                    self.phase = Phase::Hold;
                }
            }
            Phase::Switching => {
                // Ждём полного затемнения, меняем фрактал и въезжаем обратно.
                if self.fade.at_target() {
                    if let Some(pending) = self.pending.take() {
                        self.current = pending.fractal;
                        self.fractals[self.current].set_max_iter(self.cfg.max_iter);

                        // Появляемся заметно выше цели и ныряем к ней.
                        let mut entry = pending.view;
                        entry.zoom = (entry.zoom * 400.0).min(crate::camera::MAX_ZOOM);
                        self.camera.set_view(entry);
                        self.camera.fly_to(pending.view, pending.flight);

                        self.scene_name = pending.name;
                        self.hold_left = pending.hold;
                        self.phase = Phase::Flight;
                        self.fade.to(1.0, 0.8);
                        self.flash.trigger(0.35, 0.5);
                        self.camera.kick(0.015, 0.4);
                        self.renderer.invalidate();
                    } else {
                        self.phase = Phase::Hold;
                        self.fade.to(1.0, 0.8);
                    }
                }
            }
        }
    }

    /// Выбрать следующую сцену: либо из списка, либо найти самому.
    fn begin_next_scene(&mut self) {
        let discover = self.rng.next_f64() < self.cfg.discovery_ratio;

        let (fractal_idx, view, name, hold, flight) = if discover {
            let idx = self.rng.below(self.fractals.len());
            let home = self.fractals[idx].home();
            let start = View::new(home.x, home.y, home.zoom);
            let depth = 5 + self.rng.below(4) as u32;
            // Разведчик отличает границу от фона по числу итераций, поэтому
            // считать её нужно с запасом: иначе на глубине всё выглядит фоном.
            self.fractals[idx].set_max_iter(self.cfg.max_iter.max(500));
            let found = scout::find_interesting(&*self.fractals[idx], start, depth, &mut self.rng);
            (
                idx,
                found,
                "Uncharted".to_string(),
                self.cfg.scene_seconds,
                self.cfg.flight_seconds,
            )
        } else {
            let Some((idx, preset)) = self.next_preset() else {
                self.hold_left = self.cfg.scene_seconds;
                return;
            };
            (
                idx,
                preset.view(),
                preset.name.clone(),
                preset.hold.unwrap_or(self.cfg.scene_seconds),
                preset.flight.unwrap_or(self.cfg.flight_seconds),
            )
        };

        self.cycle_palette(1, self.cfg.flight_seconds);

        if fractal_idx == self.current {
            self.camera.fly_to(view, flight);
            self.scene_name = name;
            self.hold_left = hold;
            self.phase = Phase::Flight;
        } else {
            self.pending = Some(Pending {
                fractal: fractal_idx,
                view,
                name,
                hold,
                flight,
            });
            self.fade.to(0.0, 0.7);
            self.phase = Phase::Switching;
        }
    }

    /// Следующая точка из cameras.json, у которой известен фрактал.
    fn next_preset(&mut self) -> Option<(usize, Preset)> {
        for _ in 0..self.presets.len() {
            self.preset_index = (self.preset_index + 1) % self.presets.len();
            let preset = self.presets[self.preset_index].clone();
            if let Some(idx) = fractals::index_of(&self.fractals, &preset.fractal) {
                return Some((idx, preset));
            }
        }
        None
    }

    fn cycle_palette(&mut self, step: isize, seconds: f64) {
        let len = self.palettes.len();
        if len == 0 {
            return;
        }
        let next = (self.palette_index as isize + step).rem_euclid(len as isize) as usize;
        self.set_palette(next, seconds);
    }

    fn set_palette(&mut self, index: usize, seconds: f64) {
        // Замораживаем текущее состояние перехода, чтобы новый кроссфейд
        // начался ровно с того цвета, который сейчас на экране.
        let snapshot = self.current_blend().materialize();
        self.palette_from = snapshot;
        self.palette_to = self.palettes[index].clone();
        self.palette_index = index;
        self.palette_mix = Tween::new(0.0, 1.0, clamp(seconds, 0.2, 30.0), Ease::InOutSine);
    }

    /// Текущая пара палитр — то, чем раскрашивается кадр.
    ///
    /// Свободная функция, а не метод: иначе `Blend` заимствовал бы `self`
    /// целиком и рендерер нельзя было бы взять по `&mut` в том же выражении.
    fn blend<'a>(from: &'a Palette, to: &'a Palette, mix: f64) -> Blend<'a> {
        if mix >= 1.0 {
            Blend::single(to)
        } else {
            Blend { from, to, mix }
        }
    }

    fn current_blend(&self) -> Blend<'_> {
        Self::blend(
            &self.palette_from,
            &self.palette_to,
            self.palette_mix.value(),
        )
    }

    // ---- рисование -------------------------------------------------------

    fn draw(&mut self) {
        let (w, h) = self.renderer.dimensions();
        let mapper = self.camera.mapper(w, h);
        let blend = Self::blend(
            &self.palette_from,
            &self.palette_to,
            self.palette_mix.value(),
        );

        self.renderer.begin_frame();
        self.renderer.render_fractal(
            &*self.fractals[self.current],
            &mapper,
            &blend,
            &self.shading,
            &self.pool,
        );

        if self.hud {
            self.draw_hud(&mapper);
        }
    }

    /// HUD рисуется поверх уже посчитанного кадра.
    /// `mapper` тот же, что и у кадра, — второй раз его строить незачем.
    fn draw_hud(&mut self, mapper: &Mapper) {
        let (w, h) = self.renderer.dimensions();
        if w < 20 || h < 4 {
            return;
        }

        let fractal = self.fractals[self.current].name();
        let iters = self.fractals[self.current].max_iter();
        let zoom = self.camera.zoom();
        let mode = if self.autopilot { "auto" } else { "manual" };
        let ss = self.renderer.supersample;

        let line1 = format!(
            "{fractal}  {w}x{h}  {:.0} FPS  {} потоков  [{mode}]",
            self.fps.fps(),
            self.pool.threads(),
        );
        let line2 = format!(
            "{}  zoom {:.2e}  iter {iters}  {}  палитра {}",
            self.scene_name,
            zoom,
            if ss > 1 {
                format!("SS {ss}×{ss}")
            } else {
                "SS off".to_string()
            },
            self.palette_name(),
        );

        let mut line3 = format!(
            "кадр {:.1} мс  расчёт {:.1} мс  вывод {:.1} мс",
            self.fps.frame_ms(),
            self.fps.compute_ms(),
            self.fps.present_ms(),
        );
        let info = self.fractals[self.current].info();
        if !info.is_empty() {
            line3.push_str("  ");
            line3.push_str(&info);
        }
        // Ниже примерно 1e-15 на пиксель double уже не различает соседние точки,
        // и картинка начинает распадаться на «ступеньки».
        let (step_x, _) = mapper.pixel_size();
        if step_x < 1e-15 {
            line3.push_str("  ⚠ предел точности f64");
        }

        // Три верхние строки притемняются, иначе текст тонет в картинке.
        self.renderer.dim_rect(0, 0, w, 3, 0.25);

        let bright = Rgb::new(235, 240, 245);
        let dim = Rgb::new(150, 160, 175);
        self.renderer.draw_text(1, 0, &line1, bright);
        self.renderer.draw_text(1, 1, &line2, dim);
        self.renderer.draw_text(1, 2, &line3, dim);

        if let Some((msg, _)) = &self.status {
            let msg = msg.clone();
            self.renderer.dim_rect(0, h - 2, w, 1, 0.2);
            self.renderer
                .draw_text(1, h - 2, &msg, Rgb::new(255, 220, 120));
        }

        if h > 4 && w > 40 {
            let help: String = input::HELP.chars().take(w.saturating_sub(2)).collect();
            self.renderer.dim_rect(0, h - 1, w, 1, 0.2);
            self.renderer.draw_text(1, h - 1, &help, dim);
        }
    }

    fn palette_name(&self) -> &str {
        self.current_blend().name()
    }

    // ---- ввод ------------------------------------------------------------

    /// Возвращает `false`, если пора выходить.
    fn handle(&mut self, actions: &[Action]) -> bool {
        for action in actions {
            match *action {
                Action::Quit => return false,

                Action::Resized => self.sync_size(),
                Action::Redraw => {
                    let _ = self.term.clear();
                    self.renderer.invalidate();
                }

                Action::NextFractal => self.switch_fractal(1),
                Action::PrevFractal => self.switch_fractal(-1),

                Action::NextPalette => {
                    self.cycle_palette(1, 0.7);
                    self.note(format!(
                        "палитра: {}",
                        self.palettes[self.palette_index].name
                    ));
                }
                Action::PrevPalette => {
                    self.cycle_palette(-1, 0.7);
                    self.note(format!(
                        "палитра: {}",
                        self.palettes[self.palette_index].name
                    ));
                }

                Action::NextScene => {
                    self.abort_transition();
                    self.autopilot = true;
                    self.hold_left = 0.0;
                    self.phase = Phase::Hold;
                }
                Action::Discover => self.discover_now(),

                Action::ToggleAutopilot => {
                    self.autopilot = !self.autopilot;
                    self.abort_transition();
                    if !self.autopilot {
                        self.camera.cancel_flight();
                    } else {
                        self.phase = Phase::Hold;
                        self.hold_left = self.cfg.scene_seconds;
                    }
                    self.note(if self.autopilot {
                        "автопилот включён".into()
                    } else {
                        "ручное управление".to_string()
                    });
                }

                Action::ToggleHud => {
                    self.hud = !self.hud;
                    self.renderer.invalidate();
                }

                Action::ToggleSupersample => {
                    self.renderer.supersample = match self.renderer.supersample {
                        1 => 2,
                        2 => 3,
                        _ => 1,
                    };
                    let ss = self.renderer.supersample;
                    self.note(format!("суперсэмплинг {ss}×{ss}"));
                }

                Action::ToggleTruecolor => {
                    self.renderer.truecolor = !self.renderer.truecolor;
                    self.renderer.invalidate();
                    self.note(if self.renderer.truecolor {
                        "TrueColor".into()
                    } else {
                        "256 цветов".to_string()
                    });
                }

                Action::Pan(dx, dy) => {
                    self.take_control();
                    self.camera.pan(dx, dy);
                }
                Action::Zoom(f) => {
                    self.take_control();
                    self.camera.zoom_by(f);
                }
                Action::Rotate(d) => {
                    self.take_control();
                    self.camera.rotate_by(d);
                }

                Action::IterScale(k) => {
                    self.iter_scale = clamp(self.iter_scale * k, 0.1, 20.0);
                    self.note(format!("бюджет итераций ×{:.2}", self.iter_scale));
                }

                Action::Screenshot => self.screenshot(),

                Action::Reset => {
                    let home = self.fractals[self.current].home();
                    self.camera.set_view(View::new(home.x, home.y, home.zoom));
                    self.scene_name = "Home".to_string();
                    self.iter_scale = 1.0;
                }
            }
        }
        true
    }

    fn take_control(&mut self) {
        if self.autopilot {
            self.autopilot = false;
            self.note("ручное управление (Space — вернуть автопилот)".into());
        }
        self.abort_transition();
        self.camera.cancel_flight();
    }

    /// Прервать смену фрактала. Без этого любое действие во время затемнения
    /// оставило бы экран чёрным: гасить его больше некому.
    fn abort_transition(&mut self) {
        if self.phase == Phase::Switching {
            self.phase = Phase::Hold;
        }
        self.pending = None;
        self.fade.to(1.0, 0.3);
    }

    fn switch_fractal(&mut self, step: isize) {
        self.abort_transition();
        let len = self.fractals.len() as isize;
        self.current = ((self.current as isize + step).rem_euclid(len)) as usize;
        let home = self.fractals[self.current].home();
        self.camera.set_view(View::new(home.x, home.y, home.zoom));
        self.fractals[self.current].set_max_iter(self.cfg.max_iter);
        self.scene_name = "Home".to_string();
        self.phase = Phase::Hold;
        self.hold_left = self.cfg.scene_seconds;
        self.flash.trigger(0.3, 0.4);
        self.camera.kick(0.02, 0.35);
        self.note(format!("фрактал: {}", self.fractals[self.current].name()));
    }

    fn discover_now(&mut self) {
        self.abort_transition();
        let start = self.camera.view();
        let depth = 4 + self.rng.below(4) as u32;
        let found =
            scout::find_interesting(&*self.fractals[self.current], start, depth, &mut self.rng);
        self.camera
            .fly_to_with(found, self.cfg.flight_seconds * 0.6, Ease::InOutCubic);
        self.scene_name = "Uncharted".to_string();
        self.phase = Phase::Flight;
        self.hold_left = self.cfg.scene_seconds;
        self.note(format!("найдено: {:.6}, {:.6}", found.x, found.y));
    }

    fn sync_size(&mut self) {
        if self.term.refresh_size() {
            let (w, h) = self.term.size();
            self.renderer.resize(w, h);
            let _ = self.term.clear();
        }
    }

    // ---- экспорт ---------------------------------------------------------

    fn screenshot(&mut self) {
        let path = export::frame_path(
            &self.cfg.output_dir(),
            self.fractals[self.current].id(),
            self.shots,
        );
        match self.write_png(&path, self.cfg.export_width as usize, 2) {
            Ok(()) => {
                self.shots += 1;
                self.note(format!("сохранено: {}", path.display()));
            }
            Err(e) => self.note(format!("не удалось сохранить: {e}")),
        }
    }

    fn write_frame_png(&mut self) {
        let path = export::frame_path(
            &self.cfg.output_dir(),
            self.fractals[self.current].id(),
            self.fps.frames(),
        );
        if let Err(e) = self.write_png(&path, self.cfg.export_width as usize, 1) {
            self.note(format!("запись остановлена: {e}"));
            self.cfg.record = false;
        }
    }

    fn write_png(&self, path: &Path, width: usize, supersample: u32) -> std::io::Result<()> {
        let (cols, rows) = self.renderer.dimensions();
        // Пропорции PNG повторяют видимую область, а не сетку знакомест.
        let height = ((width as f64 * rows as f64 * self.cfg.char_aspect) / cols.max(1) as f64)
            .round()
            .max(1.0) as usize;

        let blend = self.current_blend();
        let mut shading = self.shading;
        shading.fade = 1.0;
        shading.flash = 0.0;

        let rgb = export::render_rgb(
            &*self.fractals[self.current],
            self.camera.view(),
            &blend,
            &shading,
            (width, height),
            supersample,
            &self.pool,
        );
        export::save_png(path, &rgb, width, height)
    }

    fn note(&mut self, msg: String) {
        self.status = Some((msg, 2.5));
    }
}

/// Файл рядом с программой важнее вшитого набора, но его отсутствие
/// или синтаксическая ошибка не должны мешать запуску.
fn load_palettes(cfg: &Config) -> Vec<Palette> {
    if !cfg.palettes_path.is_empty()
        && let Ok(src) = std::fs::read_to_string(&cfg.palettes_path)
        && let Ok(palettes) = palette::load_from_json(&src)
    {
        return palettes;
    }
    palette::builtin()
}

fn load_presets(cfg: &Config) -> Vec<Preset> {
    if !cfg.cameras_path.is_empty()
        && let Ok(src) = std::fs::read_to_string(&cfg.cameras_path)
        && let Ok(presets) = camera::load_presets(&src)
    {
        return presets;
    }
    camera::builtin_presets()
}

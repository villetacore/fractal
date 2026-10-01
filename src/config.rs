//! Настройки из config.toml и аргументов командной строки.
//!
//! Приоритет: аргументы > config.toml > значения по умолчанию.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::camera::View;
use crate::renderer::DEFAULT_RAMP;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Целевая частота кадров. 0 — считать так быстро, как получается.
    pub fps: u32,
    /// Число рабочих потоков. 0 — по числу ядер.
    pub threads: usize,
    /// Сторона сетки суперсэмплинга: 1 (выкл), 2 (2×2), 3, 4.
    pub supersample: u32,
    /// 24-битный цвет. `None` — определить по окружению.
    pub truecolor: Option<bool>,
    /// Базовое число итераций (растёт автоматически с глубиной зума).
    pub max_iter: u32,
    /// Сколько секунд стоять в точке перед перелётом.
    pub scene_seconds: f64,
    /// Длительность перелёта между точками.
    pub flight_seconds: f64,
    /// Скорость «бега» палитры, циклов в секунду.
    pub palette_speed: f64,
    /// Сжатие шкалы итераций перед выборкой цвета.
    pub color_cycle: f64,
    /// Медленное приближение во время стоянки, единиц ln(zoom) в секунду.
    pub drift_zoom: f64,
    /// Медленный поворот во время стоянки, радиан в секунду.
    pub drift_spin: f64,
    /// Отношение высоты знакоместа к ширине.
    pub char_aspect: f64,
    /// Символы от «пусто» до «сплошь».
    pub char_ramp: String,
    /// Стартовать в режиме автопилота.
    pub autopilot: bool,
    /// Показывать HUD.
    pub hud: bool,
    /// Доля сцен, которые ищутся автоматически, а не берутся из cameras.json.
    pub discovery_ratio: f64,
    /// Каталог для скриншотов и записи.
    pub output_dir: String,
    /// Ширина PNG при экспорте (высота считается по пропорции кадра).
    pub export_width: u32,
    /// Писать каждый кадр в output_dir как PNG (режим записи видео).
    pub record: bool,
    /// Путь к палитрам и точкам съёмки. Пусто — использовать встроенные.
    pub palettes_path: String,
    pub cameras_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fps: 30,
            threads: 0,
            supersample: 1,
            truecolor: None,
            max_iter: 220,
            scene_seconds: 8.0,
            flight_seconds: 7.0,
            palette_speed: 0.02,
            color_cycle: 0.035,
            drift_zoom: 0.045,
            drift_spin: 0.004,
            char_aspect: 2.0,
            char_ramp: DEFAULT_RAMP.to_string(),
            autopilot: true,
            hud: true,
            discovery_ratio: 0.25,
            output_dir: "frames".to_string(),
            export_width: 1280,
            record: false,
            palettes_path: "assets/palettes.json".to_string(),
            cameras_path: "assets/cameras.json".to_string(),
        }
    }
}

impl Config {
    /// Прочитать config.toml. Отсутствующий файл — не ошибка.
    pub fn load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(src) => {
                let cfg: Config =
                    toml::from_str(&src).map_err(|e| format!("{}: {e}", path.display()))?;
                cfg.validate()?;
                Ok(cfg)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.char_aspect <= 0.0 {
            return Err("char_aspect должен быть больше нуля".into());
        }
        if self.char_ramp.chars().count() < 2 {
            return Err("char_ramp должен содержать хотя бы два символа".into());
        }
        if self.scene_seconds <= 0.0 || self.flight_seconds <= 0.0 {
            return Err("scene_seconds и flight_seconds должны быть больше нуля".into());
        }
        if !(0.0..=1.0).contains(&self.discovery_ratio) {
            return Err("discovery_ratio должен лежать в диапазоне 0..1".into());
        }
        Ok(())
    }

    pub fn output_dir(&self) -> PathBuf {
        PathBuf::from(&self.output_dir)
    }
}

/// Разобранная строка запуска.
pub struct Cli {
    pub config: Config,
    /// Стартовый фрактал (id) — если задан явно.
    pub fractal: Option<String>,
    /// Стартовая палитра — если задана явно.
    pub palette: Option<String>,
    /// Стартовая точка обзора — если задана явно.
    pub view: Option<View>,
    /// Выйти после N кадров (используется для записи и тестов).
    pub frames: Option<u64>,
}

const USAGE: &str = "\
fractal — терминальный скринсейвер с фракталами

    fractal [ПАРАМЕТРЫ]

Параметры:
    -c, --config <ФАЙЛ>    файл настроек (по умолчанию config.toml)
    -f, --fractal <ID>     mandelbrot | julia | burning_ship | tricorn | newton
    -p, --palette <ИМЯ>    имя палитры из palettes.json
        --at <X,Y,ZOOM>    начальная точка обзора
        --fps <N>          ограничение частоты кадров (0 — без ограничения)
    -j, --threads <N>      число потоков (0 — по числу ядер)
    -s, --supersample <N>  сглаживание N×N (1..4)
        --no-autopilot     не летать самому
        --no-hud           спрятать статистику
        --no-truecolor     использовать 256 цветов вместо 24-битных
        --record <КАТАЛОГ> писать кадры в PNG (медленно, для видео)
        --frames <N>       выйти после N кадров
    -h, --help             эта справка
";

pub enum CliResult {
    Run(Box<Cli>),
    Help,
}

/// Поля конфига, заданные в командной строке. Собираются отдельно, потому что
/// применяются уже поверх прочитанного config.toml.
#[derive(Default)]
struct Overrides {
    fps: Option<u32>,
    threads: Option<usize>,
    supersample: Option<u32>,
    record_dir: Option<String>,
    autopilot: Option<bool>,
    hud: Option<bool>,
    truecolor: Option<bool>,
}

impl Overrides {
    fn apply(self, cfg: &mut Config) {
        if let Some(v) = self.fps {
            cfg.fps = v;
        }
        if let Some(v) = self.threads {
            cfg.threads = v;
        }
        if let Some(v) = self.supersample {
            cfg.supersample = v.clamp(1, 4);
        }
        if let Some(dir) = self.record_dir {
            cfg.record = true;
            cfg.output_dir = dir;
        }
        if let Some(v) = self.autopilot {
            cfg.autopilot = v;
        }
        if let Some(v) = self.hud {
            cfg.hud = v;
        }
        if let Some(v) = self.truecolor {
            cfg.truecolor = Some(v);
        }
    }
}

pub fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<CliResult, String> {
    let mut args = args.into_iter();
    let mut config_path = PathBuf::from("config.toml");
    let mut over = Overrides::default();
    let mut cli = Cli {
        config: Config::default(),
        fractal: None,
        palette: None,
        view: None,
        frames: None,
    };

    while let Some(arg) = args.next() {
        let mut value = |name: &str| -> Result<String, String> {
            args.next()
                .ok_or_else(|| format!("параметр {name} требует значение"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(CliResult::Help),
            "-c" | "--config" => config_path = PathBuf::from(value("--config")?),
            "-f" | "--fractal" => cli.fractal = Some(value("--fractal")?),
            "-p" | "--palette" => cli.palette = Some(value("--palette")?),
            "--at" => cli.view = Some(parse_view(&value("--at")?)?),
            "--frames" => cli.frames = Some(number(&value("--frames")?, "--frames")?),
            "--fps" => over.fps = Some(number(&value("--fps")?, "--fps")?),
            "-j" | "--threads" => over.threads = Some(number(&value("--threads")?, "--threads")?),
            "-s" | "--supersample" => {
                over.supersample = Some(number(&value("--supersample")?, "--supersample")?)
            }
            "--record" => over.record_dir = Some(value("--record")?),
            "--no-autopilot" => over.autopilot = Some(false),
            "--no-hud" => over.hud = Some(false),
            "--no-truecolor" => over.truecolor = Some(false),
            "--truecolor" => over.truecolor = Some(true),
            other => return Err(format!("неизвестный аргумент: {other}")),
        }
    }

    cli.config = Config::load(&config_path)?;
    over.apply(&mut cli.config);
    cli.config.validate()?;

    Ok(CliResult::Run(Box::new(cli)))
}

fn number<T: std::str::FromStr>(raw: &str, name: &str) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    raw.parse::<T>().map_err(|e| format!("{name}: {e}"))
}

pub fn usage() -> &'static str {
    USAGE
}

fn parse_view(s: &str) -> Result<View, String> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err("--at ожидает X,Y,ZOOM".into());
    }
    let nums: Result<Vec<f64>, _> = parts.iter().map(|p| p.parse::<f64>()).collect();
    let nums = nums.map_err(|e| format!("--at: {e}"))?;
    if nums[2] <= 0.0 || !nums[2].is_finite() {
        return Err("--at: zoom должен быть больше нуля".into());
    }
    Ok(View::new(nums[0], nums[1], nums[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_are_valid() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn missing_config_is_not_an_error() {
        let cfg = Config::load(Path::new("нет-такого-файла.toml")).unwrap();
        assert_eq!(cfg.fps, Config::default().fps);
    }

    #[test]
    fn cli_overrides_config() {
        let CliResult::Run(cli) = parse_args(args(&[
            "--fps", "45", "-j", "3", "-s", "2", "--no-hud", "-f", "julia",
        ]))
        .unwrap() else {
            panic!("ожидался запуск");
        };
        assert_eq!(cli.config.fps, 45);
        assert_eq!(cli.config.threads, 3);
        assert_eq!(cli.config.supersample, 2);
        assert!(!cli.config.hud);
        assert_eq!(cli.fractal.as_deref(), Some("julia"));
    }

    #[test]
    fn parses_view() {
        let CliResult::Run(cli) = parse_args(args(&["--at", "-0.5, 0.25, 1e-3"])).unwrap() else {
            panic!("ожидался запуск");
        };
        let v = cli.view.unwrap();
        assert_eq!((v.x, v.y, v.zoom), (-0.5, 0.25, 1e-3));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_args(args(&["--at", "1,2"])).is_err());
        assert!(parse_args(args(&["--at", "1,2,0"])).is_err());
        assert!(parse_args(args(&["--fps"])).is_err());
        assert!(parse_args(args(&["--что-то"])).is_err());
        assert!(parse_args(args(&["мусор"])).is_err());
    }

    #[test]
    fn help_is_recognised() {
        assert!(matches!(
            parse_args(args(&["--help"])).unwrap(),
            CliResult::Help
        ));
    }

    #[test]
    fn config_parses_from_toml() {
        let cfg: Config = toml::from_str("fps = 60\nsupersample = 2\nhud = false\n").unwrap();
        assert_eq!(cfg.fps, 60);
        assert_eq!(cfg.supersample, 2);
        assert!(!cfg.hud);
        // Остальные поля берутся из Default.
        assert_eq!(cfg.char_aspect, 2.0);
    }
}

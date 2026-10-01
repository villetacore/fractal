//! fractal — терминальный скринсейвер с фракталами.
//!
//! Движок разложен по модулям: расчёт (`fractals`, `thread_pool`),
//! представление (`renderer`, `palette`, `terminal`), движение (`camera`,
//! `animation`, `scout`) и управление (`app`, `input`, `config`).

mod animation;
mod app;
mod camera;
mod config;
mod export;
mod fps;
mod fractals;
mod input;
mod palette;
mod renderer;
mod scout;
mod terminal;
mod thread_pool;
mod util;

use std::process::ExitCode;

use config::CliResult;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let cli = match config::parse_args(args) {
        Ok(CliResult::Help) => {
            print!("{}", config::usage());
            return ExitCode::SUCCESS;
        }
        Ok(CliResult::Run(cli)) => *cli,
        Err(e) => {
            eprintln!("ошибка: {e}\n\n{}", config::usage());
            return ExitCode::FAILURE;
        }
    };

    // Паника в raw-режиме оставила бы терминал без курсора и с обрезанным
    // вводом, поэтому восстанавливаем его до того, как сообщение уйдёт в stderr.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        terminal::force_restore();
        default_hook(info);
    }));

    let mut app = match app::App::new(cli) {
        Ok(app) => app,
        Err(e) => {
            terminal::force_restore();
            eprintln!("ошибка: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = app.run();
    drop(app);

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ошибка вывода: {e}");
            ExitCode::FAILURE
        }
    }
}

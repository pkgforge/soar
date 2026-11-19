//! Full-screen terminal interface for soar.
//!
//! [`run`] takes over the terminal until the user quits. It drives the same
//! operations as the command line, so anything installed here is visible to
//! `soar` and the other way around.

use std::sync::mpsc;

use soar_config::{
    config::{CONFIG_PATH, get_current_profile, is_system_mode},
    packages::PackagesConfig,
};
use soar_core::{SoarResult, error::SoarError};
use tokio::runtime::Handle;

mod action;
mod app;
mod effect;
mod event_loop;
mod format;
mod jobs;
mod keymap;
mod loadable;
mod log;
mod logging;
mod msg;
mod privileged;
mod progress;
mod runner;
mod sink;
mod terminal;
#[cfg(test)]
mod test_support;
mod theme;
mod ui;
mod views;
mod widgets;

use app::{App, AppEnv};
use event_loop::{InputThread, run_loop};
use msg::Msg;
use runner::Runner;
use terminal::Session;
use theme::Theme;

/// Settings the command line passes to the interface.
#[derive(Debug, Clone)]
pub struct TuiOptions {
    /// Version string shown in the header.
    pub version: String,
    /// Draw without colors, using only bold, dim, and reversed text.
    pub no_color: bool,
    /// Use unicode symbols. When false, ASCII fallbacks are used.
    pub icons: bool,
    /// How many `-v` flags were given. One shows debug messages, two or more
    /// show traces.
    pub verbosity: u8,
    /// Show only errors, hiding warnings and informational messages.
    pub quiet: bool,
    /// Global options to pass to a privileged soar child, such as
    /// `--config` and `--profile`.
    pub forward_args: Vec<std::ffi::OsString>,
}

/// Runs the interface until the user quits.
pub async fn run(options: TuiOptions) -> SoarResult<()> {
    terminal::install_panic_hook();
    let handle = Handle::current();
    let env = AppEnv {
        version: options.version.clone(),
        profile: get_current_profile(),
        system: is_system_mode(),
        config_path: CONFIG_PATH
            .read()
            .map(|path| path.to_path_buf())
            .unwrap_or_default(),
        packages_path: PackagesConfig::resolve_path(None),
    };
    tokio::task::spawn_blocking(move || session_main(options, env, handle))
        .await
        .map_err(|e| SoarError::Custom(e.to_string()))?
}

/// Builds the channel, the state, and the terminal session, then runs the
/// loop until the user quits.
fn session_main(options: TuiOptions, env: AppEnv, handle: Handle) -> SoarResult<()> {
    let (tx, rx) = mpsc::channel();
    let mut runner = Runner::new(handle, tx.clone(), options.forward_args.clone());
    let theme = Theme::new(!options.no_color, options.icons);
    let mut app = App::new(env, theme);
    let log_tx = tx.clone();
    logging::init(&options, move |line| {
        let _ = log_tx.send(Msg::Output(line));
    });
    let mut session = Session::enter().map_err(io_failed("taking over the terminal"))?;
    let input = InputThread::spawn(tx.clone()).map_err(io_failed("starting the input thread"))?;
    for effect in app.startup() {
        runner
            .execute(effect, &mut session, &input)
            .map_err(io_failed("running the startup work"))?;
    }
    let result = run_loop(&mut session, &mut app, &rx, &mut runner, &input, &tx);
    input.stop();
    session
        .leave()
        .map_err(io_failed("restoring the terminal"))?;
    result.map_err(io_failed("running the interface"))?;
    Ok(())
}

/// Names what the interface was doing when an I/O error happened.
fn io_failed(action: &'static str) -> impl Fn(std::io::Error) -> SoarError {
    move |source| {
        SoarError::IoError {
            action: action.to_string(),
            source,
        }
    }
}

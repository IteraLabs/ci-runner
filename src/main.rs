mod app;
mod cpu;
mod fmt;
mod host;
mod jobs;
mod mem;
mod net;
mod probe;
mod psi;
mod therm;
mod ui;

use std::io;
use std::mem::MaybeUninit;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use app::App;

const DEFAULT_MS: u64 = 1000;
const MIN_MS: u64 = 100;
const RESTORE: &[u8] = b"\x1b[?1049l\x1b[?25h\x1b[0m";

static mut SAVED: MaybeUninit<libc::termios> = MaybeUninit::uninit();
static HAVE_SAVED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(sig: libc::c_int) {
    unsafe {
        if HAVE_SAVED.load(Ordering::Relaxed) {
            libc::tcsetattr(0, libc::TCSANOW, (&raw const SAVED).cast());
        }
        libc::write(1, RESTORE.as_ptr().cast(), RESTORE.len());
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

fn install_handlers() {
    unsafe {
        if libc::tcgetattr(0, (&raw mut SAVED).cast()) == 0 {
            HAVE_SAVED.store(true, Ordering::Relaxed);
        }
        for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT, libc::SIGQUIT] {
            libc::signal(sig, on_signal as *const () as libc::sighandler_t);
        }
    }
}

const USAGE: &str = "usage: citop [interval_ms]\n\n\
     interval_ms  refresh period in milliseconds, minimum 100, default 1000\n\n\
     -h, --help     print this message\n\
     -V, --version  print version and target\n\n\
     keys: q or Esc quit, r refresh now";

enum ArgError {
    Help,
    Version,
    Unknown,
    Invalid(&'static str),
}

fn parse_args<I: Iterator<Item = String>>(args: I) -> Result<Duration, ArgError> {
    let mut ms = None;
    for a in args {
        match a.as_str() {
            "-h" | "--help" => return Err(ArgError::Help),
            "-V" | "--version" => return Err(ArgError::Version),
            s if s.starts_with('-') => return Err(ArgError::Unknown),
            _ => {
                if ms.is_some() {
                    return Err(ArgError::Invalid("expected at most one interval_ms"));
                }
                match a.parse::<u64>() {
                    Ok(v) if v >= MIN_MS => ms = Some(v),
                    _ => {
                        return Err(ArgError::Invalid("interval_ms must be an integer >= 100"));
                    }
                }
            }
        }
    }
    Ok(Duration::from_millis(ms.unwrap_or(DEFAULT_MS)))
}

fn is_interactive() -> bool {
    unsafe { libc::isatty(0) == 1 && libc::isatty(1) == 1 }
}

fn main() -> ExitCode {
    let tick = match parse_args(std::env::args().skip(1)) {
        Ok(t) => t,
        Err(ArgError::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(ArgError::Version) => {
            println!(
                "citop {} {} {}",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH,
                std::env::consts::OS
            );
            return ExitCode::SUCCESS;
        }
        Err(ArgError::Unknown) => {
            eprintln!("citop: unknown option, see --help");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        Err(ArgError::Invalid(msg)) => {
            eprintln!("citop: {msg}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !is_interactive() {
        eprintln!("citop: stdin and stdout must be a terminal");
        return ExitCode::from(2);
    }
    install_handlers();
    match ratatui::run(|terminal| event_loop(terminal, tick)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("citop: {e}");
            ExitCode::FAILURE
        }
    }
}

fn event_loop(terminal: &mut DefaultTerminal, tick: Duration) -> io::Result<()> {
    let mut app = App::new(tick);
    app.tick();
    let mut next = Instant::now() + tick;
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|frame| ui::render(frame, &app))?;
            dirty = false;
        }

        let timeout = next.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => match k.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') | KeyCode::Char('C')
                        if k.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        return Ok(());
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        app.tick();
                        next = Instant::now() + tick;
                        dirty = true;
                    }
                    _ => {}
                },
                Event::Resize(_, _) => dirty = true,
                _ => {}
            }
        }

        let now = Instant::now();
        if now >= next {
            next = now + tick;
            app.tick();
            dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn no_arguments_uses_the_default_interval() {
        let d = parse_args(args(&[])).ok().unwrap();
        assert_eq!(d, Duration::from_millis(DEFAULT_MS));
    }

    #[test]
    fn a_positional_interval_is_accepted() {
        let d = parse_args(args(&["250"])).ok().unwrap();
        assert_eq!(d, Duration::from_millis(250));
    }

    #[test]
    fn help_is_distinct_from_an_error() {
        assert!(matches!(parse_args(args(&["-h"])), Err(ArgError::Help)));
        assert!(matches!(parse_args(args(&["--help"])), Err(ArgError::Help)));
    }

    #[test]
    fn intervals_below_the_floor_are_rejected() {
        assert!(matches!(
            parse_args(args(&["99"])),
            Err(ArgError::Invalid(_))
        ));
        assert!(matches!(
            parse_args(args(&["0"])),
            Err(ArgError::Invalid(_))
        ));
    }

    #[test]
    fn non_numeric_arguments_are_rejected() {
        assert!(matches!(
            parse_args(args(&["abc"])),
            Err(ArgError::Invalid(_))
        ));
    }

    #[test]
    fn version_is_distinct_from_help_and_from_an_error() {
        assert!(matches!(parse_args(args(&["-V"])), Err(ArgError::Version)));
        assert!(matches!(
            parse_args(args(&["--version"])),
            Err(ArgError::Version)
        ));
    }

    #[test]
    fn unknown_flags_report_as_flags_not_as_a_bad_interval() {
        assert!(matches!(parse_args(args(&["-5"])), Err(ArgError::Unknown)));
        assert!(matches!(
            parse_args(args(&["--verbose"])),
            Err(ArgError::Unknown)
        ));
        assert!(matches!(
            parse_args(args(&["--no-color"])),
            Err(ArgError::Unknown)
        ));
    }

    #[test]
    fn a_second_positional_is_rejected_rather_than_last_wins() {
        assert!(matches!(
            parse_args(args(&["500", "2000"])),
            Err(ArgError::Invalid(_))
        ));
    }
}

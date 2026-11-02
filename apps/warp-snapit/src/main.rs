use std::process::ExitCode;

use snapit::cli::{Action, USAGE};

fn main() -> ExitCode {
    let action = match snapit::cli::parse(std::env::args_os().skip(1)) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("snapit: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match action {
        Action::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("snapit {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Action::Run(cli) => run(cli),
    }
}

#[cfg(windows)]
fn run(cli: snapit::cli::Cli) -> ExitCode {
    match snapit::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("snapit: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn run(_cli: snapit::cli::Cli) -> ExitCode {
    eprintln!("snapit: the SnapIt engine is Windows-only.");
    ExitCode::FAILURE
}

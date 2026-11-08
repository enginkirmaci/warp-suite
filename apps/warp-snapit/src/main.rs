use std::process::ExitCode;

use warp_snapit::cli::{Action, USAGE};

fn main() -> ExitCode {
    let action = match warp_snapit::cli::parse(std::env::args_os().skip(1)) {
        Ok(action) => action,
        Err(error) => {
            eprintln!("warp-snapit: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    match action {
        Action::Help => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("warp-snapit {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Action::Run(cli) => run(cli),
    }
}

#[cfg(windows)]
fn run(cli: warp_snapit::cli::Cli) -> ExitCode {
    match warp_snapit::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("warp-snapit: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn run(_cli: warp_snapit::cli::Cli) -> ExitCode {
    eprintln!("warp-snapit: the SnapIt engine is Windows-only.");
    ExitCode::FAILURE
}

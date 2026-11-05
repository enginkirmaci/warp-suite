//! Command-line contract shared with the .NET companion.
//!
//! Warp Station launches the companion with `--parent <pid>`; the parent watcher
//! exits when Warp Station dies. `--dump-zones` is a Rust-port extra for checking
//! the layout math against the current monitor setup.

use std::ffi::OsString;

#[derive(Clone, Debug, Default)]
pub struct Cli {
    pub parent: Option<u32>,
    pub dump_zones: bool,
}

#[derive(Clone, Debug)]
pub enum Action {
    Run(Cli),
    Help,
    Version,
}

pub const USAGE: &str = "\
warp-snapit — Windows zone snapper (Rust port of the .NET SnapIt companion)

USAGE:
    warp-snapit [OPTIONS]

OPTIONS:
    --parent <PID>   Exit when the given process exits (used by Warp Station)
    --dump-zones     Print monitors and snap zones, then exit
    -h, --help       Show this help
    -V, --version    Show the version

Settings and layouts are shared with the .NET app in
%USERPROFILE%\\.warp\\snapit.";

pub fn parse<I>(args: I) -> Result<Action, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut cli = Cli::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        let arg = arg
            .to_str()
            .ok_or_else(|| "arguments must be valid UTF-8".to_string())?;

        match arg {
            "-h" | "--help" => return Ok(Action::Help),
            "-V" | "--version" => return Ok(Action::Version),
            "--dump-zones" => cli.dump_zones = true,
            "--parent" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--parent needs a process id".to_string())?;
                let value = value
                    .to_str()
                    .ok_or_else(|| "--parent needs a valid process id".to_string())?;
                cli.parent = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid process id: {value}"))?,
                );
            }
            other if other.starts_with("--parent=") => {
                let value = &other["--parent=".len()..];
                cli.parent = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid process id: {value}"))?,
                );
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(Action::Run(cli))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Action, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_parent_and_dump() {
        let Action::Run(cli) = parse_str(&["--parent", "1234", "--dump-zones"]).unwrap() else {
            panic!("expected run");
        };
        assert_eq!(cli.parent, Some(1234));
        assert!(cli.dump_zones);
    }

    #[test]
    fn parses_parent_equals() {
        let Action::Run(cli) = parse_str(&["--parent=42"]).unwrap() else {
            panic!("expected run");
        };
        assert_eq!(cli.parent, Some(42));
    }

    #[test]
    fn rejects_unknown_and_missing_values() {
        assert!(parse_str(&["--nope"]).is_err());
        assert!(parse_str(&["--parent"]).is_err());
        assert!(parse_str(&["--parent", "abc"]).is_err());
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(parse_str(&["--help"]).unwrap(), Action::Help));
        assert!(matches!(parse_str(&["-V"]).unwrap(), Action::Version));
    }
}

use std::io::IsTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    let invocation =
        match pdiff::cli::parse_from(std::env::args_os(), std::io::stdin().is_terminal()) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("pdiff: {error}");
                return ExitCode::from(2);
            }
        };
    match pdiff::runtime::run(invocation) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("pdiff: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

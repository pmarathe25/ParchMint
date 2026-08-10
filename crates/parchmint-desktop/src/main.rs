use parchmint_desktop::{DesktopBootstrap, ExitCode, LaunchRequest};

fn main() -> std::process::ExitCode {
    let exit = DesktopBootstrap::production()
        .and_then(|desktop| desktop.run(LaunchRequest::from_environment()))
        .unwrap_or_else(|error| error.report_and_exit());
    process_exit_code(exit)
}

fn process_exit_code(exit: ExitCode) -> std::process::ExitCode {
    match u8::try_from(exit.value()) {
        Ok(value) => std::process::ExitCode::from(value),
        Err(_) => std::process::ExitCode::FAILURE,
    }
}

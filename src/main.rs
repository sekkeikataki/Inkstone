fn main() -> gtk4::glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && inkstone::cli::is_cli_invocation(&args[1..]) {
        let code = inkstone::cli::run(&args[1..]);
        return gtk4::glib::ExitCode::from(code);
    }
    inkstone::app::run()
}

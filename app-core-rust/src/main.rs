fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = if args.as_slice() == ["--background"] {
        autofix_core::run_background_entry()
    } else if args.as_slice() == ["--shutdown-all"] {
        autofix_core::shutdown_all_entry()
    } else {
        eprintln!("usage: AF-BG-Engine.exe [--background|--shutdown-all]");
        2
    };

    std::process::exit(code);
}

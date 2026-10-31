use clap::Parser;

fn main() {
    let cli = apc_cli::Cli::parse();
    let report = matches!(cli.cmd, apc_cli::Cmd::Report);
    match apc_cli::run(cli) {
        Ok(serde_json::Value::Null) => {}
        Ok(v) if report => println!("{}", v["markdown"].as_str().unwrap_or_default()),
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default()),
        Err(e) => {
            eprintln!("{}: {e:#}", apc_core::brand::CLI_NAME);
            std::process::exit(1);
        }
    }
}

#![deny(clippy::all)]

use agent_fork::cli::Cli;
use clap::Parser;
use colored::Colorize;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.verbose {
        let _ = tracing_subscriber::fmt()
            .with_env_filter("agent_fork=debug")
            .try_init();
    }

    match agent_fork::run_app(cli).await {
        Ok(code) => {
            if code == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code as u8)
            }
        }
        Err(err) => {
            eprintln!("{}: {}", "error".bold().red(), err);
            ExitCode::from(1)
        }
    }
}

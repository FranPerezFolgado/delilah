use clap::Parser;

/// Local orchestrator for coding agents.
#[derive(Parser)]
#[command(name = "delilah", version, about)]
struct Cli {}

fn main() {
    Cli::parse();
}

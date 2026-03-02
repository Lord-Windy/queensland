mod adapters;
mod conflict;
mod engine;
mod ports;
mod prompt;
mod state;
mod tui;
mod worker;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "queensland")]
#[command(about = "A parallel task execution framework", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(short, long, global = true)]
    template: Option<String>,

    #[arg(short, long, global = true)]
    concurrency: Option<usize>,

    #[arg(long, global = true)]
    dry_run: bool,

    #[arg(short, long, global = true)]
    verbose: bool,

    #[arg(long, global = true)]
    worktree_dir: Option<String>,

    #[arg(long, global = true)]
    new: bool,
}

#[derive(Subcommand)]
enum Commands {
    Status,
    Resume,
    Cleanup,
    Merge,
    Inspect { ticket: String },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        None => {
            println!("not yet implemented");
        }
        Some(Commands::Status) => {
            println!("not yet implemented");
        }
        Some(Commands::Resume) => {
            println!("not yet implemented");
        }
        Some(Commands::Cleanup) => {
            println!("not yet implemented");
        }
        Some(Commands::Merge) => {
            println!("not yet implemented");
        }
        Some(Commands::Inspect { ticket: _ }) => {
            println!("not yet implemented");
        }
    }
}

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about = "Rustpress · prepare once, publish exact bytes")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile the reviewed corpus, build the site, and seal the complete output.
    Stage {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        site: PathBuf,
        #[arg(long)]
        release: String,
        #[arg(long)]
        previous: Option<String>,
    },
    /// Verify only the sealed bundle. No source or current configuration is read.
    Verify {
        #[arg(long)]
        site: PathBuf,
        #[arg(long)]
        release: String,
        #[arg(long)]
        expect: Option<String>,
    },
    /// Project an independently reviewed seal. Never rebuilds.
    Deploy {
        #[arg(long)]
        site: PathBuf,
        #[arg(long)]
        release: String,
        #[arg(long)]
        expect: String,
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("rustpress · refused\n{error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Stage {
            source,
            site,
            release,
            previous,
        } => rustpress::stage(&source, &site, &release, previous.as_deref()),
        Command::Verify {
            site,
            release,
            expect,
        } => {
            let verified = rustpress::verify(&site, &release, expect.as_deref())?;
            println!(
                "rustpress · verified\n{}\nseal  {}\n{} sealed files",
                release,
                verified.digest,
                verified.seal.files.len()
            );
            Ok(())
        }
        Command::Deploy {
            site,
            release,
            expect,
            dry_run,
        } => rustpress::deploy(&site, &release, &expect, dry_run),
    }
}

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[clap(name = "cargo")]
#[clap(bin_name = "cargo")]
pub enum Cargo {
    Dylib(DylibCli),
}

#[derive(Debug, clap::Args)]
#[clap(author, version, about)]
pub struct DylibCli {
    #[clap(subcommand)]
    pub subcommand: SubCommand,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    Init,
    Build,
    Run,
}

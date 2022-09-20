use clap::Parser;

#[derive(Debug, Parser)]
#[clap(name = "cargo")]
#[clap(bin_name = "cargo")]
pub enum Cargo {
    Dylib(DylibCli),
}

#[derive(Debug, clap::Args)]
#[clap(author, version, about)]
pub struct DylibCli {
    pub subcommand: String,
    pub arguments: Vec<String>,
}

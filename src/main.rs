#![allow(clippy::enum_variant_names)]

use clap::Parser as _;
use tracing::debug;

use crate::{
    application::{Application, ApplicationError},
    cli::Cli,
};

mod application;
mod cli;
mod config;
mod executor;
mod ext;
mod file_dependencies;
mod tasks;

#[compio::main]
#[snafu::report]
async fn main() -> Result<(), ApplicationError> {
    let cli_args = Cli::parse();
    setup_tracing(&cli_args);
    debug!("Parsed CLI arguments: {cli_args:?}");

    Application::run(cli_args).await?;

    Ok(())
}

fn setup_tracing(cli_args: &Cli) {
    if let Some(level) = cli_args.log_level.to_tracing_level() {
        tracing_subscriber::fmt()
            .with_max_level(level)
            .without_time()
            .compact()
            .init();
    }
}

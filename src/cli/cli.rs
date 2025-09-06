use std::path::PathBuf;

use clap::Parser;

use crate::application::data::LogLevel;

#[derive(Parser, Debug, Clone)]
#[command(version)]
pub struct Cli {
    pub target: String,
    #[clap(long, short, default_value = "warn", value_enum)]
    pub log_level: LogLevel,

    /// The root directory of the project
    #[clap(long, short, default_value = ".")]
    pub root: PathBuf,
}

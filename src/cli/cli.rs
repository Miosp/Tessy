use clap::Parser;

use crate::application::{ApplicationConfig, data::LogLevel};

#[derive(Parser, Debug, Clone)]
pub struct Cli {
    pub target: String,
    #[clap(long, short, default_value = "warn", value_enum)]
    pub log_level: LogLevel,
}

impl Into<ApplicationConfig> for Cli {
    fn into(self) -> ApplicationConfig {
        ApplicationConfig {
            target: self.target,
        }
    }
}

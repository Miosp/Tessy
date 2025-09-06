use std::path::PathBuf;

use crate::cli::Cli;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub target: String,
    pub root: PathBuf,
}

impl From<Cli> for RuntimeConfig {
    fn from(cli: Cli) -> Self {
        Self {
            target: cli.target,
            root: cli.root,
        }
    }
}

use clap::ValueEnum;

#[derive(Debug, Clone, ValueEnum, Default)]
pub enum LogLevel {
    Debug,
    Info,
    #[default]
    Warn,
    Error,
    Silent,
}

impl LogLevel {
    pub fn to_tracing_level(&self) -> Option<tracing::Level> {
        match self {
            LogLevel::Debug => Some(tracing::Level::DEBUG),
            LogLevel::Info => Some(tracing::Level::INFO),
            LogLevel::Warn => Some(tracing::Level::WARN),
            LogLevel::Error => Some(tracing::Level::ERROR),
            LogLevel::Silent => None,
        }
    }
}

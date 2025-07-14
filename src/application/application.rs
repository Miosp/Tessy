use std::sync::Arc;

use snafu::Snafu;
use snafu::prelude::*;
use tracing::debug;

use crate::application::ApplicationConfig;
use crate::config::config::TaskRegistry;
use crate::config::config::TaskRegistryCreationError;
use crate::executor::DependencyGraph;
use crate::executor::ExecutionError;
use crate::executor::Executor;
use crate::executor::ExecutorCreationError;

pub struct Application;

impl Application {
    pub async fn run(app_config: impl Into<ApplicationConfig>) -> Result<(), ApplicationError> {
        let app_config: ApplicationConfig = app_config.into();
        let config = TaskRegistry::read().await.context(TaskRegistrySnafu)?;
        debug!("Loaded config: {:?}", config);

        let dependency_graph = DependencyGraph::from_config(&config, &app_config.target);

        let arc_config = Arc::new(config);
        let arc_dependency_graph = Arc::new(dependency_graph);
        let arc_app_config = Arc::new(app_config);

        Executor::new(arc_config, arc_dependency_graph, arc_app_config)
            .context(ExecutorCreationSnafu)?
            .execute()
            .await
            .context(ApplicationExecutionSnafu)?;

        Ok(())
    }
}

#[derive(Debug, Snafu)]
pub enum ApplicationError {
    #[snafu(display("Critical failure encountered during configuration stage"))]
    TaskRegistryError { source: TaskRegistryCreationError },
    #[snafu(display("Critical failure encountered during executor creation"))]
    ExecutorCreationError { source: ExecutorCreationError },
    #[snafu(display("Critical failure encountered during application execution"))]
    ApplicationExecutionError { source: ExecutionError },
}

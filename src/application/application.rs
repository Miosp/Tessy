use std::sync::Arc;

use snafu::Snafu;
use snafu::prelude::*;
use tracing::debug;
use tracing::error;
use tracing::info;

use crate::application::ApplicationConfig;
use crate::config::config::TaskRegistry;
use crate::config::config::TaskRegistryCreationError;
use crate::executor::DependencyGraph;
use crate::executor::ExecutionError;
use crate::executor::Executor;
use crate::executor::ExecutorCreationError;
use crate::file_dependencies::DependencyTracker;

pub struct Application;

impl Application {
    pub async fn run(app_config: impl Into<ApplicationConfig>) -> Result<(), ApplicationError> {
        let app_config: ApplicationConfig = app_config.into();
        let config = TaskRegistry::read().await.context(TaskRegistrySnafu)?;
        debug!("Loaded config: {:?}", config);

        let saved_dependencies_fut = DependencyTracker::read();
        let dependency_graph = DependencyGraph::from_config(&config, &app_config.target);

        let arc_config = Arc::new(config);
        let arc_dependency_graph = Arc::new(dependency_graph);
        let arc_app_config = Arc::new(app_config);
        let mut arc_saved_dependencies = Arc::new(saved_dependencies_fut.await);

        let executed_tasks = Executor::new(
            arc_config.clone(),
            arc_dependency_graph,
            arc_app_config,
            arc_saved_dependencies.clone(),
        )
        .context(ExecutorCreationSnafu)?
        .execute()
        .await
        .context(ApplicationExecutionSnafu)?;
        info!("Executed tasks: {:?}", executed_tasks);

        info!("Updating saved dependencies");
        let tasks_iter = executed_tasks
            .iter()
            .map(|task_id| arc_config.get_task_by_id(task_id).unwrap());
        if let Some(saved_dependencies) = Arc::get_mut(&mut arc_saved_dependencies) {
            saved_dependencies.add_tasks_dependencies(tasks_iter).await;
            saved_dependencies.write().await;
        } else {
            error!(
                "Failed to get mutable reference to saved dependencies. The dependencies will not be updated."
            );
        }

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

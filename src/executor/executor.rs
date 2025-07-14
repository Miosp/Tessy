use std::num::NonZeroUsize;
use std::thread::available_parallelism;
use std::{collections::HashMap, sync::Arc};

use compio::dispatcher::{Dispatcher, DispatcherBuilder};
use compio::runtime::spawn;
use futures::StreamExt;
use futures_channel::mpsc::{self, UnboundedSender};
use snafu::{ResultExt, Snafu};
use tracing::{debug, info};

use crate::application::ApplicationConfig;
use crate::config::config::Config;
use crate::executor::DependencyGraph;
use crate::tasks::{Task, TaskError, TaskTrait};

/// Default number of worker threads when unable to determine system parallelism
const DEFAULT_WORKER_THREADS: usize = 1;

pub struct Executor {
    dispatcher: Dispatcher,
    app_config: Arc<ApplicationConfig>,
    config: Arc<Config>,
    dependency_graph: Arc<DependencyGraph>,
}

impl Executor {
    /// Creates a new Executor with the specified configuration and dependency graph
    pub fn new(
        config: Arc<Config>,
        dependency_graph: Arc<DependencyGraph>,
        app_config: Arc<ApplicationConfig>,
    ) -> Result<Self, ExecutorCreationError> {
        let workers_num = Self::determine_worker_count();
        debug!("Using {} worker threads for task execution", workers_num);

        let dispatcher = DispatcherBuilder::new()
            .worker_threads(workers_num)
            .build()
            .context(DispatcherSnafu)?;

        Ok(Self {
            dispatcher,
            config,
            dependency_graph,
            app_config,
        })
    }

    /// Determines the optimal number of worker threads for task execution
    fn determine_worker_count() -> NonZeroUsize {
        available_parallelism()
            .map(|n| n.get())
            .map(|n| NonZeroUsize::new(n))
            .ok()
            .flatten()
            .unwrap_or_else(|| NonZeroUsize::new(DEFAULT_WORKER_THREADS).unwrap())
    }

    /// Main execution method that coordinates task execution based on dependencies
    pub async fn execute(&self) -> Result<(), ExecutionError> {
        let mut dependency_counts = self.initialize_dependency_counts();
        let (task_sender, mut task_receiver) = mpsc::unbounded::<Result<String, TaskError>>();

        // Dispatch all tasks that have no dependencies
        self.dispatch_initial_tasks(&task_sender, &dependency_counts)
            .await?;

        // Process task completion results until target is reached
        self.process_task_results(&mut task_receiver, &mut dependency_counts, &task_sender)
            .await
    }

    /// Dispatches all tasks that have no dependencies and are ready to execute immediately
    async fn dispatch_initial_tasks(
        &self,
        task_sender: &UnboundedSender<Result<String, TaskError>>,
        dependency_counts: &HashMap<String, u32>,
    ) -> Result<(), ExecutionError> {
        debug!("Getting initial tasks with no dependencies");

        let ready_tasks: Vec<Task> = dependency_counts
            .iter()
            .filter_map(|(task_id, &count)| {
                if count == 0 {
                    self.config.tasks.get(task_id).cloned()
                } else {
                    None
                }
            })
            .collect();

        debug!("Dispatching {} initial tasks", ready_tasks.len());

        for task in ready_tasks {
            self.dispatch_task(task_sender.clone(), task).await?;
        }

        Ok(())
    }

    /// Processes task completion results and manages dependency countdown
    async fn process_task_results(
        &self,
        task_receiver: &mut futures_channel::mpsc::UnboundedReceiver<Result<String, TaskError>>,
        dependency_counts: &mut HashMap<String, u32>,
        task_sender: &UnboundedSender<Result<String, TaskError>>,
    ) -> Result<(), ExecutionError> {
        debug!("Starting result processing loop");

        while let Some(result) = task_receiver.next().await {
            match result {
                Ok(task_id) => {
                    debug!("Acknowledged task '{}' completion", task_id);

                    // Check if we've reached the target task
                    if task_id == self.app_config.target {
                        info!(
                            "Reached target task '{}'. Execution completed successfully.",
                            task_id
                        );
                        return Ok(());
                    }

                    // Handle dependency management for completed task
                    self.handle_task_completion(&task_id, dependency_counts, task_sender)
                        .await?;
                }
                Err(error) => {
                    return Err(error).context(TaskExecutionSnafu);
                }
            }
        }

        // Execution should end in the loop when the target task is reached, not here
        Err(ExecutionError::ExecutionEndedPrematurely)
    }

    /// Handles the completion of a task by updating dependency counts and dispatching newly ready tasks
    async fn handle_task_completion(
        &self,
        completed_task_id: &str,
        dependency_counts: &mut HashMap<String, u32>,
        task_sender: &UnboundedSender<Result<String, TaskError>>,
    ) -> Result<(), ExecutionError> {
        let parent_tasks = self
            .dependency_graph
            .task_parents
            .get(completed_task_id)
            .cloned()
            .unwrap_or_default();

        for parent_id in parent_tasks {
            if let Some(count) = dependency_counts.get_mut(&parent_id) {
                *count -= 1;
                debug!(
                    "Decremented dependency count for task '{}'. New count: {}",
                    parent_id, count
                );

                // If all dependencies are satisfied, dispatch the parent task
                if *count == 0 {
                    if let Some(task) = self.config.tasks.get(&parent_id) {
                        debug!(
                            "All dependencies satisfied for task '{}', dispatching",
                            parent_id
                        );
                        self.dispatch_task(task_sender.clone(), task.clone())
                            .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Initialize dependency counts for all tasks based on their declared dependencies
    fn initialize_dependency_counts(&self) -> HashMap<String, u32> {
        let mut counts = HashMap::new();

        for (task_id, task) in &self.config.tasks {
            let dependency_count = task.dependencies().len() as u32;
            counts.insert(task_id.clone(), dependency_count);
            debug!("Task '{}' has {} dependencies", task_id, dependency_count);
        }

        debug!("Initialized dependency counts for {} tasks", counts.len());
        counts
    }

    /// Dispatch a task to the executor and forward the result to the task receiver
    async fn dispatch_task(
        &self,
        task_sender: UnboundedSender<Result<String, TaskError>>,
        task: Task,
    ) -> Result<(), ExecutionError> {
        let task_id = task.id().clone();

        let receiver = self
            .dispatcher
            .dispatch(move || async move { task.run().await })
            .map_err(|e| ExecutionError::TaskDispatchError {
                task_id: task_id.clone(),
                error: e.to_string(),
            })?;

        info!("Dispatched task '{}'", task_id);

        // Forward the result to the task receiver with better error handling
        let task_id_for_spawn = task_id.clone();
        spawn(async move {
            let result = match receiver.await {
                Ok(inner) => inner,
                Err(e) => {
                    debug!("Task '{}' was canceled: {}", task_id_for_spawn, e);
                    Err(TaskError::CanceledError { source: e })
                }
            };

            if let Err(send_err) = task_sender.unbounded_send(result) {
                debug!(
                    "Failed to send task result for '{}': {}",
                    task_id_for_spawn, send_err
                );
            }
        })
        .detach();

        Ok(())
    }
}

#[derive(Debug, Snafu)]
pub enum ExecutorCreationError {
    #[snafu(display("Failed to create task dispatcher"))]
    DispatcherError { source: std::io::Error },
}

#[derive(Debug, Snafu)]
pub enum ExecutionError {
    #[snafu(display("Failed to dispatch task '{}': {}", task_id, error))]
    TaskDispatchError { task_id: String, error: String },
    #[snafu(display("Got a task execution error"))]
    TaskExecutionError { source: TaskError },
    #[snafu(display("Execution loop ended before reaching target task"))]
    ExecutionEndedPrematurely,
}

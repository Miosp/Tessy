use hashlink::LinkedHashMap;
use saphyr::{Scalar, Yaml};
use snafu::Snafu;

use crate::tasks::{ExecuteTask, ExecuteTaskError};

pub trait TaskTrait {
    fn from_task_yaml(task_name: &str, task_data: &LinkedHashMap<Yaml, Yaml>) -> Option<Self>
    where
        Self: Sized;
    // Runs the task and returns its id on success
    async fn run(&self) -> Result<String, TaskError>;
    fn id(&self) -> String;
    fn dependencies(&self) -> &Vec<String>;
    fn inputs(&self) -> &Vec<String>;
}

#[derive(Debug, Clone)]
pub enum Task {
    Execute(ExecuteTask),
}

impl TaskTrait for Task {
    fn from_task_yaml(task_name: &str, task_data: &LinkedHashMap<Yaml, Yaml>) -> Option<Self> {
        let task_type_declaration = task_data
            .get(&Yaml::Value(Scalar::String("type".into())))
            .and_then(|v| v.as_str());
        match task_type_declaration {
            Some("execute") | None => {
                ExecuteTask::from_task_yaml(task_name, task_data).map(Task::Execute)
            }
            _ => {
                tracing::warn!(
                    "Unknown task type for task '{}': {:?}. Skipping.",
                    task_name,
                    task_type_declaration
                );
                None
            }
        }
    }

    async fn run(&self) -> Result<String, TaskError> {
        match self {
            Task::Execute(task) => task.run().await,
        }
    }

    fn id(&self) -> String {
        match self {
            Task::Execute(task) => task.id(),
        }
    }

    fn dependencies(&self) -> &Vec<String> {
        match self {
            Task::Execute(task) => task.dependencies(),
        }
    }

    fn inputs(&self) -> &Vec<String> {
        match self {
            Task::Execute(task) => task.inputs(),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum TaskError {
    #[snafu(display("Failed to execute task"))]
    ExecutionError { source: ExecuteTaskError },
    #[snafu(display("Task got cancelled"))]
    CanceledError {
        source: futures_channel::oneshot::Canceled,
    },
}

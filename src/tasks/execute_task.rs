use compio::process::Command;
use hashlink::LinkedHashMap;
use saphyr::{Scalar, Yaml};
use snafu::Snafu;
use std::borrow::Cow;
use tracing::{debug, info};

use super::{BaseTask, TaskError, TaskTrait};

#[derive(Debug, Clone)]
pub struct ExecuteTask {
    base_task: BaseTask,
    command: String,
}

impl TaskTrait for ExecuteTask {
    fn from_task_yaml(task_name: &str, task_data: &LinkedHashMap<Yaml, Yaml>) -> Option<Self> {
        debug!("Parsing task '{}' of type 'execute'", task_name);

        let command = task_data
            .get(&Yaml::Value(Scalar::String(Cow::Borrowed("command"))))?
            .as_str()?
            .to_string();

        let base_task = BaseTask::from_task_yaml(task_name, task_data)?;

        Some(ExecuteTask { base_task, command })
    }

    async fn run(&self) -> Result<String, TaskError> {
        info!("Running task '{}'", self.id());

        let output = Command::new("cmd")
            .args(&["/C", &self.command])
            .output()
            .await
            .map_err(|_| TaskError::ExecutionError {
                source: ExecuteTaskError::ExecutionError {
                    command: self.command.clone(),
                    task_name: self.id(),
                },
            })?;

        match output.status.success() {
            true => {
                info!("Task '{}' ended execution", self.id());
                Ok(self.id())
            }
            false => Err(TaskError::ExecutionError {
                source: ExecuteTaskError::UnsuccessfulExecution {
                    command: self.command.clone(),
                    task_name: self.id(),
                    status: output.status.code().unwrap_or(-1),
                },
            }),
        }
    }

    fn id(&self) -> String {
        self.base_task.id()
    }

    fn dependencies(&self) -> &Vec<String> {
        self.base_task.dependencies()
    }
}

#[derive(Debug, Snafu)]
pub enum ExecuteTaskError {
    #[snafu(display("Failed to execute command '{}' for task '{}'", command, task_name))]
    ExecutionError { command: String, task_name: String },
    #[snafu(display(
        "Unsuccessful execution of command '{}' for task '{}'. Status: {}",
        command,
        task_name,
        status
    ))]
    UnsuccessfulExecution {
        command: String,
        task_name: String,
        status: i32,
    },
}

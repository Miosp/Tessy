use compio::process::Command;
use hashlink::LinkedHashMap;
use saphyr::{Scalar, Yaml};
use snafu::Snafu;
use std::borrow::Cow;
use tracing::{debug, info};

use super::{TaskError, TaskTrait};

#[derive(Debug, Clone)]
pub struct ExecuteTask {
    task_name: String,
    command: String,
    dependencies: Vec<String>,
}

impl TaskTrait for ExecuteTask {
    fn from_task_yaml(task_name: &str, task_data: &LinkedHashMap<Yaml, Yaml>) -> Option<Self> {
        debug!("Parsing task '{}' of type 'execute'", task_name);

        let command = task_data
            .get(&Yaml::Value(Scalar::String(Cow::Borrowed("command"))))?
            .as_str()?
            .to_string();

        let dependencies = task_data
            .get(&Yaml::Value(Scalar::String(Cow::Borrowed("dependsOn"))))
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Some(ExecuteTask {
            task_name: task_name.to_string(),
            command,
            dependencies,
        })
    }

    async fn run(&self) -> Result<String, TaskError> {
        info!("Running task '{}'", self.task_name);

        let output = Command::new("cmd")
            .args(&["/C", &self.command])
            .output()
            .await
            .map_err(|_| TaskError::ExecutionError {
                source: ExecuteTaskError::ExecutionError {
                    command: self.command.clone(),
                    task_name: self.task_name.clone(),
                },
            })?;

        match output.status.success() {
            true => {
                info!("Task '{}' ended execution", self.task_name);
                Ok(self.task_name.clone())
            }
            false => Err(TaskError::ExecutionError {
                source: ExecuteTaskError::UnsuccessfulExecution {
                    command: self.command.clone(),
                    task_name: self.task_name.clone(),
                    status: output.status.code().unwrap_or(-1),
                },
            }),
        }
    }

    fn id(&self) -> String {
        self.task_name.clone()
    }

    fn dependencies(&self) -> &Vec<String> {
        &self.dependencies
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

use compio::{io::compat::AsyncStream, process::Command, runtime::spawn};
use futures::{AsyncBufReadExt, StreamExt, io::BufReader};
use hashlink::LinkedHashMap;
use saphyr::{Scalar, Yaml};
use snafu::{ResultExt, Snafu};
use std::{borrow::Cow, process::Stdio};
use tracing::{debug, info};

use crate::tasks::task::print_from_task;

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
        let mut cmd = self.create_command();

        let mut handle = cmd
            .spawn()
            .context(SpawnSnafu {
                command: self.command.clone(),
                task_name: self.id(),
            })
            .map_err(|err| TaskError::ExecutionError { source: err })?;

        // Handle stdout
        if let Some(stdout) = handle.stdout.take() {
            self.spawn_stdout_handler(stdout, self.id());
        }

        // Handle stderr
        if let Some(stderr) = handle.stderr.take() {
            self.spawn_stderr_handler(stderr, self.id());
        }

        let status = handle
            .wait()
            .await
            .context(WaitSnafu {
                command: self.command.clone(),
                task_name: self.id(),
            })
            .map_err(|err| TaskError::ExecutionError { source: err })?;

        if status.success() {
            info!("Task '{}' completed successfully", self.id());
            Ok(self.id())
        } else {
            Err(TaskError::ExecutionError {
                source: ExecuteTaskError::UnsuccessfulExecution {
                    command: self.command.clone(),
                    task_name: self.id(),
                    status: status.code().unwrap_or(-1),
                },
            })
        }
    }

    fn id(&self) -> String {
        self.base_task.id()
    }

    fn dependencies(&self) -> &Vec<String> {
        self.base_task.dependencies()
    }

    fn inputs(&self) -> &Vec<String> {
        self.base_task.inputs()
    }
}

impl ExecuteTask {
    /// Returns the full command as a tuple of the command string and its arguments.
    /// This should be os-specific.
    fn full_command(&self) -> (&'static str, Vec<&str>) {
        #[cfg(target_family = "windows")]
        {
            let args = vec!["/C", &self.command];
            ("cmd", args)
        }
        #[cfg(target_family = "unix")]
        {
            let args = vec!["-c", &self.command];
            ("sh", args)
        }
    }

    /// Creates and configures the command with proper stdio settings
    fn create_command(&self) -> Command {
        let (command, args) = self.full_command();
        let mut cmd = Command::new(command);
        cmd.args(args);
        let _ = cmd.stdout(Stdio::piped());
        let _ = cmd.stderr(Stdio::piped());
        cmd
    }

    /// Spawns a task to handle stdout stream
    fn spawn_stdout_handler(&self, stdout: compio::process::ChildStdout, task_id: String) {
        let stream = AsyncStream::new(stdout);
        let color = self.color();
        //TODO - return the handle to the spawned task and ensure proper shutdown
        spawn(async move {
            let reader = BufReader::new(stream);
            let mut lines = reader.lines();

            while let Some(line_result) = lines.next().await {
                match line_result {
                    Ok(line) => {
                        if !line.trim().is_empty() {
                            print_from_task(&task_id, color, line.trim());
                        }
                    }
                    Err(e) => {
                        debug!("Error reading stdout for task '{}': {}", task_id, e);
                    }
                }
            }
        })
        .detach();
    }

    /// Spawns a task to handle stderr stream
    fn spawn_stderr_handler(&self, stderr: compio::process::ChildStderr, task_id: String) {
        let stream = AsyncStream::new(stderr);
        let color = self.color();
        //TODO - return the handle to the spawned task and ensure proper shutdown
        spawn(async move {
            let reader = BufReader::new(stream);
            let mut lines = reader.lines();

            while let Some(line_result) = lines.next().await {
                match line_result {
                    Ok(line) => {
                        if !line.trim().is_empty() {
                            print_from_task(&task_id, color, line.trim());
                        }
                    }
                    Err(e) => {
                        debug!("Error reading stderr for task '{}': {}", task_id, e);
                    }
                }
            }
        })
        .detach();
    }
}

#[derive(Debug, Snafu)]
pub enum ExecuteTaskError {
    #[snafu(display("Failed to spawn command '{}' for task '{}'", command, task_name))]
    SpawnError {
        command: String,
        task_name: String,
        source: std::io::Error,
    },
    #[snafu(display("Failed to wait for command '{}' for task '{}'", command, task_name))]
    WaitError {
        command: String,
        task_name: String,
        source: std::io::Error,
    },
    #[snafu(display(
        "Command '{}' for task '{}' failed with exit code {}",
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

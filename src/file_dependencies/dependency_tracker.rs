use std::env;
use std::{collections::HashMap, path::PathBuf};

use bitcode::{Decode, Encode};
use compio::fs;
use snafu::{ResultExt, Snafu};
use time::Time;
use tracing::{debug, info, warn};

use crate::ext::{BestEffortPathExt, SystemTimeExt};
use crate::tasks::{Task, TaskTrait};

const STANDARD_DEPENDENCY_FILE_PATH: &str = ".tessy/dependencies.bitcode";

fn get_standard_dependency_file_path() -> PathBuf {
    PathBuf::from(STANDARD_DEPENDENCY_FILE_PATH)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Decode, Encode)]
pub struct DependencyTracker {
    dependencies: HashMap<String, HashMap<String, Option<Time>>>,
}

impl DependencyTracker {
    /// Reads the dependency tracker from the standard file path
    pub async fn read() -> Self {
        debug!(
            "Reading dependency tracker from {}",
            get_standard_dependency_file_path().best_effort_path_display()
        );
        let path = get_standard_dependency_file_path();
        let bytes = match fs::read(path).await {
            Ok(bytes) => bytes,
            Err(_) => {
                info!("No existing dependency tracker found, starting fresh");
                return Self::default();
            }
        };
        let result = bitcode::decode::<Self>(&bytes).unwrap_or(Self::default());
        debug!("Successfully read dependency tracker: {:?}", result);
        result
    }

    pub fn add_tasks_dependencies(
        &mut self,
        tasks: &[Task],
    ) -> Result<(), DependencyTrackerCreationError> {
        for task in tasks {
            self.add_task_dependencies(task)?;
        }
        Ok(())
    }

    pub fn add_task_dependencies(
        &mut self,
        task: &Task,
    ) -> Result<(), DependencyTrackerCreationError> {
        let id = task.id();
        let inputs = task.inputs();
        let deps = Self::get_dependencies_from_inputs(inputs)?;

        self.dependencies.insert(id, deps);
        Ok(())
    }

    pub fn is_task_up_to_date(&self, task: &Task) -> bool {
        let id = task.id();
        let saved_dependencies = match self.dependencies.get(&id) {
            Some(deps) => deps,
            None => return false,
        };

        let inputs = task.inputs();
        let new_dependencies = match Self::get_dependencies_from_inputs(inputs) {
            Ok(deps) => deps,
            Err(_) => {
                warn!("Failed to get dependencies for task '{}'", id);
                return false;
            }
        };

        Self::dependencies_match(saved_dependencies, &new_dependencies)
    }

    /// Compares two dependency maps to determine if they match
    fn dependencies_match(
        saved_dependencies: &HashMap<String, Option<Time>>,
        new_dependencies: &HashMap<String, Option<Time>>,
    ) -> bool {
        // Check if all files exist in both maps with matching modification times
        for (file_path, new_time) in new_dependencies {
            match saved_dependencies.get(file_path) {
                Some(saved_time) => {
                    if saved_time != new_time {
                        return false;
                    }
                }
                None => return false,
            }
        }

        // All files match
        true
    }

    /// Saves the dependency tracker to the standard file path
    pub async fn write(&self) {
        // Ensure the directory exists
        if let Some(parent) = get_standard_dependency_file_path().parent() {
            let _ = fs::create_dir_all(parent).await;
        }

        let bytes = bitcode::encode(self);
        let _ = fs::write(get_standard_dependency_file_path(), bytes).await;
    }

    fn get_dependencies_from_inputs(
        inputs: &Vec<String>,
    ) -> Result<HashMap<String, Option<Time>>, DependencyTrackerCreationError> {
        let current_dir = env::current_dir().context(CurrentDirSnafu)?;

        Ok(inputs
            .iter()
            .map(|input| {
                let path = current_dir.join(&input);

                let metadata = path
                    .metadata()
                    .ok()
                    .map(|meta| meta.modified().ok())
                    .flatten()
                    .map(|time| time.to_time());

                (input.clone(), metadata)
            })
            .collect::<HashMap<String, Option<Time>>>())
    }
}

#[derive(Debug, Snafu)]
pub enum DependencyTrackerCreationError {
    #[snafu(display("Failed to obtain current dir"))]
    CurrentDirError { source: std::io::Error },
}

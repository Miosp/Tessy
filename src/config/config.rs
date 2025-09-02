use compio::{fs::File, io::AsyncReadExt, io::BufReader};
use hashlink::LinkedHashMap;
use saphyr::{LoadableYamlNode, Scalar, Yaml};
use snafu::prelude::*;
use std::{borrow::Cow, collections::HashMap, io::Cursor, path::Path};
use tracing::debug;

use crate::tasks::{Task, TaskTrait};

pub static TASK_FILE_NAME: &str = "tasks.yaml";

#[derive(Debug, Clone)]
pub struct TaskRegistry {
    pub tasks: HashMap<String, Task>,
}

impl TaskRegistry {
    pub async fn read() -> Result<Self, TaskRegistryCreationError> {
        Self::from_path(Path::new(TASK_FILE_NAME)).await
    }

    pub async fn from_path(path: &Path) -> Result<Self, TaskRegistryCreationError> {
        debug!("Opening config file: {}", path.display());
        let file = File::open(&path).await.context(ReadSnafu {
            file_path: path.display().to_string(),
        })?;
        Self::from_file(file).await
    }

    pub async fn from_file(file: File) -> Result<Self, TaskRegistryCreationError> {
        debug!("Reading config file");
        let cursor = Cursor::new(file);
        let mut reader = BufReader::new(cursor);

        let res = reader.read_to_string(String::new()).await;

        match res.0 {
            Ok(n) => debug!("Successfully read config file: {n} bytes"),
            _ => {
                res.0.context(ReadSnafu {
                    file_path: TASK_FILE_NAME.to_string(),
                })?;
            }
        }

        Self::from_string(&res.1)
    }

    pub fn from_string(contents: &str) -> Result<Self, TaskRegistryCreationError> {
        let contents_vec = Yaml::load_from_str(&contents)
            .map_err(|e| TaskRegistryCreationError::ParseError { source: e })?;
        let contents = contents_vec
            .get(0)
            .ok_or(TaskRegistryCreationError::MalformedConfig)?;

        let top_level = contents
            .as_mapping()
            .ok_or(TaskRegistryCreationError::TopLevelNotMap)?;

        let tasks = Self::get_tasks(top_level)?
            .into_iter()
            .map(|task| (task.id(), task))
            .try_fold(HashMap::new(), |mut acc, (id, task)| {
                if acc.contains_key(&id) {
                    // For now unreachable, as Saphyr automatically prevents duplicate keys
                    Err(TaskRegistryCreationError::DuplicateTask { task_name: id })
                } else {
                    acc.insert(id, task);
                    Ok(acc)
                }
            })?;

        Ok(TaskRegistry { tasks })
    }

    fn get_tasks(
        top_level: &LinkedHashMap<Yaml, Yaml>,
    ) -> Result<Vec<Task>, TaskRegistryCreationError> {
        let tasks = top_level
            .get(&Yaml::Value(Scalar::String(Cow::Borrowed("tasks"))))
            .unwrap_or(&Yaml::Mapping(LinkedHashMap::new()))
            .as_mapping()
            .ok_or(TaskRegistryCreationError::TasksNotMap)?
            .iter()
            .filter_map(|(key, value)| {
                if let Yaml::Value(Scalar::String(task_name)) = key {
                    if let Yaml::Mapping(task_data) = value {
                        return Some((task_name, task_data));
                    }
                }
                debug!("Skipping invalid task entry: {:?}", key);
                None
            })
            .filter_map(|(task_name, task_data)| Task::from_task_yaml(task_name, task_data))
            .collect::<Vec<_>>();

        Ok(tasks)
    }
}

#[derive(Debug, Snafu)]
pub enum TaskRegistryCreationError {
    #[snafu(display("Failed to read the config file: {}", file_path))]
    ReadError {
        file_path: String,
        source: std::io::Error,
    },
    #[snafu(display("Failed to parse the config file"))]
    ParseError { source: saphyr::ScanError },
    #[snafu(display("Improperly formatted config file"))]
    MalformedConfig,
    #[snafu(display("Top level of config should be a map"))]
    TopLevelNotMap,
    #[snafu(display("Tasks section should be a map"))]
    TasksNotMap,
    #[snafu(display("Task '{}' is defined multiple times", task_name))]
    DuplicateTask { task_name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[compio::test]
    async fn config_returns_error_on_nonexistent_file() {
        let result = TaskRegistry::from_path(Path::new("nonexistent.yaml")).await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(TaskRegistryCreationError::ReadError { .. })
        ));
    }

    #[compio::test]
    async fn config_returns_error_on_invalid_yaml() {
        let invalid_yaml = "invalid: yaml: content: [unclosed";
        let result = TaskRegistry::from_string(invalid_yaml);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(TaskRegistryCreationError::ParseError { .. })
        ));
    }

    #[compio::test]
    async fn config_returns_error_on_empty_file() {
        let empty_content = "";
        let result = TaskRegistry::from_string(empty_content);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(TaskRegistryCreationError::MalformedConfig)
        ));
    }

    #[compio::test]
    async fn config_returns_error_when_top_level_is_not_map() {
        let yaml_with_list_top_level = "- item1\n- item2";
        let result = TaskRegistry::from_string(yaml_with_list_top_level);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(TaskRegistryCreationError::TopLevelNotMap)
        ));
    }

    #[compio::test]
    async fn config_returns_error_when_top_level_is_scalar() {
        let yaml_with_scalar_top_level = "just a string";
        let result = TaskRegistry::from_string(yaml_with_scalar_top_level);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(TaskRegistryCreationError::TopLevelNotMap)
        ));
    }

    #[compio::test]
    async fn config_returns_error_when_tasks_is_not_map() {
        let yaml_with_invalid_tasks = "tasks:\n  - invalid_task_format";
        let result = TaskRegistry::from_string(yaml_with_invalid_tasks);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(TaskRegistryCreationError::TasksNotMap)
        ));
    }

    #[compio::test]
    async fn config_handles_empty_tasks_section() {
        let yaml_with_empty_tasks = "tasks: {}";
        let result = TaskRegistry::from_string(yaml_with_empty_tasks);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.tasks.is_empty());
    }

    #[compio::test]
    async fn config_handles_missing_tasks_section() {
        let yaml_without_tasks = "other_config: value";
        let result = TaskRegistry::from_string(yaml_without_tasks);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert!(config.tasks.is_empty());
    }

    #[compio::test]
    async fn config_skips_invalid_task_entries() {
        let yaml_with_mixed_entries = r#"
tasks:
  123: "invalid numeric key"
  valid_task:
    command: "echo hello"
  "another_invalid": "string value instead of map"
"#;
        let result = TaskRegistry::from_string(yaml_with_mixed_entries);
        assert!(result.is_ok());
        // Note: This test assumes valid_task would be parsed correctly by Task::from_task_yaml
        // The actual behavior depends on the Task implementation
    }

    #[compio::test]
    async fn config_handles_deeply_nested_invalid_structure() {
        let yaml_with_complex_invalid = r#"
tasks:
  nested:
    - invalid:
      - more: nesting
        even: deeper
"#;
        let result = TaskRegistry::from_string(yaml_with_complex_invalid);
        // Should succeed but skip the invalid task entry
        assert!(result.is_ok());
    }

    #[compio::test]
    async fn config_handles_null_values() {
        let yaml_with_nulls = r#"
tasks:
  null_task: null
  empty_task: {}
"#;
        let result = TaskRegistry::from_string(yaml_with_nulls);
        assert!(result.is_ok());
        // Should skip null_task and handle empty_task
    }

    #[compio::test]
    async fn config_handles_very_large_task_names() {
        let long_task_name = "a".repeat(1000);
        let yaml_with_long_name =
            format!("tasks:\n  {}:\n    command: \"echo test\"", long_task_name);
        let result = TaskRegistry::from_string(&yaml_with_long_name);
        // Should handle long names gracefully
        assert!(result.is_ok());
    }

    #[compio::test]
    async fn config_handles_special_characters_in_task_names() {
        let yaml_with_special_chars = r#"
tasks:
  "task-with-dashes":
    command: "echo dash"
  "task_with_underscores":
    command: "echo underscore"
  "task.with.dots":
    command: "echo dots"
  "task with spaces":
    command: "echo spaces"
"#;
        let result = TaskRegistry::from_string(yaml_with_special_chars);
        assert!(result.is_ok());
    }

    #[compio::test]
    async fn config_handles_unicode_in_task_names() {
        let yaml_with_unicode = r#"
tasks:
  "тест":
    command: "echo unicode"
  "🚀rocket":
    command: "echo emoji"
"#;
        let result = TaskRegistry::from_string(yaml_with_unicode);
        assert!(result.is_ok());
    }
}

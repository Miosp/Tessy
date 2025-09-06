use std::env;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use bincode::{Decode, Encode};
use compio::fs;
use tracing::{debug, info, warn};

use crate::ext::{AsyncTryFrom, BestEffortPathExt};
use crate::file_dependencies::FileFingerprint;
use crate::tasks::{Task, TaskTrait};

const STANDARD_DEPENDENCY_FILE_PATH: &str = ".tessy/dependencies.bincode.zstd";

fn get_standard_dependency_file_path() -> PathBuf {
    PathBuf::from(STANDARD_DEPENDENCY_FILE_PATH)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct DependencyTracker {
    dependencies: HashMap<String, HashMap<PathBuf, FileFingerprint>>,
}

impl DependencyTracker {
    /// Reads the dependency tracker from the standard file path
    pub async fn read() -> Self {
        let path = get_standard_dependency_file_path();
        Self::read_from_path(&path).await
    }

    pub async fn read_from_path(path: &Path) -> Self {
        debug!(
            "Reading dependency tracker from {}",
            get_standard_dependency_file_path().best_effort_path_display()
        );
        let bytes = match fs::read(path).await {
            Ok(bytes) => bytes,
            Err(e) => {
                info!(
                    "No existing dependency tracker found, starting fresh: {}",
                    e
                );
                return Self::default();
            }
        };

        Self::read_from_bytes(&bytes).await
    }

    pub async fn read_from_bytes(bytes: &[u8]) -> Self {
        debug!("Reading dependency tracker from bytes");
        let decompressed_bytes = match zstd::decode_all(&bytes[..]) {
            Ok(decompressed) => decompressed,
            Err(e) => {
                warn!("Failed to decompress dependency tracker: {}", e);
                return Self::default();
            }
        };

        debug!("Deserializing dependency tracker");
        let result: Self = match bincode::decode_from_slice(
            &decompressed_bytes[..],
            bincode::config::standard(),
        ) {
            Ok(result) => result.0,
            Err(e) => {
                warn!("Failed to read dependency tracker: ({}), starting fresh", e);
                return Self::default();
            }
        };

        info!(
            "Successfully loaded dependency tracker with {} tasks and {} total dependencies",
            result.dependencies.len(),
            result
                .dependencies
                .values()
                .map(|deps| deps.len())
                .sum::<usize>()
        );
        debug!("Successfully read dependency tracker: {:?}", result);
        result
    }

    pub async fn add_tasks_dependencies(&mut self, tasks: impl Iterator<Item = &Task>) {
        for task in tasks {
            let deps = Self::get_dependencies_from_inputs(&task.inputs()).await;
            self.dependencies.insert(task.id(), deps);
        }
    }

    pub async fn is_task_up_to_date(&self, task: &Task) -> bool {
        let id = task.id();
        info!("Checking if task '{}' is up to date", id);

        let saved_dependencies = match self.dependencies.get(&id) {
            Some(deps) => deps,
            None => {
                info!(
                    "No saved dependencies found for task '{}', marking as out of date",
                    id
                );
                return false;
            }
        };

        let inputs = task.inputs();
        let new_dependencies = Self::get_dependencies_from_inputs(&inputs).await;

        let is_up_to_date = saved_dependencies == &new_dependencies;

        is_up_to_date
    }

    /// Saves the dependency tracker to the standard file path
    pub async fn write(&self) {
        let dep_file_path = get_standard_dependency_file_path();
        self.write_into_path(&dep_file_path).await;
    }

    pub async fn write_into_path(&self, path: &Path) {
        info!(
            "Writing dependency tracker with {} tasks to {}",
            self.dependencies.len(),
            path.best_effort_path_display()
        );

        // Ensure the directory exists
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent).await;
        }

        let encoded_bytes = match bincode::encode_to_vec(self, bincode::config::standard()) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("Failed to serialize dependency tracker: {}", e);
                return;
            }
        };

        // Compress the data
        let compressed_bytes = match zstd::encode_all(&encoded_bytes[..], 3) {
            Ok(compressed) => {
                debug!(
                    "Compressed dependency tracker: {} bytes -> {} bytes ({:.1}% reduction)",
                    encoded_bytes.len(),
                    compressed.len(),
                    100.0 * (1.0 - compressed.len() as f64 / encoded_bytes.len() as f64)
                );
                compressed
            }
            Err(e) => {
                warn!("Failed to compress dependency tracker: {}", e);
                return;
            }
        };

        let write_result = fs::write(path, compressed_bytes).await;
        match write_result.0 {
            Ok(_) => info!("Successfully saved dependency tracker"),
            Err(e) => warn!("Failed to write dependency tracker file: {}", e),
        }
    }

    async fn get_dependencies_from_inputs(inputs: &[String]) -> HashMap<PathBuf, FileFingerprint> {
        let current_dir = match env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                warn!("Failed to get current directory: {}", e);
                return HashMap::new();
            }
        };

        let mut all_dependencies = HashMap::new();

        for input in inputs {
            let path = current_dir.join(input);
            if let Some(deps) = Self::get_dependencies_from_input(input, &path).await {
                for (dep_path, fingerprint) in deps {
                    all_dependencies.insert(dep_path, fingerprint);
                }
            } else {
                info!("No dependencies found for input '{}'", input);
            }
        }
        all_dependencies
    }

    async fn get_dependencies_from_input(
        input: &str,
        path: &Path,
    ) -> Option<Vec<(PathBuf, FileFingerprint)>> {
        debug!("Analyzing path: '{}'", path.best_effort_path_display());

        if !path.exists() {
            debug!("Path '{}' does not exist", path.best_effort_path_display());
            return None;
        }

        if path.is_file() {
            debug!("Processing file: '{}'", path.best_effort_path_display());
            return FileFingerprint::async_try_from(path)
                .await
                .ok()
                .map(|fingerprint| {
                    debug!("Created fingerprint for file: '{}'", input);
                    vec![(path.to_path_buf(), fingerprint)]
                });
        }

        if path.is_dir() {
            debug!(
                "Processing directory: '{}'",
                path.best_effort_path_display()
            );
            return Self::get_dependencies_from_directory(path).await;
        }

        warn!(
            "Input path '{}' is neither file nor directory",
            path.best_effort_path_display()
        );
        None
    }

    async fn get_dependencies_from_directory(
        path: &Path,
    ) -> Option<Vec<(PathBuf, FileFingerprint)>> {
        debug!("Scanning directory: '{}'", path.best_effort_path_display());

        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) => {
                warn!(
                    "Failed to read directory '{}': {}",
                    path.best_effort_path_display(),
                    e
                );
                return None;
            }
        };

        let mut all_dependencies = Vec::new();
        let mut file_count = 0;
        let mut dir_count = 0;

        for entry in entries.filter_map(|entry| entry.ok()) {
            let entry_path = entry.path();

            if entry_path.is_file() {
                file_count += 1;
                if let Ok(fingerprint) =
                    Box::pin(FileFingerprint::async_try_from(&entry_path)).await
                {
                    all_dependencies.push((entry_path, fingerprint));
                }
            } else if entry_path.is_dir() {
                dir_count += 1;
                if let Some(dir_deps) =
                    Box::pin(Self::get_dependencies_from_directory(&entry_path)).await
                {
                    all_dependencies.extend(dir_deps);
                }
            }
        }

        debug!(
            "Directory '{}' scan complete: {} files, {} subdirs, {} total dependencies",
            path.best_effort_path_display(),
            file_count,
            dir_count,
            all_dependencies.len()
        );

        Some(all_dependencies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::ExecuteTask;
    use hashlink::LinkedHashMap;
    use saphyr::{Scalar, Yaml};
    use std::borrow::Cow;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    // Helper function to create a test task
    fn create_test_task(name: &str, inputs: Vec<String>, dependencies: Vec<String>) -> Task {
        let mut task_yaml = LinkedHashMap::new();
        task_yaml.insert(
            Yaml::Value(Scalar::String(Cow::Borrowed("command"))),
            Yaml::Value(Scalar::String(Cow::Borrowed("echo test"))),
        );

        if !inputs.is_empty() {
            let inputs_yaml: Vec<Yaml> = inputs
                .iter()
                .map(|s| Yaml::Value(Scalar::String(Cow::Borrowed(s))))
                .collect();
            task_yaml.insert(
                Yaml::Value(Scalar::String(Cow::Borrowed("inputs"))),
                Yaml::Sequence(inputs_yaml),
            );
        }

        if !dependencies.is_empty() {
            let deps_yaml: Vec<Yaml> = dependencies
                .iter()
                .map(|s| Yaml::Value(Scalar::String(Cow::Borrowed(s))))
                .collect();
            task_yaml.insert(
                Yaml::Value(Scalar::String(Cow::Borrowed("dependencies"))),
                Yaml::Sequence(deps_yaml),
            );
        }

        Task::Execute(ExecuteTask::from_task_yaml(name, &task_yaml).unwrap())
    }

    #[compio::test]
    async fn test_default_dependency_tracker() {
        let tracker = DependencyTracker::default();
        assert!(tracker.dependencies.is_empty());
    }

    #[compio::test]
    async fn test_read_from_nonexistent_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nonexistent_path = temp_dir.path().join("nonexistent.bincode.zstd");

        let tracker = DependencyTracker::read_from_path(&nonexistent_path).await;
        assert!(tracker.dependencies.is_empty());
    }

    #[compio::test]
    async fn test_write_and_read_empty_tracker() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("test_dependencies.bincode.zstd");

        let original_tracker = DependencyTracker::default();
        original_tracker.write_into_path(&file_path).await;

        let loaded_tracker = DependencyTracker::read_from_path(&file_path).await;
        assert_eq!(original_tracker, loaded_tracker);
    }

    #[compio::test]
    async fn test_write_and_read_tracker_with_dependencies() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let file_path = temp_dir.path().join("test_dependencies.bincode.zstd");

        // Create a test file for fingerprinting
        let mut test_file = NamedTempFile::new_in(&temp_dir).expect("Failed to create temp file");
        writeln!(test_file, "test content").expect("Failed to write to temp file");
        let test_file_path = test_file.path().to_path_buf();

        let mut original_tracker = DependencyTracker::default();

        // Create a task with the test file as input
        let task = create_test_task(
            "test_task",
            vec![test_file_path.to_string_lossy().to_string()],
            vec![],
        );

        original_tracker
            .add_tasks_dependencies(std::iter::once(&task))
            .await;
        original_tracker.write_into_path(&file_path).await;

        let loaded_tracker = DependencyTracker::read_from_path(&file_path).await;
        assert_eq!(original_tracker, loaded_tracker);
        assert_eq!(loaded_tracker.dependencies.len(), 1);
        assert!(loaded_tracker.dependencies.contains_key("test_task"));
    }

    #[compio::test]
    async fn test_add_tasks_dependencies_with_files() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create test files
        let mut file1 = NamedTempFile::new_in(&temp_dir).expect("Failed to create temp file");
        writeln!(file1, "content 1").expect("Failed to write to temp file");
        let file1_path = file1.path().to_path_buf();

        let mut file2 = NamedTempFile::new_in(&temp_dir).expect("Failed to create temp file");
        writeln!(file2, "content 2").expect("Failed to write to temp file");
        let file2_path = file2.path().to_path_buf();

        let mut tracker = DependencyTracker::default();

        // Create tasks with different inputs
        let task1 = create_test_task(
            "task1",
            vec![file1_path.to_string_lossy().to_string()],
            vec![],
        );
        let task2 = create_test_task(
            "task2",
            vec![file2_path.to_string_lossy().to_string()],
            vec![],
        );

        tracker
            .add_tasks_dependencies([&task1, &task2].iter().copied())
            .await;

        assert_eq!(tracker.dependencies.len(), 2);
        assert!(tracker.dependencies.contains_key("task1"));
        assert!(tracker.dependencies.contains_key("task2"));

        // Check that each task has its file dependency
        let task1_deps = &tracker.dependencies["task1"];
        let task2_deps = &tracker.dependencies["task2"];

        assert_eq!(task1_deps.len(), 1);
        assert_eq!(task2_deps.len(), 1);
        assert!(task1_deps.contains_key(&file1_path));
        assert!(task2_deps.contains_key(&file2_path));
    }

    #[compio::test]
    async fn test_add_tasks_dependencies_with_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let sub_dir = temp_dir.path().join("subdir");
        std::fs::create_dir(&sub_dir).expect("Failed to create subdirectory");

        // Create files in the directory
        let file1_path = sub_dir.join("file1.txt");
        std::fs::write(&file1_path, "content 1").expect("Failed to write file");

        let file2_path = sub_dir.join("file2.txt");
        std::fs::write(&file2_path, "content 2").expect("Failed to write file");

        let mut tracker = DependencyTracker::default();

        let task = create_test_task(
            "dir_task",
            vec![sub_dir.to_string_lossy().to_string()],
            vec![],
        );

        tracker.add_tasks_dependencies(std::iter::once(&task)).await;

        assert_eq!(tracker.dependencies.len(), 1);
        let task_deps = &tracker.dependencies["dir_task"];

        // Should have both files from the directory
        assert_eq!(task_deps.len(), 2);
        assert!(task_deps.contains_key(&file1_path));
        assert!(task_deps.contains_key(&file2_path));
    }

    #[compio::test]
    async fn test_is_task_up_to_date_no_previous_dependencies() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let mut test_file = NamedTempFile::new_in(&temp_dir).expect("Failed to create temp file");
        writeln!(test_file, "test content").expect("Failed to write to temp file");

        let tracker = DependencyTracker::default();
        let task = create_test_task(
            "new_task",
            vec![test_file.path().to_string_lossy().to_string()],
            vec![],
        );

        // Task should be out of date if no previous dependencies exist
        assert!(!tracker.is_task_up_to_date(&task).await);
    }

    #[compio::test]
    async fn test_is_task_up_to_date_unchanged_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let mut test_file = NamedTempFile::new_in(&temp_dir).expect("Failed to create temp file");
        writeln!(test_file, "test content").expect("Failed to write to temp file");

        let mut tracker = DependencyTracker::default();
        let task = create_test_task(
            "test_task",
            vec![test_file.path().to_string_lossy().to_string()],
            vec![],
        );

        // Add initial dependencies
        tracker.add_tasks_dependencies(std::iter::once(&task)).await;

        // Task should be up to date since file hasn't changed
        assert!(tracker.is_task_up_to_date(&task).await);
    }

    #[compio::test]
    async fn test_is_task_up_to_date_changed_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let test_file_path = temp_dir.path().join("test_file.txt");

        // Create initial file
        std::fs::write(&test_file_path, "initial content").expect("Failed to write file");

        let mut tracker = DependencyTracker::default();
        let task = create_test_task(
            "test_task",
            vec![test_file_path.to_string_lossy().to_string()],
            vec![],
        );

        // Add initial dependencies
        tracker.add_tasks_dependencies(std::iter::once(&task)).await;

        // Wait a bit to ensure different modification time
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify the file
        std::fs::write(&test_file_path, "modified content").expect("Failed to write file");

        // Task should be out of date since file has changed
        assert!(!tracker.is_task_up_to_date(&task).await);
    }

    #[compio::test]
    async fn test_is_task_up_to_date_nonexistent_file() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nonexistent_path = temp_dir.path().join("nonexistent.txt");

        let mut tracker = DependencyTracker::default();
        let task = create_test_task(
            "test_task",
            vec![nonexistent_path.to_string_lossy().to_string()],
            vec![],
        );

        // Add dependencies (will be empty since file doesn't exist)
        tracker.add_tasks_dependencies(std::iter::once(&task)).await;

        // Task should be up to date if it has no dependencies due to nonexistent files
        assert!(tracker.is_task_up_to_date(&task).await);
    }

    #[compio::test]
    async fn test_read_from_corrupted_bytes() {
        let corrupted_data = b"this is not valid compressed bincode data";
        let tracker = DependencyTracker::read_from_bytes(corrupted_data).await;

        // Should return default tracker when reading corrupted data
        assert!(tracker.dependencies.is_empty());
    }

    #[compio::test]
    async fn test_read_from_invalid_compression() {
        // Create valid bincode but with invalid compression
        let tracker = DependencyTracker::default();
        let encoded = bincode::encode_to_vec(&tracker, bincode::config::standard()).unwrap();

        // Use the encoded data directly without compression (should fail decompression)
        let result_tracker = DependencyTracker::read_from_bytes(&encoded).await;

        // Should return default tracker when decompression fails
        assert!(result_tracker.dependencies.is_empty());
    }

    #[compio::test]
    async fn test_get_dependencies_from_nested_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let nested_dir = temp_dir.path().join("level1").join("level2");
        std::fs::create_dir_all(&nested_dir).expect("Failed to create nested directory");

        // Create files at different levels
        let file1_path = temp_dir.path().join("root_file.txt");
        std::fs::write(&file1_path, "root content").expect("Failed to write file");

        let file2_path = temp_dir.path().join("level1").join("level1_file.txt");
        std::fs::write(&file2_path, "level1 content").expect("Failed to write file");

        let file3_path = nested_dir.join("level2_file.txt");
        std::fs::write(&file3_path, "level2 content").expect("Failed to write file");

        let mut tracker = DependencyTracker::default();
        let task = create_test_task(
            "nested_task",
            vec![temp_dir.path().to_string_lossy().to_string()],
            vec![],
        );

        tracker.add_tasks_dependencies(std::iter::once(&task)).await;

        let task_deps = &tracker.dependencies["nested_task"];

        // Should find all files in the directory tree
        assert_eq!(task_deps.len(), 3);
        assert!(task_deps.contains_key(&file1_path));
        assert!(task_deps.contains_key(&file2_path));
        assert!(task_deps.contains_key(&file3_path));
    }

    #[compio::test]
    async fn test_multiple_tasks_same_dependencies() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let shared_file = temp_dir.path().join("shared.txt");
        std::fs::write(&shared_file, "shared content").expect("Failed to write file");

        let mut tracker = DependencyTracker::default();

        let task1 = create_test_task(
            "task1",
            vec![shared_file.to_string_lossy().to_string()],
            vec![],
        );
        let task2 = create_test_task(
            "task2",
            vec![shared_file.to_string_lossy().to_string()],
            vec![],
        );

        tracker
            .add_tasks_dependencies([&task1, &task2].iter().copied())
            .await;

        assert_eq!(tracker.dependencies.len(), 2);

        let task1_deps = &tracker.dependencies["task1"];
        let task2_deps = &tracker.dependencies["task2"];

        // Both tasks should have the same dependency
        assert_eq!(task1_deps.len(), 1);
        assert_eq!(task2_deps.len(), 1);
        assert!(task1_deps.contains_key(&shared_file));
        assert!(task2_deps.contains_key(&shared_file));

        // The fingerprints should be the same
        assert_eq!(task1_deps[&shared_file], task2_deps[&shared_file]);
    }

    #[compio::test]
    async fn test_empty_inputs() {
        let mut tracker = DependencyTracker::default();
        let task = create_test_task("empty_task", vec![], vec![]);

        tracker.add_tasks_dependencies(std::iter::once(&task)).await;

        let task_deps = &tracker.dependencies["empty_task"];
        assert!(task_deps.is_empty());

        // Empty task should be up to date
        assert!(tracker.is_task_up_to_date(&task).await);
    }
}

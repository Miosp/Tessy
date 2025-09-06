use std::env;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use compio::fs;
use tracing::{debug, info, warn};

use crate::ext::{AsyncTryFrom, BestEffortPathExt};
use crate::file_dependencies::FileFingerprint;
use crate::tasks::{Task, TaskTrait};

const STANDARD_DEPENDENCY_FILE_PATH: &str = ".tessy/dependencies.bincode.zstd";

fn get_standard_dependency_file_path() -> PathBuf {
    PathBuf::from(STANDARD_DEPENDENCY_FILE_PATH)
}

#[derive(Debug, Clone, PartialEq, Eq, Default, bincode::Encode, bincode::Decode)]
pub struct DependencyTracker {
    dependencies: HashMap<String, HashMap<PathBuf, FileFingerprint>>,
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
            Err(e) => {
                info!(
                    "No existing dependency tracker found, starting fresh: {}",
                    e
                );
                return Self::default();
            }
        };

        // Decompress the data
        let decompressed_bytes = match zstd::decode_all(&bytes[..]) {
            Ok(decompressed) => decompressed,
            Err(e) => {
                warn!("Failed to decompress dependency tracker: {}", e);
                return Self::default();
            }
        };

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
            let deps = Self::get_dependencies_from_inputs(task.inputs()).await;
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
        let new_dependencies = Self::get_dependencies_from_inputs(inputs).await;

        let is_up_to_date = saved_dependencies == &new_dependencies;

        is_up_to_date
    }

    /// Saves the dependency tracker to the standard file path
    pub async fn write(&self) {
        info!(
            "Writing dependency tracker with {} tasks to {}",
            self.dependencies.len(),
            get_standard_dependency_file_path().best_effort_path_display()
        );

        // Ensure the directory exists
        if let Some(parent) = get_standard_dependency_file_path().parent() {
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

        let write_result = fs::write(get_standard_dependency_file_path(), compressed_bytes).await;
        match write_result.0 {
            Ok(_) => info!("Successfully saved dependency tracker"),
            Err(e) => warn!("Failed to write dependency tracker file: {}", e),
        }
    }

    async fn get_dependencies_from_inputs(
        inputs: &Vec<String>,
    ) -> HashMap<PathBuf, FileFingerprint> {
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
                    vec![(input.into(), fingerprint)]
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

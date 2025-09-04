use std::path::{Path, PathBuf};

pub fn best_effort_path_display(path: &Path) -> String {
    match path.canonicalize() {
        Ok(canonical_path) => canonical_path.display().to_string(),
        Err(_) => {
            // Try to make an absolute path as fallback with normalization
            let absolute_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                match std::env::current_dir() {
                    Ok(current_dir) => current_dir.join(path),
                    Err(_) => path.to_path_buf(),
                }
            };

            // Normalize the path by resolving . and .. components
            let normalized = normalize_path(&absolute_path);
            normalized.display().to_string()
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::CurDir => {
                // Skip current directory components
            }
            std::path::Component::ParentDir => {
                // Pop the last component if it's not a root
                if !components.is_empty()
                    && !matches!(components.last(), Some(std::path::Component::RootDir))
                {
                    components.pop();
                }
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

pub trait BestEffortPathExt {
    fn best_effort_path_display(&self) -> String;
}

impl BestEffortPathExt for Path {
    fn best_effort_path_display(&self) -> String {
        best_effort_path_display(self)
    }
}

impl BestEffortPathExt for PathBuf {
    fn best_effort_path_display(&self) -> String {
        best_effort_path_display(self)
    }
}

impl BestEffortPathExt for &str {
    fn best_effort_path_display(&self) -> String {
        best_effort_path_display(Path::new(self))
    }
}

impl BestEffortPathExt for String {
    fn best_effort_path_display(&self) -> String {
        best_effort_path_display(Path::new(self))
    }
}

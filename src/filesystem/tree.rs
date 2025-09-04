use std::env;
use std::time::SystemTime;
use std::{collections::HashMap, path::PathBuf};

use snafu::{ResultExt, Snafu};
use tracing::warn;

/// Represents the type of a filesystem node
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesystemNode {
    File {
        size: Option<u64>,
        modified_time: Option<SystemTime>,
    },
    Directory {
        children: HashMap<String, FilesystemNode>,
    },
}

impl FilesystemNode {
    pub fn try_from_string_paths(paths: &Vec<String>) -> Result<(), FilesystemNodeCreationError> {
        let current_dir = env::current_dir().context(CurrentDirSnafu)?;

        let mut root = Self::root();

        let mapped = paths
            .iter()
            .map(|path| {
                current_dir.join(&PathBuf::from(path))
                // .components()
                // .map(|c| c.as_os_str().to_string_lossy().to_string())
                // .collect::<Vec<_>>()
            })
            .fold(root, |mut current, path| {
                let res = current.try_insert_path(path);
                match res {
                    Ok(()) => current,
                    Err(e) => {
                        warn!("Failed to insert path: {}", e.path.display());
                        current
                    }
                }
            });

        Ok(())
    }

    pub fn try_insert_path(&mut self, path: PathBuf) -> Result<(), CannotInsertIntoFileError> {
        let mut components = path.components().peekable();
        let mut current = self;

        while let Some(component) = components.next() {
            let name = component.as_os_str().to_string_lossy().to_string();

            if components.peek().is_none() {
                // Last component, insert file
                match current {
                    FilesystemNode::Directory { children } => {
                        children.insert(
                            name,
                            FilesystemNode::File {
                                size: None,
                                modified_time: None,
                            },
                        );
                    }
                    FilesystemNode::File { .. } => {
                        return Err(CannotInsertIntoFileError { path });
                    }
                }
            } else {
                // Intermediate component, ensure directory exists
                match current {
                    FilesystemNode::Directory { children } => {
                        current = children.entry(name.clone()).or_insert_with(|| {
                            FilesystemNode::Directory {
                                children: HashMap::new(),
                            }
                        });
                    }
                    FilesystemNode::File { .. } => {
                        return Err(CannotInsertIntoFileError { path });
                    }
                }
            }
        }

        Ok(())
    }

    pub fn root() -> Self {
        FilesystemNode::Directory {
            children: HashMap::new(),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum FilesystemNodeCreationError {
    #[snafu(display("Failed to obtain current dir"))]
    CurrentDirError { source: std::io::Error },
}

#[derive(Debug, Snafu)]
#[snafu(display("Cannot insert a file into a file"))]
pub struct CannotInsertIntoFileError {
    path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_from_string_paths() {
        let paths = vec!["path/to/file1.txt".into(), "path/to/file2.txt".into()];
        let result = FilesystemNode::try_from_string_paths(&paths);
        assert!(result.is_ok());
    }
}

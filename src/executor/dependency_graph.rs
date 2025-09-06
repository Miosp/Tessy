use std::collections::HashMap;
use std::collections::HashSet;

use snafu::location;
use tracing::debug;
use tracing::error;

use crate::config::task_registry::TaskRegistry;
use crate::tasks::TaskTrait;

/// Stores the dependency graph of tasks in the executor module.
/// Knowing the task dependencies and the task, which the user wants to execute,
/// we can determine which tasks depend on which
///
/// This needs to store the tasks in a child-array of parents, so that we can easily
/// mark tasks as executed
/// we need to also store the leaf tasks, which are the tasks that do not depend on any other task
/// This allows us to initialize the execution
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    task_parents: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn from_config(config: &TaskRegistry, final_task: &String) -> Self {
        // First, collect all tasks that are needed to execute the final task
        let needed_tasks = Self::collect_needed_tasks(config, final_task);
        debug!("Needed tasks for {}: {:?}", final_task, needed_tasks);

        // Only initialize task_parents for tasks that are needed
        let mut task_parents = needed_tasks
            .iter()
            .map(|task_id| (task_id.clone(), Vec::new()))
            .collect::<HashMap<_, _>>();

        // Build dependency graph only for needed tasks
        for task_id in &needed_tasks {
            if let Some(task) = config.get_task_by_id(task_id) {
                for dep_id in task.dependencies() {
                    if let Some(parents) = task_parents.get_mut(dep_id) {
                        parents.push(task_id.clone());
                    } else {
                        error!(
                            "Assumption that all task IDs should be present in the task_parents map failed {}",
                            location!()
                        );
                    }
                }
            } else {
                error!(
                    "Assumption that all task IDs should be present in the config failed {}",
                    location!()
                );
            }
        }

        debug!("Constructed dependency graph: {:?}", task_parents);
        DependencyGraph { task_parents }
    }

    pub fn get_parent_by_id(&self, task_id: impl AsRef<str>) -> Option<&Vec<String>> {
        self.task_parents.get(task_id.as_ref())
    }

    pub fn get_task_parents_iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.task_parents.iter()
    }

    /// Recursively collect all tasks needed to execute the final task
    fn collect_needed_tasks(config: &TaskRegistry, final_task: &String) -> HashSet<String> {
        let mut needed_tasks = HashSet::new();
        let mut visited = HashSet::new();

        Self::collect_dependencies_recursive(config, final_task, &mut needed_tasks, &mut visited);

        needed_tasks
    }

    /// Recursively collect dependencies for a task
    fn collect_dependencies_recursive(
        config: &TaskRegistry,
        task_id: &String,
        needed_tasks: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        // Avoid cycles
        if visited.contains(task_id) {
            return;
        }
        visited.insert(task_id.clone());

        // Add this task to the needed set
        needed_tasks.insert(task_id.clone());

        // If the task exists in config, recursively collect its dependencies
        if let Some(task) = config.get_task_by_id(task_id) {
            for dep_id in task.dependencies() {
                Self::collect_dependencies_recursive(config, dep_id, needed_tasks, visited);
            }
        } else {
            error!(
                "Assumption that all task IDs should be present in the config failed {}",
                location!()
            );
        }
    }
}

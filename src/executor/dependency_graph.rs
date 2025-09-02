use std::collections::HashMap;
use std::collections::HashSet;

use tracing::debug;

use crate::config::config::TaskRegistry;
use crate::tasks::TaskTrait;

// Stores the dependency graph of tasks in the executor module.
// Knowing the task dependencies and the task, which the user wants to execute,
// we can determine which tasks depend on which
//
// This needs to store the tasks in a child-array of parents, so that we can easily
// mark tasks as executed
// we need to also store the leaf tasks, which are the tasks that do not depend on any other task
// This allows us to initialize the execution
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub task_parents: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    pub fn from_config(config: &TaskRegistry, final_task: &String) -> Self {
        // First, collect all tasks that are needed to execute the final task
        let needed_tasks = Self::collect_needed_tasks(config, final_task);
        debug!("Needed tasks for {}: {:?}", final_task, needed_tasks);

        let mut task_parents = HashMap::new();

        // Only initialize task_parents for tasks that are needed
        for task_id in &needed_tasks {
            task_parents.insert(task_id.clone(), Vec::new());
        }

        // Build dependency graph only for needed tasks
        for task_id in &needed_tasks {
            if let Some(task) = config.tasks.get(task_id) {
                let dependencies = task.dependencies();

                if !dependencies.is_empty() {
                    for dep_id in dependencies {
                        // Only add dependencies that are also in our needed tasks set
                        if needed_tasks.contains(dep_id) {
                            if let Some(parents) = task_parents.get_mut(dep_id) {
                                parents.push(task_id.clone());
                            }
                        }
                    }
                }
            }
        }

        debug!("Constructed dependency graph: {:?}", task_parents);
        DependencyGraph { task_parents }
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
        if let Some(task) = config.tasks.get(task_id) {
            for dep_id in task.dependencies() {
                Self::collect_dependencies_recursive(config, dep_id, needed_tasks, visited);
            }
        }
    }
}

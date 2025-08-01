use hashlink::LinkedHashMap;
use saphyr::{Scalar, Yaml};

use crate::tasks::TaskTrait;

use super::TaskError;

#[derive(Debug, Clone)]
pub struct BaseTask {
    name: String,
    dependencies: Vec<String>,
}

impl TaskTrait for BaseTask {
    fn from_task_yaml(task_name: &str, task_data: &LinkedHashMap<Yaml, Yaml>) -> Option<Self> {
        let dependencies = task_data
            .get(&Yaml::Value(Scalar::String("dependsOn".into())))
            .and_then(|v| v.as_sequence())
            .map(|seq| {
                seq.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Some(BaseTask {
            name: task_name.to_string(),
            dependencies,
        })
    }

    async fn run(&self) -> Result<String, TaskError> {
        Ok(self.id())
    }

    fn id(&self) -> String {
        self.name.clone()
    }

    fn dependencies(&self) -> &Vec<String> {
        &self.dependencies
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashlink::LinkedHashMap;
    use ordered_float::OrderedFloat;
    use rstest::rstest;
    use saphyr::{Scalar, Yaml};

    #[test]
    fn test_base_task_from_task_yaml_with_dependencies() {
        let task_name = "test_task";
        let mut task_data = LinkedHashMap::new();
        let dependencies = vec![
            Yaml::Value(Scalar::String("dep1".into())),
            Yaml::Value(Scalar::String("dep2".into())),
            Yaml::Value(Scalar::String("dep3".into())),
        ];
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Sequence(dependencies),
        );

        let base_task = BaseTask::from_task_yaml(task_name, &task_data);

        assert!(base_task.is_some());
        let task = base_task.unwrap();
        assert_eq!(task.name, "test_task");
        assert_eq!(task.dependencies, vec!["dep1", "dep2", "dep3"]);
    }

    #[test]
    fn test_base_task_from_task_yaml_without_dependencies() {
        let task_name = "test_task";
        let task_data = LinkedHashMap::new();

        let base_task = BaseTask::from_task_yaml(task_name, &task_data);

        assert!(base_task.is_some());
        let task = base_task.unwrap();
        assert_eq!(task.name, "test_task");
        assert!(task.dependencies.is_empty());
    }

    #[test]
    fn test_base_task_from_task_yaml_with_empty_dependencies() {
        let task_name = "test_task";
        let mut task_data = LinkedHashMap::new();
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Sequence(vec![]),
        );

        let base_task = BaseTask::from_task_yaml(task_name, &task_data);

        assert!(base_task.is_some());
        let task = base_task.unwrap();
        assert_eq!(task.name, "test_task");
        assert!(task.dependencies.is_empty());
    }

    #[test]
    fn test_base_task_from_task_yaml_with_mixed_dependency_types() {
        let task_name = "test_task";
        let mut task_data = LinkedHashMap::new();
        let dependencies = vec![
            Yaml::Value(Scalar::String("valid_dep".into())),
            Yaml::Value(Scalar::Integer(42)), // This should be filtered out
            Yaml::Value(Scalar::String("another_valid_dep".into())),
            Yaml::Value(Scalar::FloatingPoint(OrderedFloat(3.14))), // This should be filtered out
        ];
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Sequence(dependencies),
        );

        let base_task = BaseTask::from_task_yaml(task_name, &task_data);

        assert!(base_task.is_some());
        let task = base_task.unwrap();
        assert_eq!(task.name, "test_task");
        assert_eq!(task.dependencies, vec!["valid_dep", "another_valid_dep"]);
    }

    #[test]
    fn test_base_task_from_task_yaml_with_non_sequence_depends_on() {
        let task_name = "test_task";
        let mut task_data = LinkedHashMap::new();
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Value(Scalar::String("not_a_sequence".into())),
        );

        let base_task = BaseTask::from_task_yaml(task_name, &task_data);

        assert!(base_task.is_some());
        let task = base_task.unwrap();
        assert_eq!(task.name, "test_task");
        assert!(task.dependencies.is_empty());
    }

    #[compio::test]
    async fn test_base_task_run_returns_id() {
        let task_name = "test_task";
        let task_data = LinkedHashMap::new();
        let base_task = BaseTask::from_task_yaml(task_name, &task_data).unwrap();

        let result = base_task.run().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_task");
    }

    #[test]
    fn test_base_task_id() {
        let task_name = "my_task";
        let task_data = LinkedHashMap::new();
        let base_task = BaseTask::from_task_yaml(task_name, &task_data).unwrap();

        let id = base_task.id();

        assert_eq!(id, "my_task");
    }

    #[test]
    fn test_base_task_dependencies() {
        let task_name = "test_task";
        let mut task_data = LinkedHashMap::new();
        let dependencies = vec![
            Yaml::Value(Scalar::String("dep1".into())),
            Yaml::Value(Scalar::String("dep2".into())),
        ];
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Sequence(dependencies),
        );
        let base_task = BaseTask::from_task_yaml(task_name, &task_data).unwrap();

        let deps = base_task.dependencies();

        assert_eq!(deps, &vec!["dep1", "dep2"]);
    }

    #[rstest]
    #[case("simple_task", vec![])]
    #[case("task_with_one_dep", vec!["dep1"])]
    #[case("task_with_multiple_deps", vec!["dep1", "dep2", "dep3"])]
    fn test_base_task_creation_with_various_dependencies(
        #[case] task_name: &str,
        #[case] expected_deps: Vec<&str>,
    ) {
        let mut task_data = LinkedHashMap::new();
        if !expected_deps.is_empty() {
            let dependencies: Vec<Yaml> = expected_deps
                .iter()
                .map(|dep| Yaml::Value(Scalar::String((*dep).into())))
                .collect();
            task_data.insert(
                Yaml::Value(Scalar::String("dependsOn".into())),
                Yaml::Sequence(dependencies),
            );
        }

        let base_task = BaseTask::from_task_yaml(task_name, &task_data);

        assert!(base_task.is_some());
        let task = base_task.unwrap();
        assert_eq!(task.name, task_name);
        assert_eq!(task.dependencies, expected_deps);
        assert_eq!(task.id(), task_name);
        assert_eq!(task.dependencies(), &expected_deps);
    }

    #[test]
    fn test_base_task_clone() {
        let task_name = "cloneable_task";
        let mut task_data = LinkedHashMap::new();
        let dependencies = vec![Yaml::Value(Scalar::String("dep1".into()))];
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Sequence(dependencies),
        );
        let base_task = BaseTask::from_task_yaml(task_name, &task_data).unwrap();

        let cloned_task = base_task.clone();

        assert_eq!(base_task.name, cloned_task.name);
        assert_eq!(base_task.dependencies, cloned_task.dependencies);
        assert_eq!(base_task.id(), cloned_task.id());
    }

    #[test]
    fn test_base_task_debug_format() {
        let task_name = "debug_task";
        let mut task_data = LinkedHashMap::new();
        let dependencies = vec![Yaml::Value(Scalar::String("dep1".into()))];
        task_data.insert(
            Yaml::Value(Scalar::String("dependsOn".into())),
            Yaml::Sequence(dependencies),
        );
        let base_task = BaseTask::from_task_yaml(task_name, &task_data).unwrap();

        let debug_output = format!("{:?}", base_task);

        assert!(debug_output.contains("BaseTask"));
        assert!(debug_output.contains("debug_task"));
        assert!(debug_output.contains("dep1"));
    }
}

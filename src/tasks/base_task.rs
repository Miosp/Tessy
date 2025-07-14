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

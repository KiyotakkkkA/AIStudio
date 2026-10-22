use std::collections::HashSet;
use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::Value;
use zvs_contract::{AppError, AppErrorCode, GraphBinding, RunGraph, Validate};

use super::types::{NodeDef, PermissionRequirement, StepContext};

#[async_trait]
trait ErasedNode: Send + Sync {
    fn permission_for(&self, input: &Value) -> PermissionRequirement;

    async fn execute(&self, context: &StepContext, input: Value) -> Result<Value, AppError>;

    async fn execute_tool(
        &self,
        context: &StepContext,
        input: Value,
    ) -> Result<(Value, Option<Value>), AppError>;
}

fn output_value<T: serde::Serialize>(output: T) -> Result<Value, AppError> {
    serde_json::to_value(output)
        .map_err(|error| AppError::new(AppErrorCode::ValidationFailed, error.to_string()))
}

#[async_trait]
impl<N: NodeDef> ErasedNode for N {
    fn permission_for(&self, input: &Value) -> PermissionRequirement {
        NodeDef::permission_for(self, input)
    }

    async fn execute(&self, context: &StepContext, input: Value) -> Result<Value, AppError> {
        let input: N::Input = zvs_contract::parse(input)?;
        let output = self.run(context, input).await?;
        output.validate()?;
        output_value(output)
    }

    async fn execute_tool(
        &self,
        context: &StepContext,
        input: Value,
    ) -> Result<(Value, Option<Value>), AppError> {
        let typed: N::Input = zvs_contract::parse(input.clone())?;
        let output = self.run(context, typed).await?;
        output.validate()?;
        let meta = self.tool_meta(&input, &output);
        Ok((output_value(output)?, meta))
    }
}

pub fn json_schema<T: JsonSchema>() -> Value {
    SchemaSettings::draft2020_12()
        .with(|settings| settings.inline_subschemas = true)
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

pub fn input_schema<T: JsonSchema>() -> Value {
    let mut schema = json_schema::<T>();
    absent_not_null(&mut schema);
    schema
}

fn strip_null(schema: &mut Value) {
    let Value::Object(map) = schema else {
        return;
    };
    if let Some(Value::Array(types)) = map.get_mut("type") {
        types.retain(|kind| kind != "null");
        if types.len() == 1 {
            let only = types.remove(0);
            map.insert("type".to_owned(), only);
        }
    }
    if let Some(Value::Array(values)) = map.get_mut("enum") {
        values.retain(|value| !value.is_null());
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(branches)) = map.get_mut(key) {
            branches.retain(|branch| branch.get("type").is_none_or(|kind| kind != "null"));
        }
    }
}

fn absent_not_null(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            let required: Vec<String> = map
                .get("required")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            if let Some(Value::Object(properties)) = map.get_mut("properties") {
                for (name, property) in properties.iter_mut() {
                    if !required.contains(name) {
                        strip_null(property);
                    }
                }
            }
            for value in map.values_mut() {
                absent_not_null(value);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(absent_not_null),
        _ => {}
    }
}

pub struct RegisteredNode {
    node_type: String,
    permission: PermissionRequirement,
    defers_approval: bool,
    side_effect: bool,
    input_schema: Value,
    output_schema: Value,
    node: Box<dyn ErasedNode>,
}

impl RegisteredNode {
    fn new<N: NodeDef>(node: N) -> Self {
        Self {
            node_type: node.node_type().to_owned(),
            permission: node.permission(),
            defers_approval: node.defers_approval(),
            side_effect: node.side_effect(),
            input_schema: input_schema::<N::Input>(),
            output_schema: json_schema::<N::Output>(),
            node: Box::new(node),
        }
    }

    pub fn node_type(&self) -> &str {
        &self.node_type
    }

    pub const fn permission(&self) -> &PermissionRequirement {
        &self.permission
    }

    pub fn permission_for(&self, input: &Value) -> PermissionRequirement {
        self.node.permission_for(input)
    }

    pub const fn defers_approval(&self) -> bool {
        self.defers_approval
    }

    pub const fn side_effect(&self) -> bool {
        self.side_effect
    }

    pub const fn side_effect_free(&self) -> bool {
        !self.side_effect
    }

    pub const fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    pub const fn output_schema(&self) -> &Value {
        &self.output_schema
    }

    pub async fn execute(&self, context: &StepContext, input: Value) -> Result<Value, AppError> {
        self.node.execute(context, input).await
    }

    pub async fn execute_tool(
        &self,
        context: &StepContext,
        input: Value,
    ) -> Result<(Value, Option<Value>), AppError> {
        self.node.execute_tool(context, input).await
    }
}

fn invalid(message: &str) -> AppError {
    AppError::new(AppErrorCode::ValidationFailed, message)
}

#[derive(Default)]
pub struct NodeRegistry {
    nodes: RwLock<Vec<Arc<RegisteredNode>>>,
}

impl NodeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<N: NodeDef>(&self, node: N) -> Result<(), AppError> {
        let registered = RegisteredNode::new(node);
        let mut nodes = self.nodes.write().unwrap_or_else(PoisonError::into_inner);
        if nodes
            .iter()
            .any(|node| node.node_type == registered.node_type)
        {
            return Err(AppError::new(
                AppErrorCode::Conflict,
                format!("Duplicate node type: {}", registered.node_type),
            ));
        }
        nodes.push(Arc::new(registered));
        Ok(())
    }

    pub fn unregister(&self, node_type: &str) -> bool {
        let mut nodes = self.nodes.write().unwrap_or_else(PoisonError::into_inner);
        let before = nodes.len();
        nodes.retain(|node| node.node_type != node_type);
        nodes.len() != before
    }

    pub fn list(&self) -> Vec<Arc<RegisteredNode>> {
        self.nodes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn for_tool(&self, key: &str) -> Option<Arc<RegisteredNode>> {
        let candidates: Vec<_> = self
            .list()
            .into_iter()
            .filter(|node| node.permission.tool_key() == Some(key))
            .collect();
        candidates
            .iter()
            .find(|node| node.node_type == key)
            .or_else(|| candidates.first())
            .cloned()
    }

    pub fn resolve(&self, node_type: &str) -> Result<Arc<RegisteredNode>, AppError> {
        self.get(node_type)
    }

    pub fn get(&self, node_type: &str) -> Result<Arc<RegisteredNode>, AppError> {
        self.find(node_type).ok_or_else(|| {
            AppError::new(
                AppErrorCode::ValidationFailed,
                format!("Неизвестный тип узла: {node_type}"),
            )
        })
    }

    fn find(&self, node_type: &str) -> Option<Arc<RegisteredNode>> {
        self.nodes
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|node| node.node_type == node_type)
            .cloned()
    }

    pub fn can_resume(&self, graph: &RunGraph) -> bool {
        graph.nodes.iter().all(|node| {
            self.find(&node.node_type)
                .is_some_and(|registered| registered.side_effect_free())
        })
    }

    pub fn validate(&self, graph: &RunGraph) -> Result<(), AppError> {
        let ids: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        if ids.len() != graph.nodes.len() {
            return Err(invalid("Идентификаторы узлов должны быть уникальны"));
        }
        for node in &graph.nodes {
            self.get(&node.node_type)?;
            let dependencies = node.dependencies();
            if dependencies.iter().collect::<HashSet<_>>().len() != dependencies.len() {
                return Err(invalid("Повтор зависимости"));
            }
            if dependencies
                .iter()
                .any(|dependency| !ids.contains(dependency.as_str()))
            {
                return Err(invalid("Неизвестная зависимость"));
            }
            for (_, binding) in node.bindings() {
                if let GraphBinding::Node { node_id } = binding
                    && !dependencies.contains(node_id)
                {
                    return Err(invalid("Источник данных должен быть зависимостью узла"));
                }
            }
            if node.has_bindings() && !node.input().is_object() {
                return Err(invalid("Узел с привязками должен принимать объект"));
            }
        }
        let mut completed: HashSet<&str> = HashSet::new();
        while completed.len() < ids.len() {
            let ready: Vec<&str> = graph
                .nodes
                .iter()
                .filter(|node| {
                    !completed.contains(node.id.as_str())
                        && node
                            .dependencies()
                            .iter()
                            .all(|id| completed.contains(id.as_str()))
                })
                .map(|node| node.id.as_str())
                .collect();
            if ready.is_empty() {
                return Err(invalid("Граф содержит цикл"));
            }
            completed.extend(ready);
        }
        Ok(())
    }
}

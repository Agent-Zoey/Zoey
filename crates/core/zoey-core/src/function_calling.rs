//! Function calling support for LLMs

use crate::{ZoeyError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Function definition for LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,

    /// Function description
    pub description: String,

    /// Parameters schema (JSON Schema)
    pub parameters: serde_json::Value,

    /// Whether this function is required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Function call from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name to call
    pub name: String,

    /// Arguments (JSON object)
    pub arguments: serde_json::Value,
}

/// Function execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResult {
    /// Function name that was called
    pub name: String,

    /// Result value
    pub result: serde_json::Value,

    /// Whether the call succeeded
    pub success: bool,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Function handler type
pub type FunctionHandler = Arc<
    dyn Fn(
            serde_json::Value,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>>
        + Send
        + Sync,
>;

/// Function registry for managing callable functions
pub struct FunctionRegistry {
    functions: HashMap<String, (FunctionDefinition, FunctionHandler)>,
}

impl FunctionRegistry {
    /// Create a new function registry
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Register a function
    pub fn register(&mut self, definition: FunctionDefinition, handler: FunctionHandler) {
        info!("Registering function: {}", definition.name);
        debug!("Function description: {}", definition.description);
        self.functions
            .insert(definition.name.clone(), (definition, handler));
    }

    /// Validate a function definition
    pub fn validate_definition(definition: &FunctionDefinition) -> Result<()> {
        if definition.name.is_empty() {
            return Err(ZoeyError::validation("Function name cannot be empty"));
        }

        if definition.name.contains(char::is_whitespace) {
            return Err(ZoeyError::validation(
                "Function name cannot contain whitespace",
            ));
        }

        if definition.description.is_empty() {
            return Err(ZoeyError::validation(
                "Function description cannot be empty",
            ));
        }

        // Validate parameters is valid JSON
        if !definition.parameters.is_object() {
            return Err(ZoeyError::validation(
                "Function parameters must be a JSON object",
            ));
        }

        Ok(())
    }

    /// Get function definition
    pub fn get_definition(&self, name: &str) -> Option<&FunctionDefinition> {
        self.functions.get(name).map(|(def, _)| def)
    }

    /// Get all function definitions
    pub fn get_all_definitions(&self) -> Vec<FunctionDefinition> {
        self.functions
            .values()
            .map(|(def, _)| def.clone())
            .collect()
    }

    /// Execute a function call
    pub async fn execute(&self, call: FunctionCall) -> FunctionResult {
        info!("Executing function: {}", call.name);
        debug!("Function arguments: {}", call.arguments);

        match self.functions.get(&call.name) {
            Some((_def, handler)) => match handler(call.arguments.clone()).await {
                Ok(result) => {
                    info!("Function {} executed successfully", call.name);
                    debug!("Result: {}", result);
                    FunctionResult {
                        name: call.name,
                        result,
                        success: true,
                        error: None,
                    }
                }
                Err(e) => {
                    warn!("Function {} failed: {}", call.name, e);
                    FunctionResult {
                        name: call.name,
                        result: serde_json::Value::Null,
                        success: false,
                        error: Some(e.to_string()),
                    }
                }
            },
            None => {
                warn!("Function '{}' not found in registry", call.name);
                FunctionResult {
                    name: call.name.clone(),
                    result: serde_json::Value::Null,
                    success: false,
                    error: Some(format!("Function '{}' not found", call.name)),
                }
            }
        }
    }

    /// Check if function exists
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get number of registered functions
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create a function definition
pub fn create_function_definition(
    name: impl Into<String>,
    description: impl Into<String>,
    parameters: serde_json::Value,
) -> FunctionDefinition {
    FunctionDefinition {
        name: name.into(),
        description: description.into(),
        parameters,
        required: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_function_registry() {
        let mut registry = FunctionRegistry::new();

        let def = FunctionDefinition {
            name: "get_weather".to_string(),
            description: "Get current weather".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                }
            }),
            required: Some(true),
        };

        let handler: FunctionHandler = Arc::new(|_args| {
            Box::pin(async move {
                Ok(serde_json::json!({
                    "temperature": 72,
                    "condition": "sunny"
                }))
            })
        });

        registry.register(def, handler);

        assert_eq!(registry.len(), 1);
        assert!(registry.has_function("get_weather"));
    }

    #[tokio::test]
    async fn test_function_execution() {
        let mut registry = FunctionRegistry::new();

        let def = create_function_definition(
            "add_numbers",
            "Add two numbers",
            serde_json::json!({"type": "object"}),
        );

        let handler: FunctionHandler = Arc::new(|args| {
            Box::pin(async move {
                let a = args["a"].as_i64().unwrap_or(0);
                let b = args["b"].as_i64().unwrap_or(0);
                Ok(serde_json::json!(a + b))
            })
        });

        registry.register(def, handler);

        let call = FunctionCall {
            name: "add_numbers".to_string(),
            arguments: serde_json::json!({"a": 5, "b": 3}),
        };

        let result = registry.execute(call).await;

        assert!(result.success);
        assert_eq!(result.result, serde_json::json!(8));
    }

    #[tokio::test]
    async fn test_function_not_found() {
        let registry = FunctionRegistry::new();

        let call = FunctionCall {
            name: "nonexistent".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = registry.execute(call).await;

        assert!(!result.success);
        assert!(result.error.is_some());
    }
}

//! Component types: Action, Provider, Evaluator

use super::{Memory, State};
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Action example for training
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionExample {
    /// Name/speaker
    pub name: String,
    /// Content text
    pub text: String,
}

// ============================================================================
// ACTION PARAMETERS - Typed parameter definitions for actions
// ============================================================================

/// Parameter type for action inputs (compatible with JSON Schema / OpenAI function calling)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterType {
    /// String value
    String,
    /// Numeric value (integer or float)
    Number,
    /// Integer value
    Integer,
    /// Boolean value
    Boolean,
    /// Array of values
    Array,
    /// Nested object
    Object,
}

impl ParameterType {
    /// Get JSON Schema type string
    pub fn as_json_schema_type(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Integer => "integer",
            Self::Boolean => "boolean",
            Self::Array => "array",
            Self::Object => "object",
        }
    }
}

/// Action parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionParameter {
    /// Parameter name (used as key in params HashMap)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Whether this parameter is required
    pub required: bool,
    /// Parameter type
    pub param_type: ParameterType,
    /// Default value if not provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// Enum values (for string parameters with fixed options)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

impl ActionParameter {
    /// Create a required string parameter
    pub fn required_string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required: true,
            param_type: ParameterType::String,
            default: None,
            enum_values: None,
        }
    }

    /// Create an optional string parameter
    pub fn optional_string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required: false,
            param_type: ParameterType::String,
            default: None,
            enum_values: None,
        }
    }

    /// Create a required number parameter
    pub fn required_number(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required: true,
            param_type: ParameterType::Number,
            default: None,
            enum_values: None,
        }
    }

    /// Create an optional number parameter with default
    pub fn optional_number_with_default(
        name: impl Into<String>,
        description: impl Into<String>,
        default: f64,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required: false,
            param_type: ParameterType::Number,
            default: Some(serde_json::json!(default)),
            enum_values: None,
        }
    }

    /// Create a string parameter with enum values
    pub fn string_enum(
        name: impl Into<String>,
        description: impl Into<String>,
        values: Vec<String>,
        required: bool,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required,
            param_type: ParameterType::String,
            default: None,
            enum_values: Some(values),
        }
    }

    /// Create a boolean parameter
    pub fn boolean(name: impl Into<String>, description: impl Into<String>, required: bool) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required,
            param_type: ParameterType::Boolean,
            default: None,
            enum_values: None,
        }
    }

    /// Set default value
    pub fn with_default(mut self, default: serde_json::Value) -> Self {
        self.default = Some(default);
        self
    }
}

/// Generate OpenAI-compatible function calling schema from action parameters
pub fn generate_function_schema(
    name: &str,
    description: &str,
    parameters: &[ActionParameter],
) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in parameters {
        let mut prop = serde_json::Map::new();
        prop.insert(
            "type".to_string(),
            serde_json::Value::String(param.param_type.as_json_schema_type().to_string()),
        );
        prop.insert(
            "description".to_string(),
            serde_json::Value::String(param.description.clone()),
        );

        if let Some(ref enum_vals) = param.enum_values {
            prop.insert(
                "enum".to_string(),
                serde_json::Value::Array(
                    enum_vals
                        .iter()
                        .map(|v| serde_json::Value::String(v.clone()))
                        .collect(),
                ),
            );
        }

        properties.insert(param.name.clone(), serde_json::Value::Object(prop));

        if param.required {
            required.push(serde_json::Value::String(param.name.clone()));
        }
    }

    serde_json::json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        }
    })
}

/// Validate parameters against their definitions
pub fn validate_parameters(
    params: &HashMap<String, serde_json::Value>,
    definitions: &[ActionParameter],
) -> Result<HashMap<String, serde_json::Value>> {
    let mut validated = HashMap::new();

    for def in definitions {
        match params.get(&def.name) {
            Some(value) => {
                // Type check
                let valid = match def.param_type {
                    ParameterType::String => value.is_string(),
                    ParameterType::Number => value.is_number(),
                    ParameterType::Integer => value.is_i64() || value.is_u64(),
                    ParameterType::Boolean => value.is_boolean(),
                    ParameterType::Array => value.is_array(),
                    ParameterType::Object => value.is_object(),
                };

                if !valid {
                    return Err(crate::ZoeyError::validation(format!(
                        "Parameter '{}' must be of type {:?}",
                        def.name, def.param_type
                    )));
                }

                // Enum check
                if let Some(ref enum_vals) = def.enum_values {
                    if let Some(s) = value.as_str() {
                        if !enum_vals.contains(&s.to_string()) {
                            return Err(crate::ZoeyError::validation(format!(
                                "Parameter '{}' must be one of: {:?}",
                                def.name, enum_vals
                            )));
                        }
                    }
                }

                validated.insert(def.name.clone(), value.clone());
            }
            None => {
                if def.required {
                    if let Some(ref default) = def.default {
                        validated.insert(def.name.clone(), default.clone());
                    } else {
                        return Err(crate::ZoeyError::validation(format!(
                            "Required parameter '{}' is missing",
                            def.name
                        )));
                    }
                } else if let Some(ref default) = def.default {
                    validated.insert(def.name.clone(), default.clone());
                }
            }
        }
    }

    Ok(validated)
}

/// Handler callback function type
pub type HandlerCallback = Arc<
    dyn Fn(
            super::Content,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Memory>>> + Send>>
        + Send
        + Sync,
>;

/// Options passed to action handlers
#[derive(Debug, Clone)]
pub struct HandlerOptions {
    /// Previous action results
    pub action_context: Option<ActionContext>,

    /// Action plan information
    pub action_plan: Option<ActionPlan>,

    /// Additional custom options
    pub custom: HashMap<String, serde_json::Value>,
}

/// Context with previous action results
#[derive(Debug, Clone)]
pub struct ActionContext {
    /// Previous results from actions in this run
    pub previous_results: Vec<ActionResult>,
}

impl ActionContext {
    /// Get a previous result by action name
    pub fn get_previous_result(&self, action_name: &str) -> Option<&ActionResult> {
        self.previous_results
            .iter()
            .find(|r| r.action_name.as_deref() == Some(action_name))
    }
}

/// Action plan for multi-step execution
#[derive(Debug, Clone)]
pub struct ActionPlan {
    /// Total number of steps
    pub total_steps: usize,
    /// Current step (1-based)
    pub current_step: usize,
    /// Steps with status tracking
    pub steps: Vec<ActionPlanStep>,
    /// AI's reasoning
    pub thought: String,
}

/// Individual step in an action plan
#[derive(Debug, Clone)]
pub struct ActionPlanStep {
    /// Action name
    pub action: String,
    /// Status
    pub status: ActionPlanStatus,
    /// Result if completed
    pub result: Option<ActionResult>,
    /// Error if failed
    pub error: Option<String>,
}

/// Status of an action plan step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionPlanStatus {
    /// Not yet started
    Pending,
    /// Currently executing
    Running,
    /// Completed successfully
    Completed,
    /// Failed
    Failed,
}

/// Result returned by an action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    /// Action name that produced this result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_name: Option<String>,

    /// Optional text description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Values to merge into state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<HashMap<String, String>>,

    /// Structured data payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, serde_json::Value>>,

    /// Whether the action succeeded
    pub success: bool,

    /// Error information if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Action trait - defines what an agent can do
#[async_trait]
pub trait Action: Send + Sync {
    /// Action name (unique identifier)
    fn name(&self) -> &str;

    /// Detailed description of what this action does
    fn description(&self) -> &str;

    /// Similar action descriptions (for matching)
    fn similes(&self) -> Vec<String> {
        vec![]
    }

    /// Example usages for training/documentation
    fn examples(&self) -> Vec<Vec<ActionExample>> {
        vec![]
    }

    // ========================================================================
    // NEW: Typed Parameters Support
    // ========================================================================

    /// Define typed parameters for this action.
    /// Returns empty vec by default (backward compatible).
    /// 
    /// # Example
    /// ```ignore
    /// fn parameters(&self) -> Vec<ActionParameter> {
    ///     vec![
    ///         ActionParameter::required_string("query", "Search query"),
    ///         ActionParameter::optional_number_with_default("limit", "Max results", 10.0),
    ///     ]
    /// }
    /// ```
    fn parameters(&self) -> Vec<ActionParameter> {
        vec![]
    }

    /// Generate OpenAI/Anthropic-compatible function calling schema.
    /// Auto-generated from name(), description(), and parameters().
    fn function_schema(&self) -> serde_json::Value {
        generate_function_schema(self.name(), self.description(), &self.parameters())
    }

    /// Execute with validated, typed parameters.
    /// Override this for structured parameter handling.
    /// Default implementation returns None (use handler() instead).
    /// 
    /// # Arguments
    /// * `params` - Validated parameters (type-checked against parameters())
    /// * `runtime` - Agent runtime reference
    /// 
    /// # Example
    /// ```ignore
    /// async fn execute(
    ///     &self,
    ///     params: HashMap<String, serde_json::Value>,
    ///     runtime: Arc<dyn Any + Send + Sync>,
    /// ) -> Result<Option<ActionResult>> {
    ///     let query = params.get("query").unwrap().as_str().unwrap();
    ///     let limit = params.get("limit").and_then(|v| v.as_f64()).unwrap_or(10.0);
    ///     // ... execute action
    ///     Ok(Some(ActionResult { ... }))
    /// }
    /// ```
    async fn execute(
        &self,
        _params: HashMap<String, serde_json::Value>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<Option<ActionResult>> {
        // Default: not implemented, use handler() instead
        Ok(None)
    }

    /// Execute with validated parameters, providing full context.
    /// This is an enhanced execute() with access to message and state.
    async fn execute_with_context(
        &self,
        params: HashMap<String, serde_json::Value>,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<Option<ActionResult>> {
        // Default: delegate to simpler execute()
        self.execute(params, runtime).await
    }

    // ========================================================================
    // Original Methods (unchanged for backward compatibility)
    // ========================================================================

    /// Validate if this action should be executed
    async fn validate(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<bool>;

    /// Execute the action (original message-based interface)
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        options: Option<HandlerOptions>,
        callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>>;
}

/// Helper trait for actions that primarily use typed parameters
/// Implement this for a simpler experience when you don't need message-based handling
#[async_trait]
pub trait ParameterizedAction: Send + Sync {
    /// Action name
    fn name(&self) -> &str;
    
    /// Action description
    fn description(&self) -> &str;
    
    /// Parameter definitions
    fn parameters(&self) -> Vec<ActionParameter>;
    
    /// Execute with validated parameters
    async fn execute(
        &self,
        params: HashMap<String, serde_json::Value>,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<ActionResult>;
}

/// Wrapper to convert ParameterizedAction into Action
pub struct ParameterizedActionWrapper<T: ParameterizedAction> {
    inner: T,
}

impl<T: ParameterizedAction> ParameterizedActionWrapper<T> {
    pub fn new(action: T) -> Self {
        Self { inner: action }
    }
}

#[async_trait]
impl<T: ParameterizedAction + 'static> Action for ParameterizedActionWrapper<T> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> Vec<ActionParameter> {
        self.inner.parameters()
    }

    async fn validate(
        &self,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
        _message: &Memory,
        _state: &State,
    ) -> Result<bool> {
        Ok(true)
    }

    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        _state: &State,
        _options: Option<HandlerOptions>,
        _callback: Option<HandlerCallback>,
    ) -> Result<Option<ActionResult>> {
        // Extract parameters from message content (expecting JSON in the text field)
        let content_text = message.content.text.clone();

        // Parse JSON parameters from content
        let raw_params: HashMap<String, serde_json::Value> = 
            serde_json::from_str(&content_text).unwrap_or_default();

        // Validate against parameter definitions
        let validated_params = validate_parameters(&raw_params, &self.inner.parameters())?;

        // Execute with validated parameters
        let result = self.inner.execute(validated_params, runtime).await?;
        Ok(Some(result))
    }
}

/// Provider result
#[derive(Debug, Clone, Default)]
pub struct ProviderResult {
    /// Human-readable text for LLM prompt
    pub text: Option<String>,

    /// Key-value pairs for template substitution
    pub values: Option<HashMap<String, String>>,

    /// Structured data for programmatic access
    pub data: Option<HashMap<String, serde_json::Value>>,
}

/// Provider trait - supplies contextual information
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Description
    fn description(&self) -> Option<String> {
        None
    }

    /// Whether the provider is dynamic (re-evaluated each time)
    fn dynamic(&self) -> bool {
        false
    }

    /// Position in provider list (affects execution order)
    fn position(&self) -> i32 {
        0
    }

    /// Whether the provider is private (not listed, must be called explicitly)
    fn private(&self) -> bool {
        false
    }

    /// Declared capabilities for routing (e.g., CHAT, VISION, FUNCTION_CALLING)
    fn capabilities(&self) -> Option<Vec<String>> {
        None
    }

    /// Get provider data
    async fn get(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<ProviderResult>;
}

/// Evaluation example
#[derive(Debug, Clone)]
pub struct EvaluationExample {
    /// Prompt/context
    pub prompt: String,
    /// Example messages
    pub messages: Vec<ActionExample>,
    /// Expected outcome
    pub outcome: String,
}

/// Evaluator trait - post-interaction processing
#[async_trait]
pub trait Evaluator: Send + Sync {
    /// Evaluator name
    fn name(&self) -> &str;

    /// Detailed description
    fn description(&self) -> &str;

    /// Similar evaluator descriptions
    fn similes(&self) -> Vec<String> {
        vec![]
    }

    /// Example evaluations
    fn examples(&self) -> Vec<EvaluationExample> {
        vec![]
    }

    /// Whether to always run (skip validation)
    fn always_run(&self) -> bool {
        false
    }

    /// Validate if this evaluator should run
    async fn validate(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
    ) -> Result<bool>;

    /// Execute the evaluator
    async fn handler(
        &self,
        runtime: Arc<dyn std::any::Any + Send + Sync>,
        message: &Memory,
        state: &State,
        did_respond: bool,
        responses: Option<Vec<Memory>>,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_result_serialization() {
        let result = ActionResult {
            action_name: Some("test_action".to_string()),
            text: Some("Success".to_string()),
            values: None,
            data: None,
            success: true,
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test_action"));
    }

    #[test]
    fn test_provider_result() {
        let result = ProviderResult {
            text: Some("Test text".to_string()),
            values: None,
            data: None,
        };

        assert_eq!(result.text, Some("Test text".to_string()));
    }
}

//! Plugin types

use super::{Action, Evaluator, Provider, Service, TestSuite};
use crate::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

/// HTTP route configuration
#[derive(Debug, Clone)]
pub struct Route {
    /// Route type
    pub route_type: RouteType,

    /// Route path
    pub path: String,

    /// File path for static routes
    pub file_path: Option<String>,

    /// Whether route is public
    pub public: bool,

    /// Name for public routes (used in UI tabs)
    pub name: Option<String>,

    /// Handler function for dynamic routes
    pub handler: Option<RouteHandler>,

    /// Whether route expects multipart/form-data
    pub is_multipart: bool,
}

/// Route type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteType {
    /// GET request
    Get,
    /// POST request
    Post,
    /// PUT request
    Put,
    /// PATCH request
    Patch,
    /// DELETE request
    Delete,
    /// Static file serving
    Static,
}

/// Route handler function type (type-erased for flexibility)
pub type RouteHandler = Arc<dyn std::any::Any + Send + Sync>;

/// Plugin trait
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name (unique identifier)
    fn name(&self) -> &str;

    /// Plugin description
    fn description(&self) -> &str;

    /// Plugin dependencies (other plugin names)
    fn dependencies(&self) -> Vec<String> {
        vec![]
    }

    /// Test dependencies (plugins needed only for testing)
    fn test_dependencies(&self) -> Vec<String> {
        vec![]
    }

    /// Priority (higher = loads later, overrides earlier plugins)
    fn priority(&self) -> i32 {
        0
    }

    /// Initialize plugin
    async fn init(
        &self,
        _config: HashMap<String, String>,
        _runtime: Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        Ok(())
    }

    /// Actions provided by this plugin
    fn actions(&self) -> Vec<Arc<dyn Action>> {
        vec![]
    }

    /// Providers provided by this plugin
    fn providers(&self) -> Vec<Arc<dyn Provider>> {
        vec![]
    }

    /// Evaluators provided by this plugin
    fn evaluators(&self) -> Vec<Arc<dyn Evaluator>> {
        vec![]
    }

    /// Services provided by this plugin
    fn services(&self) -> Vec<Arc<dyn Service>> {
        vec![]
    }

    /// Model handlers provided by this plugin
    fn models(&self) -> HashMap<String, super::ModelHandler> {
        HashMap::new()
    }

    /// Event handlers provided by this plugin
    fn events(&self) -> HashMap<String, Vec<super::EventHandler>> {
        HashMap::new()
    }

    /// HTTP routes provided by this plugin
    fn routes(&self) -> Vec<Route> {
        vec![]
    }

    /// Test suites for this plugin
    fn tests(&self) -> Vec<TestSuite> {
        vec![]
    }

    /// Database schema for this plugin
    fn schema(&self) -> Option<serde_json::Value> {
        None
    }

    /// Component types defined by this plugin
    fn component_types(&self) -> Vec<ComponentType> {
        vec![]
    }

    /// Configuration schema
    fn config_schema(&self) -> Option<serde_json::Value> {
        None
    }
}

/// Component type definition
#[derive(Clone)]
pub struct ComponentType {
    /// Component type name
    pub name: String,

    /// JSON schema for component data
    pub schema: serde_json::Value,

    /// Optional validation function
    pub validator: Option<ComponentValidator>,
}

impl std::fmt::Debug for ComponentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentType")
            .field("name", &self.name)
            .field("schema", &self.schema)
            .field("validator", &"<function>")
            .finish()
    }
}

/// Component validator function type
pub type ComponentValidator = Arc<dyn Fn(&serde_json::Value) -> bool + Send + Sync>;

/// Project agent configuration
#[derive(Clone)]
pub struct ProjectAgent {
    /// Character definition
    pub character: super::Character,

    /// Initialization function
    pub init: Option<ProjectAgentInit>,

    /// Plugins to load
    pub plugins: Vec<Arc<dyn Plugin>>,

    /// Test suites
    pub tests: Vec<TestSuite>,
}

impl std::fmt::Debug for ProjectAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectAgent")
            .field("character", &self.character)
            .field(
                "init",
                &if self.init.is_some() {
                    "<function>"
                } else {
                    "None"
                },
            )
            .field("plugins", &format!("{} plugins", self.plugins.len()))
            .field("tests", &format!("{} tests", self.tests.len()))
            .finish()
    }
}

/// Project agent init function type
pub type ProjectAgentInit = Arc<
    dyn Fn(
            Arc<dyn std::any::Any + Send + Sync>,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Project definition
#[derive(Clone)]
pub struct Project {
    /// Agents in this project
    pub agents: Vec<ProjectAgent>,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("agents", &format!("{} agents", self.agents.len()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlugin;

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            "mock-plugin"
        }

        fn description(&self) -> &str {
            "A mock plugin for testing"
        }
    }

    #[test]
    fn test_plugin_basics() {
        let plugin = MockPlugin;
        assert_eq!(plugin.name(), "mock-plugin");
        assert_eq!(plugin.description(), "A mock plugin for testing");
        assert_eq!(plugin.priority(), 0);
    }

    #[test]
    fn test_route_type() {
        let rt = RouteType::Get;
        assert_eq!(rt, RouteType::Get);
    }
}

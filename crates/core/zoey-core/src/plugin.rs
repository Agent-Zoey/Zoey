//! Plugin loading and management utilities

use crate::types::Plugin;
use crate::{ZoeyError, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Validate a plugin's structure
///
/// # Arguments
/// * `plugin` - The plugin to validate
///
/// # Returns
/// A result with validation errors if any
pub fn validate_plugin(plugin: &Arc<dyn Plugin>) -> Result<()> {
    let mut errors = Vec::new();

    // Check name
    if plugin.name().is_empty() {
        errors.push("Plugin must have a name".to_string());
    }

    // Check description
    if plugin.description().is_empty() {
        errors.push("Plugin should have a description".to_string());
    }

    if !errors.is_empty() {
        return Err(ZoeyError::Validation(format!(
            "Plugin validation failed: {}",
            errors.join(", ")
        )));
    }

    Ok(())
}

/// Resolve plugin dependencies with circular dependency detection
/// Performs topological sorting of plugins to ensure dependencies are loaded in the correct order
///
/// # Arguments
/// * `plugins` - Map of plugin names to plugin instances
/// * `is_test_mode` - Whether to include test dependencies
///
/// # Returns
/// A vector of plugins in dependency order
pub fn resolve_plugin_dependencies(
    plugins: HashMap<String, Arc<dyn Plugin>>,
    is_test_mode: bool,
) -> Result<Vec<Arc<dyn Plugin>>> {
    let mut resolution_order: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut visiting: HashSet<String> = HashSet::new();

    fn visit(
        plugin_name: &str,
        plugins: &HashMap<String, Arc<dyn Plugin>>,
        is_test_mode: bool,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        resolution_order: &mut Vec<String>,
    ) -> Result<()> {
        if !plugins.contains_key(plugin_name) {
            return Err(ZoeyError::NotFound(format!(
                "Plugin dependency '{}' not found",
                plugin_name
            )));
        }

        if visited.contains(plugin_name) {
            return Ok(());
        }

        if visiting.contains(plugin_name) {
            return Err(ZoeyError::Validation(format!(
                "Circular dependency detected involving plugin: {}",
                plugin_name
            )));
        }

        visiting.insert(plugin_name.to_string());

        if let Some(plugin) = plugins.get(plugin_name) {
            // Visit regular dependencies
            for dep in plugin.dependencies() {
                visit(
                    &dep,
                    plugins,
                    is_test_mode,
                    visited,
                    visiting,
                    resolution_order,
                )?;
            }

            // Visit test dependencies if in test mode
            if is_test_mode {
                for dep in plugin.test_dependencies() {
                    visit(
                        &dep,
                        plugins,
                        is_test_mode,
                        visited,
                        visiting,
                        resolution_order,
                    )?;
                }
            }
        }

        visiting.remove(plugin_name);
        visited.insert(plugin_name.to_string());
        resolution_order.push(plugin_name.to_string());

        Ok(())
    }

    // Visit all plugins
    for name in plugins.keys() {
        if !visited.contains(name) {
            visit(
                name,
                &plugins,
                is_test_mode,
                &mut visited,
                &mut visiting,
                &mut resolution_order,
            )?;
        }
    }

    // Return plugins in resolution order
    let final_plugins: Vec<Arc<dyn Plugin>> = resolution_order
        .iter()
        .filter_map(|name| plugins.get(name).cloned())
        .collect();

    Ok(final_plugins)
}

/// Load and initialize plugins
///
/// # Arguments
/// * `plugins` - List of plugins to load
/// * `is_test_mode` - Whether to include test dependencies
///
/// # Returns
/// A vector of plugins in dependency order
pub async fn load_plugins(
    plugins: Vec<Arc<dyn Plugin>>,
    is_test_mode: bool,
) -> Result<Vec<Arc<dyn Plugin>>> {
    // Validate all plugins
    for plugin in &plugins {
        validate_plugin(plugin)?;
    }

    // Create plugin map
    let mut plugin_map: HashMap<String, Arc<dyn Plugin>> = HashMap::new();
    for plugin in plugins {
        plugin_map.insert(plugin.name().to_string(), plugin);
    }

    // Resolve dependencies
    resolve_plugin_dependencies(plugin_map, is_test_mode)
}

/// Initialize plugins with runtime
///
/// # Arguments
/// * `plugins` - List of plugins to initialize
/// * `config` - Configuration map
/// * `runtime` - Runtime instance (type-erased)
///
/// # Returns
/// Result indicating success or failure
pub async fn initialize_plugins(
    plugins: &[Arc<dyn Plugin>],
    config: HashMap<String, String>,
    runtime: Arc<dyn std::any::Any + Send + Sync>,
) -> Result<()> {
    for plugin in plugins {
        plugin.init(config.clone(), runtime.clone()).await?;
    }

    Ok(())
}

/// Get all actions from plugins
///
/// # Arguments
/// * `plugins` - List of plugins
///
/// # Returns
/// A vector of all actions from all plugins
pub fn get_plugin_actions(plugins: &[Arc<dyn Plugin>]) -> Vec<Arc<dyn crate::types::Action>> {
    plugins.iter().flat_map(|plugin| plugin.actions()).collect()
}

/// Get all providers from plugins
///
/// # Arguments
/// * `plugins` - List of plugins
///
/// # Returns
/// A vector of all providers from all plugins
pub fn get_plugin_providers(plugins: &[Arc<dyn Plugin>]) -> Vec<Arc<dyn crate::types::Provider>> {
    plugins
        .iter()
        .flat_map(|plugin| plugin.providers())
        .collect()
}

/// Get all evaluators from plugins
///
/// # Arguments
/// * `plugins` - List of plugins
///
/// # Returns
/// A vector of all evaluators from all plugins
pub fn get_plugin_evaluators(plugins: &[Arc<dyn Plugin>]) -> Vec<Arc<dyn crate::types::Evaluator>> {
    plugins
        .iter()
        .flat_map(|plugin| plugin.evaluators())
        .collect()
}

/// Get all services from plugins
///
/// # Arguments
/// * `plugins` - List of plugins
///
/// # Returns
/// A vector of all services from all plugins
pub fn get_plugin_services(plugins: &[Arc<dyn Plugin>]) -> Vec<Arc<dyn crate::types::Service>> {
    plugins
        .iter()
        .flat_map(|plugin| plugin.services())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockPlugin {
        name: String,
        dependencies: Vec<String>,
    }

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Mock plugin"
        }

        fn dependencies(&self) -> Vec<String> {
            self.dependencies.clone()
        }
    }

    #[tokio::test]
    async fn test_validate_plugin() {
        let plugin: Arc<dyn Plugin> = Arc::new(MockPlugin {
            name: "test-plugin".to_string(),
            dependencies: vec![],
        });

        assert!(validate_plugin(&plugin).is_ok());
    }

    #[tokio::test]
    async fn test_empty_name_validation() {
        let plugin: Arc<dyn Plugin> = Arc::new(MockPlugin {
            name: "".to_string(),
            dependencies: vec![],
        });

        assert!(validate_plugin(&plugin).is_err());
    }

    #[test]
    fn test_resolve_dependencies() {
        let plugin_a: Arc<dyn Plugin> = Arc::new(MockPlugin {
            name: "plugin-a".to_string(),
            dependencies: vec![],
        });

        let plugin_b: Arc<dyn Plugin> = Arc::new(MockPlugin {
            name: "plugin-b".to_string(),
            dependencies: vec!["plugin-a".to_string()],
        });

        let mut plugins = HashMap::new();
        plugins.insert("plugin-a".to_string(), plugin_a);
        plugins.insert("plugin-b".to_string(), plugin_b);

        let result = resolve_plugin_dependencies(plugins, false);
        assert!(result.is_ok());

        let resolved = result.unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].name(), "plugin-a");
        assert_eq!(resolved[1].name(), "plugin-b");
    }

    #[test]
    fn test_circular_dependency_detection() {
        let plugin_a: Arc<dyn Plugin> = Arc::new(MockPlugin {
            name: "plugin-a".to_string(),
            dependencies: vec!["plugin-b".to_string()],
        });

        let plugin_b: Arc<dyn Plugin> = Arc::new(MockPlugin {
            name: "plugin-b".to_string(),
            dependencies: vec!["plugin-a".to_string()],
        });

        let mut plugins = HashMap::new();
        plugins.insert("plugin-a".to_string(), plugin_a);
        plugins.insert("plugin-b".to_string(), plugin_b);

        let result = resolve_plugin_dependencies(plugins, false);
        assert!(result.is_err());
    }
}

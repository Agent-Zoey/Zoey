//! Service types for stateful components

use crate::Result;
use async_trait::async_trait;
use std::any::Any;
use uuid::Uuid;

/// Service name type
pub type ServiceTypeName = String;

/// Service trait for stateful, long-running components
#[async_trait]
pub trait Service: Send + Sync + Any {
    /// Service type name (unique identifier)
    fn service_type(&self) -> &str;

    /// Initialize the service
    async fn initialize(
        &mut self,
        _runtime: std::sync::Arc<dyn std::any::Any + Send + Sync>,
    ) -> Result<()> {
        Ok(())
    }

    /// Start the service (begin background tasks)
    async fn start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Stop the service (cleanup)
    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    /// Check if service is running
    fn is_running(&self) -> bool {
        false
    }

    /// Get service health status
    async fn health_check(&self) -> Result<ServiceHealth> {
        Ok(ServiceHealth::Healthy)
    }

    /// Downcast support
    fn as_any(&self) -> &dyn Any
    where
        Self: Sized,
    {
        self
    }

    /// Optional agent query hook for coordination services
    fn query_agents(&self, _capability: &str) -> Option<Vec<(Uuid, f32)>> {
        None
    }
}

/// Service health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ServiceHealth {
    /// Service is healthy and operational
    Healthy,
    /// Service is degraded but functional
    Degraded,
    /// Service is unhealthy/not functional
    Unhealthy,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockService;

    #[async_trait]
    impl Service for MockService {
        fn service_type(&self) -> &str {
            "mock-service"
        }
    }

    #[tokio::test]
    async fn test_service_health() {
        let service = MockService;
        let health = service.health_check().await.unwrap();
        assert_eq!(health, ServiceHealth::Healthy);
    }
}

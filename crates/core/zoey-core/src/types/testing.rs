//! Testing types and framework

use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Test case trait
#[async_trait]
pub trait TestCase: Send + Sync {
    /// Test name
    fn name(&self) -> &str;

    /// Test description
    fn description(&self) -> &str;

    /// Run the test
    async fn run(&self, runtime: Arc<dyn std::any::Any + Send + Sync>) -> Result<()>;
}

/// Test suite
#[derive(Clone)]
pub struct TestSuite {
    /// Suite name
    pub name: String,

    /// Test cases
    pub tests: Vec<Arc<dyn TestCase>>,
}

impl std::fmt::Debug for TestSuite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestSuite")
            .field("name", &self.name)
            .field("tests", &format!("{} tests", self.tests.len()))
            .finish()
    }
}

impl TestSuite {
    /// Create a new test suite
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tests: Vec::new(),
        }
    }

    /// Add a test case
    pub fn add_test(&mut self, test: Arc<dyn TestCase>) {
        self.tests.push(test);
    }
}

/// Test results
#[derive(Debug, Clone, Default)]
pub struct TestResults {
    /// Passed tests
    pub passed: Vec<String>,

    /// Failed tests with error messages
    pub failed: Vec<(String, String)>,

    /// Skipped tests
    pub skipped: Vec<String>,
}

impl TestResults {
    /// Create new empty results
    pub fn new() -> Self {
        Self::default()
    }

    /// Total number of tests
    pub fn total(&self) -> usize {
        self.passed.len() + self.failed.len() + self.skipped.len()
    }

    /// Whether all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed.is_empty()
    }

    /// Success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total() == 0 {
            return 0.0;
        }
        (self.passed.len() as f64 / self.total() as f64) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_results() {
        let mut results = TestResults::new();
        results.passed.push("test1".to_string());
        results
            .failed
            .push(("test2".to_string(), "error".to_string()));

        assert_eq!(results.total(), 2);
        assert!(!results.all_passed());
        assert_eq!(results.success_rate(), 50.0);
    }

    #[test]
    fn test_test_suite() {
        let suite = TestSuite::new("test_suite");
        assert_eq!(suite.name, "test_suite");
        assert_eq!(suite.tests.len(), 0);
    }
}

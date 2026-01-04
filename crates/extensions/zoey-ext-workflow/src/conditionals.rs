/*!
# Conditional Branching

Provides conditional logic for workflow execution including:
- If/else branching
- Switch/case statements
- Loops (for, while, until)
- Parallel branches
- Error handling branches
*/

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Condition expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Simple boolean value
    Boolean(bool),
    /// Compare two values
    Compare {
        left: Expression,
        operator: CompareOp,
        right: Expression,
    },
    /// Logical AND
    And(Vec<Condition>),
    /// Logical OR
    Or(Vec<Condition>),
    /// Logical NOT
    Not(Box<Condition>),
    /// Check if value exists
    Exists(String),
    /// Check if value is null/empty
    IsEmpty(String),
    /// Check if value matches pattern
    Matches { value: String, pattern: String },
    /// Check if value is in list
    In { value: Expression, list: Vec<Value> },
    /// Custom expression (evaluated at runtime)
    Expression(String),
}

impl Condition {
    /// Create a simple boolean condition
    pub fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    /// Create equality comparison
    pub fn equals(left: Expression, right: Expression) -> Self {
        Self::Compare {
            left,
            operator: CompareOp::Eq,
            right,
        }
    }

    /// Create greater than comparison
    pub fn greater_than(left: Expression, right: Expression) -> Self {
        Self::Compare {
            left,
            operator: CompareOp::Gt,
            right,
        }
    }

    /// Create less than comparison
    pub fn less_than(left: Expression, right: Expression) -> Self {
        Self::Compare {
            left,
            operator: CompareOp::Lt,
            right,
        }
    }

    /// Combine with AND
    pub fn and(conditions: Vec<Condition>) -> Self {
        Self::And(conditions)
    }

    /// Combine with OR
    pub fn or(conditions: Vec<Condition>) -> Self {
        Self::Or(conditions)
    }

    /// Negate condition
    pub fn not(condition: Condition) -> Self {
        Self::Not(Box::new(condition))
    }

    /// Evaluate condition against context
    pub fn evaluate(&self, context: &EvalContext) -> Result<bool, ConditionError> {
        match self {
            Self::Boolean(b) => Ok(*b),
            Self::Compare {
                left,
                operator,
                right,
            } => {
                let left_val = left.evaluate(context)?;
                let right_val = right.evaluate(context)?;
                Ok(operator.compare(&left_val, &right_val))
            }
            Self::And(conditions) => {
                for cond in conditions {
                    if !cond.evaluate(context)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Or(conditions) => {
                for cond in conditions {
                    if cond.evaluate(context)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Not(cond) => Ok(!cond.evaluate(context)?),
            Self::Exists(key) => Ok(context.get(key).is_some()),
            Self::IsEmpty(key) => match context.get(key) {
                None => Ok(true),
                Some(Value::Null) => Ok(true),
                Some(Value::String(s)) => Ok(s.is_empty()),
                Some(Value::Array(a)) => Ok(a.is_empty()),
                Some(Value::Object(o)) => Ok(o.is_empty()),
                _ => Ok(false),
            },
            Self::Matches { value, pattern } => {
                let val = context
                    .get(value)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ConditionError::InvalidValue(value.clone()))?;
                let re = regex::Regex::new(pattern)
                    .map_err(|e| ConditionError::InvalidPattern(e.to_string()))?;
                Ok(re.is_match(val))
            }
            Self::In { value, list } => {
                let val = value.evaluate(context)?;
                Ok(list.contains(&val))
            }
            Self::Expression(expr) => {
                // Simple expression evaluation
                // In production, would use a proper expression parser
                if let Some(val) = context.get(expr) {
                    match val {
                        Value::Bool(b) => Ok(*b),
                        Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) != 0.0),
                        Value::String(s) => Ok(!s.is_empty()),
                        _ => Ok(true),
                    }
                } else {
                    Ok(false)
                }
            }
        }
    }
}

/// Comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    /// Equal
    Eq,
    /// Not equal
    Ne,
    /// Greater than
    Gt,
    /// Greater than or equal
    Ge,
    /// Less than
    Lt,
    /// Less than or equal
    Le,
    /// Contains (for strings/arrays)
    Contains,
    /// Starts with (for strings)
    StartsWith,
    /// Ends with (for strings)
    EndsWith,
}

impl CompareOp {
    /// Compare two values
    pub fn compare(&self, left: &Value, right: &Value) -> bool {
        match self {
            Self::Eq => left == right,
            Self::Ne => left != right,
            Self::Gt => Self::numeric_cmp(left, right, |a, b| a > b),
            Self::Ge => Self::numeric_cmp(left, right, |a, b| a >= b),
            Self::Lt => Self::numeric_cmp(left, right, |a, b| a < b),
            Self::Le => Self::numeric_cmp(left, right, |a, b| a <= b),
            Self::Contains => Self::contains(left, right),
            Self::StartsWith => Self::string_op(left, right, |s, p| s.starts_with(p)),
            Self::EndsWith => Self::string_op(left, right, |s, p| s.ends_with(p)),
        }
    }

    fn numeric_cmp<F>(left: &Value, right: &Value, cmp: F) -> bool
    where
        F: Fn(f64, f64) -> bool,
    {
        match (left.as_f64(), right.as_f64()) {
            (Some(l), Some(r)) => cmp(l, r),
            _ => false,
        }
    }

    fn contains(haystack: &Value, needle: &Value) -> bool {
        match haystack {
            Value::String(s) => needle.as_str().map(|n| s.contains(n)).unwrap_or(false),
            Value::Array(arr) => arr.contains(needle),
            _ => false,
        }
    }

    fn string_op<F>(left: &Value, right: &Value, op: F) -> bool
    where
        F: Fn(&str, &str) -> bool,
    {
        match (left.as_str(), right.as_str()) {
            (Some(l), Some(r)) => op(l, r),
            _ => false,
        }
    }
}

/// Expression for evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    /// Literal value
    Literal(Value),
    /// Variable reference
    Variable(String),
    /// Path expression (e.g., "data.items[0].name")
    Path(String),
    /// Function call
    Function { name: String, args: Vec<Expression> },
    /// Arithmetic operation
    Arithmetic {
        left: Box<Expression>,
        op: ArithmeticOp,
        right: Box<Expression>,
    },
}

impl Expression {
    /// Create a literal expression
    pub fn literal(value: impl Into<Value>) -> Self {
        Self::Literal(value.into())
    }

    /// Create a variable reference
    pub fn var(name: impl Into<String>) -> Self {
        Self::Variable(name.into())
    }

    /// Create a path expression
    pub fn path(path: impl Into<String>) -> Self {
        Self::Path(path.into())
    }

    /// Evaluate expression
    pub fn evaluate(&self, context: &EvalContext) -> Result<Value, ConditionError> {
        match self {
            Self::Literal(v) => Ok(v.clone()),
            Self::Variable(name) => context
                .get(name)
                .cloned()
                .ok_or_else(|| ConditionError::VariableNotFound(name.clone())),
            Self::Path(path) => context
                .get_path(path)
                .ok_or_else(|| ConditionError::PathNotFound(path.clone())),
            Self::Function { name, args } => {
                let evaluated_args: Result<Vec<Value>, _> =
                    args.iter().map(|a| a.evaluate(context)).collect();
                context.call_function(name, &evaluated_args?)
            }
            Self::Arithmetic { left, op, right } => {
                let l = left.evaluate(context)?;
                let r = right.evaluate(context)?;
                op.apply(&l, &r)
            }
        }
    }
}

/// Arithmetic operators
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

impl ArithmeticOp {
    /// Apply operation to two values
    pub fn apply(&self, left: &Value, right: &Value) -> Result<Value, ConditionError> {
        let l = left.as_f64().ok_or(ConditionError::NotNumeric)?;
        let r = right.as_f64().ok_or(ConditionError::NotNumeric)?;

        let result = match self {
            Self::Add => l + r,
            Self::Sub => l - r,
            Self::Mul => l * r,
            Self::Div => {
                if r == 0.0 {
                    return Err(ConditionError::DivisionByZero);
                }
                l / r
            }
            Self::Mod => l % r,
        };

        Ok(Value::Number(
            serde_json::Number::from_f64(result).ok_or(ConditionError::NotNumeric)?,
        ))
    }
}

/// Evaluation context
#[derive(Debug, Clone, Default)]
pub struct EvalContext {
    variables: HashMap<String, Value>,
}

impl EvalContext {
    /// Create new context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Get a variable
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.variables.get(key)
    }

    /// Get value by path (e.g., "data.items[0].name")
    pub fn get_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let mut current = self.get(parts[0])?.clone();
        for part in &parts[1..] {
            // Handle array indexing
            if let Some(bracket_pos) = part.find('[') {
                let field = &part[..bracket_pos];
                let idx_str = &part[bracket_pos + 1..part.len() - 1];
                let idx: usize = idx_str.parse().ok()?;

                if !field.is_empty() {
                    current = current.get(field)?.clone();
                }
                current = current.get(idx)?.clone();
            } else {
                current = current.get(part)?.clone();
            }
        }

        Some(current)
    }

    /// Call a built-in function
    pub fn call_function(&self, name: &str, args: &[Value]) -> Result<Value, ConditionError> {
        match name {
            "len" => {
                if let Some(arg) = args.first() {
                    match arg {
                        Value::String(s) => Ok(Value::Number(s.len().into())),
                        Value::Array(a) => Ok(Value::Number(a.len().into())),
                        Value::Object(o) => Ok(Value::Number(o.len().into())),
                        _ => Err(ConditionError::InvalidArgument),
                    }
                } else {
                    Err(ConditionError::MissingArgument)
                }
            }
            "lower" => args
                .first()
                .and_then(|v| v.as_str())
                .map(|s| Value::String(s.to_lowercase()))
                .ok_or(ConditionError::InvalidArgument),
            "upper" => args
                .first()
                .and_then(|v| v.as_str())
                .map(|s| Value::String(s.to_uppercase()))
                .ok_or(ConditionError::InvalidArgument),
            "abs" => args
                .first()
                .and_then(|v| v.as_f64())
                .and_then(|n| serde_json::Number::from_f64(n.abs()))
                .map(Value::Number)
                .ok_or(ConditionError::InvalidArgument),
            "min" => {
                let nums: Vec<f64> = args.iter().filter_map(|v| v.as_f64()).collect();
                nums.iter()
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .and_then(|n| serde_json::Number::from_f64(*n))
                    .map(Value::Number)
                    .ok_or(ConditionError::InvalidArgument)
            }
            "max" => {
                let nums: Vec<f64> = args.iter().filter_map(|v| v.as_f64()).collect();
                nums.iter()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .and_then(|n| serde_json::Number::from_f64(*n))
                    .map(Value::Number)
                    .ok_or(ConditionError::InvalidArgument)
            }
            _ => Err(ConditionError::UnknownFunction(name.to_string())),
        }
    }

    /// Create context from JSON value
    pub fn from_json(value: Value) -> Self {
        let mut ctx = Self::new();
        if let Value::Object(map) = value {
            for (k, v) in map {
                ctx.set(k, v);
            }
        }
        ctx
    }
}

/// If-then-else branch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfBranch {
    /// Branch name
    pub name: String,
    /// Condition to evaluate
    pub condition: Condition,
    /// Tasks to execute if true
    pub then_tasks: Vec<String>,
    /// Else-if branches
    pub else_if: Vec<(Condition, Vec<String>)>,
    /// Tasks to execute if all conditions false
    pub else_tasks: Vec<String>,
}

impl IfBranch {
    /// Create a new if branch
    pub fn new(name: impl Into<String>, condition: Condition) -> Self {
        Self {
            name: name.into(),
            condition,
            then_tasks: Vec::new(),
            else_if: Vec::new(),
            else_tasks: Vec::new(),
        }
    }

    /// Set then tasks
    pub fn then(mut self, tasks: Vec<String>) -> Self {
        self.then_tasks = tasks;
        self
    }

    /// Add else-if branch
    pub fn else_if(mut self, condition: Condition, tasks: Vec<String>) -> Self {
        self.else_if.push((condition, tasks));
        self
    }

    /// Set else tasks
    pub fn else_branch(mut self, tasks: Vec<String>) -> Self {
        self.else_tasks = tasks;
        self
    }

    /// Evaluate and get tasks to execute
    pub fn evaluate(&self, context: &EvalContext) -> Result<Vec<String>, ConditionError> {
        if self.condition.evaluate(context)? {
            return Ok(self.then_tasks.clone());
        }

        for (cond, tasks) in &self.else_if {
            if cond.evaluate(context)? {
                return Ok(tasks.clone());
            }
        }

        Ok(self.else_tasks.clone())
    }
}

/// Switch/case branch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchBranch {
    /// Branch name
    pub name: String,
    /// Expression to evaluate
    pub expression: Expression,
    /// Cases (value -> tasks)
    pub cases: Vec<(Value, Vec<String>)>,
    /// Default tasks
    pub default: Vec<String>,
}

impl SwitchBranch {
    /// Create a new switch branch
    pub fn new(name: impl Into<String>, expression: Expression) -> Self {
        Self {
            name: name.into(),
            expression,
            cases: Vec::new(),
            default: Vec::new(),
        }
    }

    /// Add a case
    pub fn case(mut self, value: Value, tasks: Vec<String>) -> Self {
        self.cases.push((value, tasks));
        self
    }

    /// Set default tasks
    pub fn default_case(mut self, tasks: Vec<String>) -> Self {
        self.default = tasks;
        self
    }

    /// Evaluate and get tasks to execute
    pub fn evaluate(&self, context: &EvalContext) -> Result<Vec<String>, ConditionError> {
        let value = self.expression.evaluate(context)?;

        for (case_val, tasks) in &self.cases {
            if &value == case_val {
                return Ok(tasks.clone());
            }
        }

        Ok(self.default.clone())
    }
}

/// Loop configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoopConfig {
    /// For loop with counter
    For {
        variable: String,
        start: i64,
        end: i64,
        step: i64,
    },
    /// For-each loop over collection
    ForEach {
        variable: String,
        collection: String,
    },
    /// While loop
    While {
        condition: Condition,
        max_iterations: usize,
    },
    /// Until loop (opposite of while)
    Until {
        condition: Condition,
        max_iterations: usize,
    },
}

/// Loop branch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopBranch {
    /// Loop name
    pub name: String,
    /// Loop configuration
    pub config: LoopConfig,
    /// Tasks to execute in loop
    pub body: Vec<String>,
    /// Enable parallel execution of iterations
    pub parallel: bool,
    /// Break condition (optional)
    pub break_condition: Option<Condition>,
    /// Continue condition (optional)
    pub continue_condition: Option<Condition>,
}

impl LoopBranch {
    /// Create a for loop
    pub fn for_loop(name: impl Into<String>, var: impl Into<String>, start: i64, end: i64) -> Self {
        Self {
            name: name.into(),
            config: LoopConfig::For {
                variable: var.into(),
                start,
                end,
                step: 1,
            },
            body: Vec::new(),
            parallel: false,
            break_condition: None,
            continue_condition: None,
        }
    }

    /// Create a for-each loop
    pub fn for_each(
        name: impl Into<String>,
        var: impl Into<String>,
        collection: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            config: LoopConfig::ForEach {
                variable: var.into(),
                collection: collection.into(),
            },
            body: Vec::new(),
            parallel: false,
            break_condition: None,
            continue_condition: None,
        }
    }

    /// Create a while loop
    pub fn while_loop(name: impl Into<String>, condition: Condition) -> Self {
        Self {
            name: name.into(),
            config: LoopConfig::While {
                condition,
                max_iterations: 1000,
            },
            body: Vec::new(),
            parallel: false,
            break_condition: None,
            continue_condition: None,
        }
    }

    /// Set loop body
    pub fn body(mut self, tasks: Vec<String>) -> Self {
        self.body = tasks;
        self
    }

    /// Enable parallel execution
    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    /// Set break condition
    pub fn break_on(mut self, condition: Condition) -> Self {
        self.break_condition = Some(condition);
        self
    }

    /// Generate iterations
    pub fn iterations(&self, context: &EvalContext) -> Result<Vec<LoopIteration>, ConditionError> {
        match &self.config {
            LoopConfig::For {
                variable,
                start,
                end,
                step,
            } => {
                let mut iterations = Vec::new();
                let mut i = *start;
                let mut idx = 0;
                while (step > &0 && i < *end) || (step < &0 && i > *end) {
                    let mut vars = HashMap::new();
                    vars.insert(variable.clone(), Value::Number(i.into()));
                    iterations.push(LoopIteration {
                        index: idx,
                        variables: vars,
                    });
                    i += step;
                    idx += 1;
                }
                Ok(iterations)
            }
            LoopConfig::ForEach {
                variable,
                collection,
            } => {
                let coll = context
                    .get(collection)
                    .ok_or_else(|| ConditionError::VariableNotFound(collection.clone()))?;

                match coll {
                    Value::Array(arr) => Ok(arr
                        .iter()
                        .enumerate()
                        .map(|(idx, val)| {
                            let mut vars = HashMap::new();
                            vars.insert(variable.clone(), val.clone());
                            vars.insert(format!("{}_index", variable), Value::Number(idx.into()));
                            LoopIteration {
                                index: idx,
                                variables: vars,
                            }
                        })
                        .collect()),
                    _ => Err(ConditionError::InvalidValue(collection.clone())),
                }
            }
            LoopConfig::While { max_iterations, .. } | LoopConfig::Until { max_iterations, .. } => {
                // For while/until loops, we generate one iteration at a time
                Ok((0..*max_iterations)
                    .map(|i| LoopIteration {
                        index: i,
                        variables: HashMap::new(),
                    })
                    .collect())
            }
        }
    }
}

/// Single loop iteration context
#[derive(Debug, Clone)]
pub struct LoopIteration {
    /// Iteration index
    pub index: usize,
    /// Variables for this iteration
    pub variables: HashMap<String, Value>,
}

/// Parallel branch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelBranch {
    /// Branch name
    pub name: String,
    /// Parallel branches to execute
    pub branches: Vec<Vec<String>>,
    /// Wait for all or any
    pub wait_mode: WaitMode,
    /// Failure mode
    pub failure_mode: FailureMode,
}

/// Wait mode for parallel branches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitMode {
    /// Wait for all branches to complete
    All,
    /// Wait for first branch to complete
    Any,
    /// Wait for N branches to complete
    N(usize),
}

/// Failure mode for parallel branches
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureMode {
    /// Fail if any branch fails
    FailFast,
    /// Continue on failure
    Continue,
    /// Fail only if all branches fail
    FailAll,
}

impl ParallelBranch {
    /// Create new parallel branch
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            branches: Vec::new(),
            wait_mode: WaitMode::All,
            failure_mode: FailureMode::FailFast,
        }
    }

    /// Add a branch
    pub fn add_branch(mut self, tasks: Vec<String>) -> Self {
        self.branches.push(tasks);
        self
    }

    /// Set wait mode
    pub fn wait(mut self, mode: WaitMode) -> Self {
        self.wait_mode = mode;
        self
    }

    /// Set failure mode
    pub fn on_failure(mut self, mode: FailureMode) -> Self {
        self.failure_mode = mode;
        self
    }
}

/// Condition errors
#[derive(Debug, thiserror::Error)]
pub enum ConditionError {
    #[error("Variable not found: {0}")]
    VariableNotFound(String),
    #[error("Path not found: {0}")]
    PathNotFound(String),
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("Unknown function: {0}")]
    UnknownFunction(String),
    #[error("Invalid argument")]
    InvalidArgument,
    #[error("Missing argument")]
    MissingArgument,
    #[error("Value is not numeric")]
    NotNumeric,
    #[error("Division by zero")]
    DivisionByZero,
    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_condition() {
        let ctx = EvalContext::new();
        let cond = Condition::Boolean(true);
        assert!(cond.evaluate(&ctx).unwrap());
    }

    #[test]
    fn test_comparison() {
        let mut ctx = EvalContext::new();
        ctx.set("x", 10);

        let cond = Condition::Compare {
            left: Expression::var("x"),
            operator: CompareOp::Gt,
            right: Expression::literal(5),
        };

        assert!(cond.evaluate(&ctx).unwrap());
    }

    #[test]
    fn test_and_or() {
        let ctx = EvalContext::new();

        let cond = Condition::and(vec![Condition::Boolean(true), Condition::Boolean(true)]);
        assert!(cond.evaluate(&ctx).unwrap());

        let cond = Condition::and(vec![Condition::Boolean(true), Condition::Boolean(false)]);
        assert!(!cond.evaluate(&ctx).unwrap());

        let cond = Condition::or(vec![Condition::Boolean(false), Condition::Boolean(true)]);
        assert!(cond.evaluate(&ctx).unwrap());
    }

    #[test]
    fn test_exists() {
        let mut ctx = EvalContext::new();
        ctx.set("exists", "value");

        assert!(Condition::Exists("exists".to_string())
            .evaluate(&ctx)
            .unwrap());
        assert!(!Condition::Exists("missing".to_string())
            .evaluate(&ctx)
            .unwrap());
    }

    #[test]
    fn test_if_branch() {
        let mut ctx = EvalContext::new();
        ctx.set("status", "success");

        let branch = IfBranch::new(
            "check_status",
            Condition::Compare {
                left: Expression::var("status"),
                operator: CompareOp::Eq,
                right: Expression::literal("success"),
            },
        )
        .then(vec!["handle_success".to_string()])
        .else_branch(vec!["handle_failure".to_string()]);

        let tasks = branch.evaluate(&ctx).unwrap();
        assert_eq!(tasks, vec!["handle_success"]);

        ctx.set("status", "error");
        let tasks = branch.evaluate(&ctx).unwrap();
        assert_eq!(tasks, vec!["handle_failure"]);
    }

    #[test]
    fn test_switch_branch() {
        let mut ctx = EvalContext::new();
        ctx.set("color", "red");

        let branch = SwitchBranch::new("color_switch", Expression::var("color"))
            .case(
                Value::String("red".to_string()),
                vec!["handle_red".to_string()],
            )
            .case(
                Value::String("blue".to_string()),
                vec!["handle_blue".to_string()],
            )
            .default_case(vec!["handle_default".to_string()]);

        let tasks = branch.evaluate(&ctx).unwrap();
        assert_eq!(tasks, vec!["handle_red"]);

        ctx.set("color", "green");
        let tasks = branch.evaluate(&ctx).unwrap();
        assert_eq!(tasks, vec!["handle_default"]);
    }

    #[test]
    fn test_for_loop_iterations() {
        let ctx = EvalContext::new();
        let loop_branch = LoopBranch::for_loop("counter", "i", 0, 5);

        let iterations = loop_branch.iterations(&ctx).unwrap();
        assert_eq!(iterations.len(), 5);
        assert_eq!(
            iterations[0].variables.get("i"),
            Some(&Value::Number(0.into()))
        );
        assert_eq!(
            iterations[4].variables.get("i"),
            Some(&Value::Number(4.into()))
        );
    }

    #[test]
    fn test_foreach_loop() {
        let mut ctx = EvalContext::new();
        ctx.set("items", serde_json::json!(["a", "b", "c"]));

        let loop_branch = LoopBranch::for_each("process", "item", "items");
        let iterations = loop_branch.iterations(&ctx).unwrap();

        assert_eq!(iterations.len(), 3);
    }

    #[test]
    fn test_expression_arithmetic() {
        let mut ctx = EvalContext::new();
        ctx.set("a", 10);
        ctx.set("b", 3);

        let expr = Expression::Arithmetic {
            left: Box::new(Expression::var("a")),
            op: ArithmeticOp::Add,
            right: Box::new(Expression::var("b")),
        };

        let result = expr.evaluate(&ctx).unwrap();
        assert_eq!(result.as_f64(), Some(13.0));
    }

    #[test]
    fn test_path_expression() {
        let mut ctx = EvalContext::new();
        ctx.set(
            "data",
            serde_json::json!({
                "items": [
                    {"name": "first"},
                    {"name": "second"}
                ]
            }),
        );

        let result = ctx.get_path("data.items[0].name");
        assert_eq!(result, Some(Value::String("first".to_string())));
    }
}

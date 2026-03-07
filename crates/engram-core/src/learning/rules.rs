/// Rule definition and parser — humans create rules, engram executes them.
///
/// Rules are pattern-matching triggers:
///   WHEN <pattern> THEN <action>
///
/// Engram never invents rules. All rules are explicitly authored by humans.
/// Rules operate on the graph structure and produce derived edges/properties.
///
/// Rule format (YAML-like, parsed from strings):
///   name: transitive_type
///   when:
///     - edge(A, "is_a", B)
///     - edge(B, "is_a", C)
///   then:
///     - edge(A, "is_a", C, confidence = min(e1.confidence, e2.confidence))

use std::fmt;

/// A rule definition.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Human-readable name.
    pub name: String,
    /// Pattern conditions (all must match).
    pub conditions: Vec<Condition>,
    /// Actions to take when conditions match.
    pub actions: Vec<Action>,
}

/// A pattern condition in a rule.
#[derive(Debug, Clone)]
pub enum Condition {
    /// Match an edge: edge(from_var, relationship, to_var)
    Edge {
        from_var: String,
        relationship: String,
        to_var: String,
    },
    /// Match a node property: prop(node_var, key, value)
    Property {
        node_var: String,
        key: String,
        value: String,
    },
    /// Confidence threshold: confidence(var, ">", 0.5)
    Confidence {
        var: String,
        op: ConditionOp,
        threshold: f32,
    },
}

/// Comparison operators for conditions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConditionOp {
    Gt,
    Gte,
    Lt,
    Lte,
}

/// An action to take when a rule fires.
#[derive(Debug, Clone)]
pub enum Action {
    /// Create an edge: edge(from_var, relationship, to_var)
    CreateEdge {
        from_var: String,
        relationship: String,
        to_var: String,
        confidence_expr: ConfidenceExpr,
    },
    /// Set a property: prop(node_var, key, value_expr)
    SetProperty {
        node_var: String,
        key: String,
        value: String,
    },
    /// Flag for review.
    Flag {
        node_var: String,
        reason: String,
    },
}

/// Expression for computing derived confidence.
#[derive(Debug, Clone)]
pub enum ConfidenceExpr {
    /// Fixed value.
    Literal(f32),
    /// Minimum of two edge confidences.
    Min(String, String),
    /// Product of two edge confidences.
    Product(String, String),
}

/// Result of rule execution.
#[derive(Debug)]
pub struct RuleResult {
    pub rule_name: String,
    pub fired: bool,
    pub edges_created: u32,
    pub properties_set: u32,
    pub flags_raised: u32,
}

/// Parse a simple rule definition string.
///
/// Format:
///   rule <name>
///   when edge(<from>, "<rel>", <to>)
///   when edge(<from>, "<rel>", <to>)
///   then edge(<from>, "<rel>", <to>, confidence=min(<e1>,<e2>))
///
/// This is intentionally minimal — not a full YAML parser.
pub fn parse_rule(input: &str) -> Result<Rule, RuleParseError> {
    let mut name = String::new();
    let mut conditions = Vec::new();
    let mut actions = Vec::new();

    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(n) = line.strip_prefix("rule ") {
            name = n.trim().to_string();
        } else if let Some(cond) = line.strip_prefix("when ") {
            conditions.push(parse_condition(cond.trim())?);
        } else if let Some(act) = line.strip_prefix("then ") {
            actions.push(parse_action(act.trim())?);
        }
    }

    if name.is_empty() {
        return Err(RuleParseError("missing rule name".into()));
    }
    if conditions.is_empty() {
        return Err(RuleParseError("no conditions defined".into()));
    }
    if actions.is_empty() {
        return Err(RuleParseError("no actions defined".into()));
    }

    Ok(Rule {
        name,
        conditions,
        actions,
    })
}

fn parse_condition(s: &str) -> Result<Condition, RuleParseError> {
    if let Some(inner) = strip_func(s, "edge") {
        let parts = split_args(inner);
        if parts.len() != 3 {
            return Err(RuleParseError(format!("edge condition needs 3 args, got {}", parts.len())));
        }
        return Ok(Condition::Edge {
            from_var: parts[0].clone(),
            relationship: unquote(&parts[1]),
            to_var: parts[2].clone(),
        });
    }

    if let Some(inner) = strip_func(s, "prop") {
        let parts = split_args(inner);
        if parts.len() != 3 {
            return Err(RuleParseError(format!("prop condition needs 3 args, got {}", parts.len())));
        }
        return Ok(Condition::Property {
            node_var: parts[0].clone(),
            key: unquote(&parts[1]),
            value: unquote(&parts[2]),
        });
    }

    if let Some(inner) = strip_func(s, "confidence") {
        let parts = split_args(inner);
        if parts.len() != 3 {
            return Err(RuleParseError("confidence condition needs 3 args".into()));
        }
        let op_str = unquote(&parts[1]);
        let op = match op_str.as_str() {
            ">" => ConditionOp::Gt,
            ">=" => ConditionOp::Gte,
            "<" => ConditionOp::Lt,
            "<=" => ConditionOp::Lte,
            _ => return Err(RuleParseError(format!("unknown op: {}", op_str))),
        };
        let threshold: f32 = parts[2].parse()
            .map_err(|_| RuleParseError(format!("invalid threshold: {}", parts[2])))?;
        return Ok(Condition::Confidence {
            var: parts[0].clone(),
            op,
            threshold,
        });
    }

    Err(RuleParseError(format!("unknown condition: {s}")))
}

fn parse_action(s: &str) -> Result<Action, RuleParseError> {
    if let Some(inner) = strip_func(s, "edge") {
        let parts = split_args(inner);
        if parts.len() < 3 {
            return Err(RuleParseError("edge action needs at least 3 args".into()));
        }
        let confidence_expr = if parts.len() >= 4 {
            parse_confidence_expr(&parts[3])?
        } else {
            ConfidenceExpr::Literal(0.50)
        };
        return Ok(Action::CreateEdge {
            from_var: parts[0].clone(),
            relationship: unquote(&parts[1]),
            to_var: parts[2].clone(),
            confidence_expr,
        });
    }

    if let Some(inner) = strip_func(s, "flag") {
        let parts = split_args(inner);
        if parts.len() != 2 {
            return Err(RuleParseError("flag action needs 2 args".into()));
        }
        return Ok(Action::Flag {
            node_var: parts[0].clone(),
            reason: unquote(&parts[1]),
        });
    }

    Err(RuleParseError(format!("unknown action: {s}")))
}

fn parse_confidence_expr(s: &str) -> Result<ConfidenceExpr, RuleParseError> {
    let s = s.trim();
    if let Some(inner) = strip_func(s, "min") {
        let parts = split_args(inner);
        if parts.len() != 2 {
            return Err(RuleParseError("min() needs 2 args".into()));
        }
        return Ok(ConfidenceExpr::Min(parts[0].clone(), parts[1].clone()));
    }
    if let Some(inner) = strip_func(s, "product") {
        let parts = split_args(inner);
        if parts.len() != 2 {
            return Err(RuleParseError("product() needs 2 args".into()));
        }
        return Ok(ConfidenceExpr::Product(parts[0].clone(), parts[1].clone()));
    }
    // Try literal
    if let Ok(v) = s.parse::<f32>() {
        return Ok(ConfidenceExpr::Literal(v));
    }
    Err(RuleParseError(format!("invalid confidence expr: {s}")))
}

fn strip_func<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let s = s.trim();
    if s.starts_with(name) && s[name.len()..].starts_with('(') && s.ends_with(')') {
        Some(&s[name.len() + 1..s.len() - 1])
    } else {
        None
    }
}

fn split_args(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_quote = false;

    for ch in s.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                current.push(ch);
            }
            '(' if !in_quote => {
                depth += 1;
                current.push(ch);
            }
            ')' if !in_quote => {
                depth -= 1;
                current.push(ch);
            }
            ',' if !in_quote && depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        parts.push(trimmed);
    }
    parts
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Rule parse error.
#[derive(Debug, Clone)]
pub struct RuleParseError(pub String);

impl fmt::Display for RuleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rule parse error: {}", self.0)
    }
}

impl std::error::Error for RuleParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_transitive_rule() {
        let input = r#"
rule transitive_type
when edge(A, "is_a", B)
when edge(B, "is_a", C)
then edge(A, "is_a", C, min(e1, e2))
"#;
        let rule = parse_rule(input).unwrap();
        assert_eq!(rule.name, "transitive_type");
        assert_eq!(rule.conditions.len(), 2);
        assert_eq!(rule.actions.len(), 1);

        match &rule.conditions[0] {
            Condition::Edge { from_var, relationship, to_var } => {
                assert_eq!(from_var, "A");
                assert_eq!(relationship, "is_a");
                assert_eq!(to_var, "B");
            }
            _ => panic!("expected Edge condition"),
        }
    }

    #[test]
    fn parse_flag_rule() {
        let input = r#"
rule stale_warning
when confidence(node, "<", 0.2)
then flag(node, "low confidence — review needed")
"#;
        let rule = parse_rule(input).unwrap();
        assert_eq!(rule.name, "stale_warning");
        match &rule.conditions[0] {
            Condition::Confidence { var, op, threshold } => {
                assert_eq!(var, "node");
                assert_eq!(*op, ConditionOp::Lt);
                assert!((*threshold - 0.2).abs() < f32::EPSILON);
            }
            _ => panic!("expected Confidence condition"),
        }
        match &rule.actions[0] {
            Action::Flag { node_var, reason } => {
                assert_eq!(node_var, "node");
                assert!(reason.contains("low confidence"));
            }
            _ => panic!("expected Flag action"),
        }
    }

    #[test]
    fn parse_error_no_name() {
        let input = "when edge(A, \"is_a\", B)\nthen edge(A, \"is_a\", C)";
        assert!(parse_rule(input).is_err());
    }

    #[test]
    fn parse_error_no_conditions() {
        let input = "rule test\nthen edge(A, \"is_a\", C)";
        assert!(parse_rule(input).is_err());
    }

    #[test]
    fn parse_confidence_literal() {
        let input = r#"
rule simple
when edge(A, "knows", B)
then edge(A, "knows", B, 0.75)
"#;
        let rule = parse_rule(input).unwrap();
        match &rule.actions[0] {
            Action::CreateEdge { confidence_expr, .. } => {
                match confidence_expr {
                    ConfidenceExpr::Literal(v) => assert!((*v - 0.75).abs() < f32::EPSILON),
                    _ => panic!("expected Literal"),
                }
            }
            _ => panic!("expected CreateEdge"),
        }
    }
}

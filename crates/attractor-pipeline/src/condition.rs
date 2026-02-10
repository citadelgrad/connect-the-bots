//! Condition expression language: parser and evaluator.
//!
//! Grammar:
//! ```text
//! ConditionExpr  ::= Clause ( '&&' Clause )*
//! Clause         ::= Key Operator Literal
//! Key            ::= identifier ( '.' identifier )*
//! Operator       ::= '=' | '!='
//! Literal        ::= QuotedString | BareWord | Integer | Boolean
//! ```

use attractor_types::AttractorError;

/// A parsed condition expression consisting of one or more clauses joined by `&&`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionExpr {
    pub clauses: Vec<Clause>,
}

/// A single comparison clause: `key op value`.
#[derive(Debug, Clone, PartialEq)]
pub struct Clause {
    pub key: String,
    pub operator: Operator,
    pub value: String,
}

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Eq,
    NotEq,
}

/// Parse a condition string into a [`ConditionExpr`].
///
/// An empty or whitespace-only input produces an expression with zero clauses,
/// which [`evaluate_condition`] treats as always true.
pub fn parse_condition(input: &str) -> Result<ConditionExpr, AttractorError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(ConditionExpr {
            clauses: Vec::new(),
        });
    }

    let mut clauses = Vec::new();
    for part in trimmed.split("&&") {
        let clause = parse_clause(part.trim())?;
        clauses.push(clause);
    }

    Ok(ConditionExpr { clauses })
}

fn parse_clause(input: &str) -> Result<Clause, AttractorError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(make_error("empty clause"));
    }

    // Find the operator position. We need to distinguish `!=` from `=`.
    let (key_end, operator, op_len) = find_operator(input)?;

    let key = input[..key_end].trim().to_string();
    if key.is_empty() {
        return Err(make_error("missing key before operator"));
    }
    // Validate key: identifier segments separated by dots.
    for seg in key.split('.') {
        if seg.is_empty() || !seg.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(make_error(&format!("invalid key segment: '{seg}'")));
        }
    }

    let raw_value = input[key_end + op_len..].trim();
    if raw_value.is_empty() {
        return Err(make_error(&format!("missing value after operator in '{input}'")));
    }

    let value = strip_quotes(raw_value);

    Ok(Clause {
        key,
        operator,
        value,
    })
}

fn find_operator(input: &str) -> Result<(usize, Operator, usize), AttractorError> {
    // Scan for `!=` or `=`, skipping quoted regions.
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                // Skip quoted string.
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                i += 1; // skip closing quote
            }
            b'!' if i + 1 < bytes.len() && bytes[i + 1] == b'=' => {
                return Ok((i, Operator::NotEq, 2));
            }
            b'=' => {
                return Ok((i, Operator::Eq, 1));
            }
            _ => {
                i += 1;
            }
        }
    }
    Err(make_error(&format!("no operator found in '{input}'")))
}

fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
        {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

fn make_error(msg: &str) -> AttractorError {
    AttractorError::ValidationError(format!("condition parse error: {msg}"))
}

/// Evaluate a condition expression against values provided by a resolver function.
///
/// The `resolve` function maps a key (e.g. `"outcome"`, `"context.env"`) to its
/// string value. Keys not found should resolve to an empty string.
///
/// An expression with zero clauses (from an empty input) evaluates to `true`.
pub fn evaluate_condition(expr: &ConditionExpr, resolve: &dyn Fn(&str) -> String) -> bool {
    // Empty expression → always true.
    if expr.clauses.is_empty() {
        return true;
    }

    expr.clauses.iter().all(|clause| {
        let actual = resolve(&clause.key);
        match clause.operator {
            Operator::Eq => actual == clause.value,
            Operator::NotEq => actual != clause.value,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_resolve(outcome: &str) -> impl Fn(&str) -> String + '_ {
        move |key: &str| match key {
            "outcome" => outcome.to_string(),
            "preferred_label" => "my_label".to_string(),
            "context.tests_passed" => "true".to_string(),
            "context.env" => "prod".to_string(),
            _ => String::new(),
        }
    }

    #[test]
    fn simple_equality() {
        let expr = parse_condition("outcome=success").unwrap();
        assert_eq!(expr.clauses.len(), 1);
        assert_eq!(expr.clauses[0].key, "outcome");
        assert_eq!(expr.clauses[0].operator, Operator::Eq);
        assert_eq!(expr.clauses[0].value, "success");

        assert!(evaluate_condition(&expr, &simple_resolve("success")));
        assert!(!evaluate_condition(&expr, &simple_resolve("fail")));
    }

    #[test]
    fn not_equal() {
        let expr = parse_condition("outcome!=fail").unwrap();
        assert_eq!(expr.clauses[0].operator, Operator::NotEq);
        assert_eq!(expr.clauses[0].value, "fail");

        assert!(evaluate_condition(&expr, &simple_resolve("success")));
        assert!(!evaluate_condition(&expr, &simple_resolve("fail")));
    }

    #[test]
    fn compound_condition() {
        let expr = parse_condition("outcome=success && context.tests_passed=true").unwrap();
        assert_eq!(expr.clauses.len(), 2);
        assert_eq!(expr.clauses[0].key, "outcome");
        assert_eq!(expr.clauses[1].key, "context.tests_passed");

        assert!(evaluate_condition(&expr, &simple_resolve("success")));
        assert!(!evaluate_condition(&expr, &simple_resolve("fail")));
    }

    #[test]
    fn empty_condition_always_true() {
        let expr = parse_condition("").unwrap();
        assert!(expr.clauses.is_empty());
        assert!(evaluate_condition(&expr, &simple_resolve("anything")));

        let expr2 = parse_condition("   ").unwrap();
        assert!(evaluate_condition(&expr2, &simple_resolve("anything")));
    }

    #[test]
    fn missing_context_key_empty_string() {
        // Empty value after operator is a parse error
        assert!(parse_condition("context.unknown=").is_err());

        // But comparing a missing context key to a value works: missing resolves to ""
        let expr = parse_condition("context.unknown!=something").unwrap();
        assert!(evaluate_condition(&expr, &simple_resolve("success")));
    }

    #[test]
    fn quoted_string_values() {
        let expr = parse_condition(r#"outcome="success""#).unwrap();
        assert_eq!(expr.clauses[0].value, "success");
        assert!(evaluate_condition(&expr, &simple_resolve("success")));

        let expr2 = parse_condition("outcome='success'").unwrap();
        assert_eq!(expr2.clauses[0].value, "success");
        assert!(evaluate_condition(&expr2, &simple_resolve("success")));
    }

    #[test]
    fn parse_error_invalid_syntax() {
        // No operator
        assert!(parse_condition("outcome").is_err());
        // Empty clause in compound
        assert!(parse_condition("outcome=success && ").is_err());
        // Invalid key
        assert!(parse_condition("=value").is_err());
    }

    #[test]
    fn integer_values_as_strings() {
        let expr = parse_condition("context.count=42").unwrap();
        assert_eq!(expr.clauses[0].value, "42");

        let resolve = |key: &str| -> String {
            if key == "context.count" {
                "42".to_string()
            } else {
                String::new()
            }
        };
        assert!(evaluate_condition(&expr, &resolve));
    }
}

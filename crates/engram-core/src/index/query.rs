/// Query language parser for engram.
///
/// Syntax examples:
///   "postgresql"                          — full-text search
///   label:server-01                       — exact label match
///   type:database                         — filter by node type name
///   tier:core                             — filter by memory tier
///   confidence>0.8                        — confidence filter
///   created:2024-01-01..2024-12-31        — temporal range (created_at)
///   event:2024-01-01..2024-12-31          — temporal range (event_time)
///   prop:role=database                    — property filter
///   "web server" AND type:server          — boolean AND
///   tier:core OR tier:active              — boolean OR

use std::fmt;

/// A parsed query
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// Full-text search
    FullText(String),
    /// Exact label match
    Label(String),
    /// Filter by node type name
    NodeType(String),
    /// Filter by memory tier
    Tier(String),
    /// Filter by sensitivity level
    Sensitivity(String),
    /// Confidence comparison
    Confidence { op: CmpOp, value: f32 },
    /// Temporal range on created_at (unix nanos)
    CreatedRange { from: Option<i64>, to: Option<i64> },
    /// Temporal range on event_time (unix nanos)
    EventRange { from: Option<i64>, to: Option<i64> },
    /// Property key=value match
    Property { key: String, value: String },
    /// Boolean AND of two queries
    And(Box<Query>, Box<Query>),
    /// Boolean OR of two queries
    Or(Box<Query>, Box<Query>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CmpOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmpOp::Gt => write!(f, ">"),
            CmpOp::Gte => write!(f, ">="),
            CmpOp::Lt => write!(f, "<"),
            CmpOp::Lte => write!(f, "<="),
            CmpOp::Eq => write!(f, "="),
        }
    }
}

/// Parse a query string into a Query AST.
pub fn parse(input: &str) -> Result<Query, ParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ParseError("empty query".into()));
    }

    parse_or(input)
}

#[derive(Debug, Clone)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

// --- Parser internals ---

/// Parse OR expressions: expr OR expr
fn parse_or(input: &str) -> Result<Query, ParseError> {
    if let Some((left, right)) = split_at_keyword(input, " OR ") {
        let lq = parse_and(left)?;
        let rq = parse_or(right)?;
        Ok(Query::Or(Box::new(lq), Box::new(rq)))
    } else {
        parse_and(input)
    }
}

/// Parse AND expressions: expr AND expr
fn parse_and(input: &str) -> Result<Query, ParseError> {
    if let Some((left, right)) = split_at_keyword(input, " AND ") {
        let lq = parse_atom(left)?;
        let rq = parse_and(right)?;
        Ok(Query::And(Box::new(lq), Box::new(rq)))
    } else {
        parse_atom(input)
    }
}

/// Parse a single query atom (filter or full-text term)
fn parse_atom(input: &str) -> Result<Query, ParseError> {
    let input = input.trim();

    // Quoted full-text search
    if input.starts_with('"') && input.ends_with('"') && input.len() > 2 {
        return Ok(Query::FullText(input[1..input.len() - 1].to_string()));
    }

    // label:value
    if let Some(val) = input.strip_prefix("label:") {
        return Ok(Query::Label(val.to_string()));
    }

    // type:value
    if let Some(val) = input.strip_prefix("type:") {
        return Ok(Query::NodeType(val.to_string()));
    }

    // tier:value
    if let Some(val) = input.strip_prefix("tier:") {
        return Ok(Query::Tier(val.to_string()));
    }

    // sensitivity:value
    if let Some(val) = input.strip_prefix("sensitivity:") {
        return Ok(Query::Sensitivity(val.to_string()));
    }

    // confidence>=0.8, confidence>0.5, etc.
    if let Some(rest) = input.strip_prefix("confidence") {
        return parse_confidence(rest);
    }

    // created:from..to
    if let Some(val) = input.strip_prefix("created:") {
        return parse_time_range(val).map(|(from, to)| Query::CreatedRange { from, to });
    }

    // event:from..to
    if let Some(val) = input.strip_prefix("event:") {
        return parse_time_range(val).map(|(from, to)| Query::EventRange { from, to });
    }

    // prop:key=value
    if let Some(val) = input.strip_prefix("prop:") {
        if let Some((key, value)) = val.split_once('=') {
            return Ok(Query::Property {
                key: key.to_string(),
                value: value.to_string(),
            });
        }
        return Err(ParseError(format!("invalid property filter: {val}")));
    }

    // Default: full-text search
    Ok(Query::FullText(input.to_string()))
}

fn parse_confidence(rest: &str) -> Result<Query, ParseError> {
    let (op, val_str) = if let Some(v) = rest.strip_prefix(">=") {
        (CmpOp::Gte, v)
    } else if let Some(v) = rest.strip_prefix(">") {
        (CmpOp::Gt, v)
    } else if let Some(v) = rest.strip_prefix("<=") {
        (CmpOp::Lte, v)
    } else if let Some(v) = rest.strip_prefix("<") {
        (CmpOp::Lt, v)
    } else if let Some(v) = rest.strip_prefix("=") {
        (CmpOp::Eq, v)
    } else {
        return Err(ParseError(format!("invalid confidence filter: confidence{rest}")));
    };

    let value: f32 = val_str
        .trim()
        .parse()
        .map_err(|_| ParseError(format!("invalid confidence value: {val_str}")))?;

    Ok(Query::Confidence { op, value })
}

fn parse_time_range(val: &str) -> Result<(Option<i64>, Option<i64>), ParseError> {
    if let Some((from_str, to_str)) = val.split_once("..") {
        let from = if from_str.is_empty() {
            None
        } else {
            Some(parse_timestamp(from_str)?)
        };
        let to = if to_str.is_empty() {
            None
        } else {
            Some(parse_timestamp(to_str)?)
        };
        Ok((from, to))
    } else {
        // Single timestamp — exact match (range of that day/second)
        let ts = parse_timestamp(val)?;
        Ok((Some(ts), Some(ts)))
    }
}

/// Parse a timestamp from either epoch nanos or ISO date (YYYY-MM-DD).
fn parse_timestamp(s: &str) -> Result<i64, ParseError> {
    let s = s.trim();

    // Try epoch nanos first
    if let Ok(nanos) = s.parse::<i64>() {
        return Ok(nanos);
    }

    // Try ISO date YYYY-MM-DD
    if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
        let year: i32 = s[..4].parse().map_err(|_| ParseError(format!("invalid date: {s}")))?;
        let month: u32 = s[5..7].parse().map_err(|_| ParseError(format!("invalid date: {s}")))?;
        let day: u32 = s[8..10].parse().map_err(|_| ParseError(format!("invalid date: {s}")))?;

        // Simple days-since-epoch calculation (not accounting for leap seconds)
        let days = days_since_epoch(year, month, day)
            .ok_or_else(|| ParseError(format!("invalid date: {s}")))?;
        let nanos = days as i64 * 86_400_000_000_000i64;
        return Ok(nanos);
    }

    Err(ParseError(format!("invalid timestamp: {s}")))
}

fn days_since_epoch(year: i32, month: u32, day: u32) -> Option<i64> {
    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }
    // Simplified: calculate days from 1970-01-01
    let mut total_days: i64 = 0;
    for y in 1970..year {
        total_days += if is_leap(y) { 366 } else { 365 };
    }
    // Could be negative for years before 1970, but that's fine
    if year < 1970 {
        for y in year..1970 {
            total_days -= if is_leap(y) { 366 } else { 365 };
        }
    }
    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += month_days[m as usize] as i64;
        if m == 2 && is_leap(year) {
            total_days += 1;
        }
    }
    total_days += (day - 1) as i64;
    Some(total_days)
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Split at a keyword, but not inside quotes.
fn split_at_keyword<'a>(input: &'a str, keyword: &str) -> Option<(&'a str, &'a str)> {
    let mut in_quotes = false;
    let kw_bytes = keyword.as_bytes();
    let input_bytes = input.as_bytes();

    for i in 0..input_bytes.len() {
        if input_bytes[i] == b'"' {
            in_quotes = !in_quotes;
        }
        if !in_quotes && i + kw_bytes.len() <= input_bytes.len() {
            if &input_bytes[i..i + kw_bytes.len()] == kw_bytes {
                return Some((&input[..i], &input[i + kw_bytes.len()..]));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fulltext() {
        let q = parse("postgresql server").unwrap();
        assert_eq!(q, Query::FullText("postgresql server".into()));
    }

    #[test]
    fn parse_quoted_fulltext() {
        let q = parse("\"web server\"").unwrap();
        assert_eq!(q, Query::FullText("web server".into()));
    }

    #[test]
    fn parse_label() {
        let q = parse("label:server-01").unwrap();
        assert_eq!(q, Query::Label("server-01".into()));
    }

    #[test]
    fn parse_type() {
        let q = parse("type:database").unwrap();
        assert_eq!(q, Query::NodeType("database".into()));
    }

    #[test]
    fn parse_tier() {
        let q = parse("tier:core").unwrap();
        assert_eq!(q, Query::Tier("core".into()));
    }

    #[test]
    fn parse_confidence() {
        let q = parse("confidence>=0.8").unwrap();
        assert_eq!(q, Query::Confidence { op: CmpOp::Gte, value: 0.8 });
    }

    #[test]
    fn parse_confidence_gt() {
        let q = parse("confidence>0.5").unwrap();
        assert_eq!(q, Query::Confidence { op: CmpOp::Gt, value: 0.5 });
    }

    #[test]
    fn parse_property() {
        let q = parse("prop:role=database").unwrap();
        assert_eq!(q, Query::Property { key: "role".into(), value: "database".into() });
    }

    #[test]
    fn parse_created_range() {
        let q = parse("created:2024-01-01..2024-12-31").unwrap();
        match q {
            Query::CreatedRange { from, to } => {
                assert!(from.is_some());
                assert!(to.is_some());
            }
            _ => panic!("expected CreatedRange"),
        }
    }

    #[test]
    fn parse_and() {
        let q = parse("type:server AND confidence>0.5").unwrap();
        match q {
            Query::And(left, right) => {
                assert_eq!(*left, Query::NodeType("server".into()));
                assert_eq!(*right, Query::Confidence { op: CmpOp::Gt, value: 0.5 });
            }
            _ => panic!("expected And"),
        }
    }

    #[test]
    fn parse_or() {
        let q = parse("tier:core OR tier:active").unwrap();
        match q {
            Query::Or(left, right) => {
                assert_eq!(*left, Query::Tier("core".into()));
                assert_eq!(*right, Query::Tier("active".into()));
            }
            _ => panic!("expected Or"),
        }
    }

    #[test]
    fn parse_complex() {
        // OR has lower precedence than AND
        let q = parse("type:server AND confidence>0.8 OR tier:core").unwrap();
        match q {
            Query::Or(left, right) => {
                assert!(matches!(*left, Query::And(_, _)));
                assert_eq!(*right, Query::Tier("core".into()));
            }
            _ => panic!("expected Or wrapping And"),
        }
    }

    #[test]
    fn parse_empty_returns_error() {
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_open_range() {
        let q = parse("created:..2024-06-01").unwrap();
        match q {
            Query::CreatedRange { from, to } => {
                assert!(from.is_none());
                assert!(to.is_some());
            }
            _ => panic!("expected CreatedRange"),
        }
    }
}

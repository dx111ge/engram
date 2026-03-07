/// Multi-tenant authentication and authorization.
///
/// API key-based auth with role and topic-level ACLs.
///
/// Config format (TOML):
///   [users.sven]
///   key = "sk-..."
///   role = "admin"
///   topics = ["*"]
///
///   [users.dev-agent]
///   key = "sk-..."
///   role = "write"
///   topics = ["code", "architecture"]
///   deny_read = ["credentials"]

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

/// User role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Write,
    Read,
}

impl Role {
    pub fn can_write(&self) -> bool {
        matches!(self, Role::Admin | Role::Write)
    }

    pub fn can_read(&self) -> bool {
        true // all roles can read (within topic ACL)
    }
}

/// A single user's access configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct UserConfig {
    pub key: String,
    pub role: Role,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub deny_read: Vec<String>,
}

impl UserConfig {
    /// Check if user can access a given topic.
    pub fn can_access_topic(&self, topic: &str) -> bool {
        if self.deny_read.iter().any(|t| t == topic) {
            return false;
        }
        self.topics.iter().any(|t| t == "*" || t == topic)
    }
}

/// Auth configuration — loaded from TOML file.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub users: HashMap<String, UserConfig>,
    /// When true, requests without API key are rejected.
    #[serde(default)]
    pub enabled: bool,
}

impl AuthConfig {
    /// Load from a TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        // Minimal TOML parser for our specific format.
        // We parse [users.NAME] sections manually since we don't want a toml dependency.
        let mut config = AuthConfig {
            users: HashMap::new(),
            enabled: true,
        };

        let mut current_user: Option<String> = None;
        let mut current_key = String::new();
        let mut current_role = Role::Read;
        let mut current_topics: Vec<String> = Vec::new();
        let mut current_deny: Vec<String> = Vec::new();

        for line in toml_str.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Section header: [users.name]
            if line.starts_with("[users.") && line.ends_with(']') {
                // Save previous user
                if let Some(ref name) = current_user {
                    config.users.insert(
                        name.clone(),
                        UserConfig {
                            key: current_key.clone(),
                            role: current_role,
                            topics: current_topics.clone(),
                            deny_read: current_deny.clone(),
                        },
                    );
                }

                let name = &line[7..line.len() - 1];
                current_user = Some(name.to_string());
                current_key.clear();
                current_role = Role::Read;
                current_topics.clear();
                current_deny.clear();
                continue;
            }

            if current_user.is_none() {
                // Top-level key
                if let Some((k, v)) = line.split_once('=') {
                    let k = k.trim();
                    let v = v.trim();
                    if k == "enabled" {
                        config.enabled = v == "true";
                    }
                }
                continue;
            }

            // Key-value in user section
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                match k {
                    "key" => current_key = unquote(v),
                    "role" => {
                        current_role = match unquote(v).as_str() {
                            "admin" => Role::Admin,
                            "write" => Role::Write,
                            _ => Role::Read,
                        };
                    }
                    "topics" => current_topics = parse_string_array(v),
                    "deny_read" => current_deny = parse_string_array(v),
                    _ => {}
                }
            }
        }

        // Save last user
        if let Some(ref name) = current_user {
            config.users.insert(
                name.clone(),
                UserConfig {
                    key: current_key,
                    role: current_role,
                    topics: current_topics,
                    deny_read: current_deny,
                },
            );
        }

        Ok(config)
    }

    /// Look up a user by API key.
    pub fn authenticate(&self, api_key: &str) -> Option<(&str, &UserConfig)> {
        self.users
            .iter()
            .find(|(_, cfg)| cfg.key == api_key)
            .map(|(name, cfg)| (name.as_str(), cfg))
    }
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let s = s.strip_prefix('[').unwrap_or(s);
    let s = s.strip_suffix(']').unwrap_or(s);
    s.split(',')
        .map(|item| unquote(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

/// Axum middleware for API key authentication.
pub async fn auth_middleware(
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract auth config from extensions
    let auth = request
        .extensions()
        .get::<Arc<AuthConfig>>()
        .cloned();

    if let Some(auth) = auth {
        if auth.enabled {
            let api_key = request
                .headers()
                .get("x-api-key")
                .or_else(|| request.headers().get("authorization"))
                .and_then(|v| v.to_str().ok())
                .map(|v| v.strip_prefix("Bearer ").unwrap_or(v));

            match api_key {
                Some(key) => {
                    if auth.authenticate(key).is_none() {
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                }
                None => return Err(StatusCode::UNAUTHORIZED),
            }
        }
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_auth_config() {
        let toml = r#"
enabled = true

[users.sven]
key = "sk-admin-123"
role = "admin"
topics = ["*"]

[users.agent]
key = "sk-agent-456"
role = "write"
topics = ["code", "architecture"]
deny_read = ["credentials", "hr"]

[users.dashboard]
key = "sk-dash-789"
role = "read"
topics = ["incidents"]
"#;
        let config = AuthConfig::from_toml(toml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.users.len(), 3);

        let sven = &config.users["sven"];
        assert_eq!(sven.role, Role::Admin);
        assert!(sven.can_access_topic("anything"));

        let agent = &config.users["agent"];
        assert_eq!(agent.role, Role::Write);
        assert!(agent.role.can_write());
        assert!(agent.can_access_topic("code"));
        assert!(!agent.can_access_topic("credentials"));

        let dashboard = &config.users["dashboard"];
        assert_eq!(dashboard.role, Role::Read);
        assert!(!dashboard.role.can_write());
    }

    #[test]
    fn authenticate_by_key() {
        let toml = r#"
enabled = true

[users.test]
key = "sk-test-key"
role = "admin"
topics = ["*"]
"#;
        let config = AuthConfig::from_toml(toml).unwrap();
        assert!(config.authenticate("sk-test-key").is_some());
        assert!(config.authenticate("wrong-key").is_none());
    }

    #[test]
    fn disabled_auth() {
        let config = AuthConfig::default();
        assert!(!config.enabled);
    }
}

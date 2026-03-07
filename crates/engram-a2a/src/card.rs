/// Agent Card — the A2A discovery document.
///
/// Served at `GET /.well-known/agent.json`. Describes what engram can do,
/// following the Google A2A protocol specification (v0.2).

/// Agent Card returned at /.well-known/agent.json.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: String,
    pub version: String,
    pub protocol_version: String,
    pub capabilities: Capabilities,
    pub skills: Vec<Skill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<Authentication>,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
}

/// What the agent supports.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    pub state_transition_history: bool,
}

/// A skill the agent can perform.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub examples: Vec<String>,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
}

/// Authentication schemes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Authentication {
    pub schemes: Vec<String>,
}

impl AgentCard {
    /// Build the default engram agent card.
    pub fn engram_default(base_url: &str) -> Self {
        AgentCard {
            name: "engram".to_string(),
            description: "High-performance AI memory engine. Store, query, and reason over \
                knowledge graphs with GPU-accelerated traversal.".to_string(),
            url: base_url.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: "0.2".to_string(),
            capabilities: Capabilities {
                streaming: true,
                push_notifications: true,
                state_transition_history: true,
            },
            skills: vec![
                Skill {
                    id: "store-knowledge".to_string(),
                    name: "Store Knowledge".to_string(),
                    description: "Store facts, entities, and relationships in the knowledge \
                        graph with confidence scoring and provenance tracking.".to_string(),
                    tags: vec!["memory", "knowledge", "store", "facts"]
                        .into_iter().map(String::from).collect(),
                    examples: vec![
                        "Remember that server-01 runs PostgreSQL 15".to_string(),
                        "The CEO approved the budget on March 1st".to_string(),
                        "Python 3.12 introduced generic type syntax".to_string(),
                    ],
                    input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
                    output_modes: vec!["application/json".to_string()],
                },
                Skill {
                    id: "query-knowledge".to_string(),
                    name: "Query Knowledge".to_string(),
                    description: "Query the knowledge graph with graph traversal, semantic \
                        similarity, or natural language. Returns facts with confidence scores \
                        and provenance.".to_string(),
                    tags: vec!["memory", "knowledge", "query", "search", "recall"]
                        .into_iter().map(String::from).collect(),
                    examples: vec![
                        "What do we know about server-01?".to_string(),
                        "Find all causes of the outage last Tuesday".to_string(),
                        "What technologies does the payment team use?".to_string(),
                    ],
                    input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
                    output_modes: vec!["application/json".to_string()],
                },
                Skill {
                    id: "reason".to_string(),
                    name: "Reason & Prove".to_string(),
                    description: "Use logical inference to derive new knowledge, prove \
                        hypotheses, or detect contradictions in stored knowledge.".to_string(),
                    tags: vec!["reasoning", "inference", "proof", "logic"]
                        .into_iter().map(String::from).collect(),
                    examples: vec![
                        "Why might the database be slow?".to_string(),
                        "Is it true that all production servers have monitoring?".to_string(),
                        "Are there any contradictions about the release date?".to_string(),
                    ],
                    input_modes: vec!["text/plain".to_string()],
                    output_modes: vec!["application/json".to_string()],
                },
                Skill {
                    id: "learn".to_string(),
                    name: "Learn & Correct".to_string(),
                    description: "Reinforce confirmed knowledge, correct wrong facts, or \
                        trigger knowledge decay. Continuous learning from feedback.".to_string(),
                    tags: vec!["learning", "correction", "feedback", "memory"]
                        .into_iter().map(String::from).collect(),
                    examples: vec![
                        "That fact about the server IP was wrong, it's actually 10.0.0.5".to_string(),
                        "Confirm that the deployment succeeded".to_string(),
                        "Forget outdated information about the old API".to_string(),
                    ],
                    input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
                    output_modes: vec!["application/json".to_string()],
                },
                Skill {
                    id: "explain".to_string(),
                    name: "Explain Provenance".to_string(),
                    description: "Explain how a fact was derived, its full provenance chain, \
                        confidence history, and supporting/contradicting evidence.".to_string(),
                    tags: vec!["provenance", "explain", "trust", "audit"]
                        .into_iter().map(String::from).collect(),
                    examples: vec![
                        "How do we know that server-01 is in the EU datacenter?".to_string(),
                        "What's the evidence for this security recommendation?".to_string(),
                        "Why is the confidence for this fact so low?".to_string(),
                    ],
                    input_modes: vec!["text/plain".to_string()],
                    output_modes: vec!["application/json".to_string()],
                },
            ],
            authentication: Some(Authentication {
                schemes: vec!["bearer".to_string()],
            }),
            default_input_modes: vec!["text/plain".to_string(), "application/json".to_string()],
            default_output_modes: vec!["application/json".to_string()],
        }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Find a skill by ID.
    pub fn find_skill(&self, id: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.id == id)
    }

    /// List all skill IDs.
    pub fn skill_ids(&self) -> Vec<&str> {
        self.skills.iter().map(|s| s.id.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_card() {
        let card = AgentCard::engram_default("http://localhost:3030");
        assert_eq!(card.name, "engram");
        assert_eq!(card.protocol_version, "0.2");
        assert_eq!(card.skills.len(), 5);
        assert!(card.capabilities.streaming);
    }

    #[test]
    fn find_skill() {
        let card = AgentCard::engram_default("http://localhost:3030");
        let skill = card.find_skill("query-knowledge").unwrap();
        assert_eq!(skill.name, "Query Knowledge");
        assert!(card.find_skill("nonexistent").is_none());
    }

    #[test]
    fn skill_ids() {
        let card = AgentCard::engram_default("http://localhost:3030");
        let ids = card.skill_ids();
        assert_eq!(ids, vec!["store-knowledge", "query-knowledge", "reason", "learn", "explain"]);
    }

    #[test]
    fn json_roundtrip() {
        let card = AgentCard::engram_default("http://localhost:3030");
        let json = card.to_json();
        let parsed: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "engram");
        assert_eq!(parsed.skills.len(), 5);
        // Verify camelCase serialization
        assert!(json.contains("protocolVersion"));
        assert!(json.contains("pushNotifications"));
        assert!(json.contains("inputModes"));
    }

    #[test]
    fn well_known_path() {
        let card = AgentCard::engram_default("http://localhost:3030");
        assert_eq!(card.url, "http://localhost:3030");
    }
}

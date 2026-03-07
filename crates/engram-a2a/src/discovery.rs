/// Agent discovery — find other A2A-compatible agents.
///
/// Agents advertise themselves at /.well-known/agent.json. Discovery
/// involves fetching this endpoint from known agent URLs and caching
/// the results.

use crate::card::AgentCard;

/// A discovered agent in the network.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredAgent {
    /// Base URL where the agent is hosted
    pub url: String,
    /// The agent's card (capabilities, skills)
    pub card: AgentCard,
    /// When the card was last fetched (unix millis)
    pub last_fetched: u64,
    /// Whether the agent is currently reachable
    pub reachable: bool,
}

/// Agent registry for tracking known agents.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentRegistry {
    agents: Vec<DiscoveredAgent>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        AgentRegistry {
            agents: Vec::new(),
        }
    }

    /// Add or update a discovered agent.
    pub fn register(&mut self, agent: DiscoveredAgent) {
        if let Some(existing) = self.agents.iter_mut().find(|a| a.url == agent.url) {
            *existing = agent;
        } else {
            self.agents.push(agent);
        }
    }

    /// Remove an agent by URL.
    pub fn remove(&mut self, url: &str) -> bool {
        let before = self.agents.len();
        self.agents.retain(|a| a.url != url);
        self.agents.len() < before
    }

    /// Find agents that have a specific skill.
    pub fn find_by_skill(&self, skill_id: &str) -> Vec<&DiscoveredAgent> {
        self.agents
            .iter()
            .filter(|a| a.card.find_skill(skill_id).is_some())
            .collect()
    }

    /// Find agents with any matching tag.
    pub fn find_by_tag(&self, tag: &str) -> Vec<&DiscoveredAgent> {
        self.agents
            .iter()
            .filter(|a| {
                a.card.skills.iter().any(|s| s.tags.iter().any(|t| t == tag))
            })
            .collect()
    }

    /// List all known agents.
    pub fn all(&self) -> &[DiscoveredAgent] {
        &self.agents
    }

    /// List reachable agents.
    pub fn reachable(&self) -> Vec<&DiscoveredAgent> {
        self.agents.iter().filter(|a| a.reachable).collect()
    }

    /// Get agent by URL.
    pub fn get(&self, url: &str) -> Option<&DiscoveredAgent> {
        self.agents.iter().find(|a| a.url == url)
    }

    /// Mark an agent as reachable or unreachable.
    pub fn set_reachable(&mut self, url: &str, reachable: bool) {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.url == url) {
            agent.reachable = reachable;
        }
    }

    /// Number of known agents.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    /// Build the well-known URL for agent card discovery.
    pub fn well_known_url(base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        format!("{base}/.well-known/agent.json")
    }

    /// Save to file.
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Load from file.
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(url: &str) -> DiscoveredAgent {
        DiscoveredAgent {
            url: url.to_string(),
            card: AgentCard::engram_default(url),
            last_fetched: 1000,
            reachable: true,
        }
    }

    #[test]
    fn register_and_find() {
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://localhost:3030"));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("http://localhost:3030").is_some());
    }

    #[test]
    fn update_existing() {
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://localhost:3030"));
        let mut updated = make_agent("http://localhost:3030");
        updated.last_fetched = 2000;
        reg.register(updated);
        assert_eq!(reg.len(), 1); // still 1
        assert_eq!(reg.get("http://localhost:3030").unwrap().last_fetched, 2000);
    }

    #[test]
    fn find_by_skill() {
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://a:3030"));
        reg.register(make_agent("http://b:3030"));
        let found = reg.find_by_skill("query-knowledge");
        assert_eq!(found.len(), 2);
        let found = reg.find_by_skill("nonexistent");
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn find_by_tag() {
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://a:3030"));
        let found = reg.find_by_tag("memory");
        assert_eq!(found.len(), 1);
        let found = reg.find_by_tag("nonexistent");
        assert_eq!(found.len(), 0);
    }

    #[test]
    fn remove_agent() {
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://a:3030"));
        assert!(reg.remove("http://a:3030"));
        assert_eq!(reg.len(), 0);
        assert!(!reg.remove("http://nonexistent:3030"));
    }

    #[test]
    fn reachability() {
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://a:3030"));
        assert_eq!(reg.reachable().len(), 1);
        reg.set_reachable("http://a:3030", false);
        assert_eq!(reg.reachable().len(), 0);
    }

    #[test]
    fn well_known_url() {
        assert_eq!(
            AgentRegistry::well_known_url("http://localhost:3030"),
            "http://localhost:3030/.well-known/agent.json"
        );
        assert_eq!(
            AgentRegistry::well_known_url("http://localhost:3030/"),
            "http://localhost:3030/.well-known/agent.json"
        );
    }

    #[test]
    fn save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agents.json");
        let mut reg = AgentRegistry::new();
        reg.register(make_agent("http://a:3030"));
        reg.save(&path).unwrap();
        let loaded = AgentRegistry::load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
    }
}

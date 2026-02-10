/// Subagent management for spawning child agent sessions.
use std::collections::HashMap;

/// Configuration for spawning a subagent.
#[derive(Debug, Clone)]
pub struct SubagentConfig {
    pub name: String,
    pub model: String,
    pub system_prompt: String,
    pub max_tool_rounds: usize,
    pub tools: Vec<String>, // tool names to enable
}

/// Status of a spawned subagent.
#[derive(Debug, Clone, PartialEq)]
pub enum SubagentStatus {
    Running,
    Completed(String), // final output
    Failed(String),     // error message
}

/// Tracks spawned subagents.
pub struct SubagentManager {
    agents: HashMap<String, SubagentStatus>,
    next_id: usize,
}

impl SubagentManager {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            next_id: 0,
        }
    }

    /// Register a new subagent (would be spawned by the session).
    pub fn register(&mut self, name: &str) -> String {
        let id = format!("subagent-{}", self.next_id);
        self.next_id += 1;
        self.agents.insert(id.clone(), SubagentStatus::Running);
        tracing::info!(id = %id, name = %name, "Subagent registered");
        id
    }

    /// Update subagent status.
    pub fn update_status(&mut self, id: &str, status: SubagentStatus) {
        if let Some(entry) = self.agents.get_mut(id) {
            *entry = status;
        }
    }

    /// Check if a subagent is still running.
    pub fn is_running(&self, id: &str) -> bool {
        matches!(self.agents.get(id), Some(SubagentStatus::Running))
    }

    /// Get all running subagent IDs.
    pub fn running_agents(&self) -> Vec<String> {
        self.agents
            .iter()
            .filter(|(_, s)| matches!(s, SubagentStatus::Running))
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get subagent status.
    pub fn status(&self, id: &str) -> Option<&SubagentStatus> {
        self.agents.get(id)
    }

    /// Total count of tracked agents.
    pub fn count(&self) -> usize {
        self.agents.len()
    }
}

impl Default for SubagentManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_track_subagent() {
        let mut mgr = SubagentManager::new();
        let id = mgr.register("worker-1");

        assert_eq!(id, "subagent-0");
        assert_eq!(mgr.count(), 1);
        assert!(mgr.is_running(&id));
        assert_eq!(mgr.status(&id), Some(&SubagentStatus::Running));
    }

    #[test]
    fn update_status_to_completed() {
        let mut mgr = SubagentManager::new();
        let id = mgr.register("worker-1");

        mgr.update_status(&id, SubagentStatus::Completed("done".into()));
        assert!(!mgr.is_running(&id));
        assert_eq!(
            mgr.status(&id),
            Some(&SubagentStatus::Completed("done".into()))
        );
    }

    #[test]
    fn update_status_to_failed() {
        let mut mgr = SubagentManager::new();
        let id = mgr.register("worker-1");

        mgr.update_status(&id, SubagentStatus::Failed("oops".into()));
        assert!(!mgr.is_running(&id));
        assert_eq!(
            mgr.status(&id),
            Some(&SubagentStatus::Failed("oops".into()))
        );
    }

    #[test]
    fn running_agents_filters_correctly() {
        let mut mgr = SubagentManager::new();
        let id1 = mgr.register("worker-1");
        let id2 = mgr.register("worker-2");
        let _id3 = mgr.register("worker-3");

        mgr.update_status(&id1, SubagentStatus::Completed("done".into()));
        mgr.update_status(&id2, SubagentStatus::Failed("err".into()));

        let running = mgr.running_agents();
        assert_eq!(running.len(), 1);
        assert!(running.contains(&"subagent-2".to_string()));
    }

    #[test]
    fn sequential_ids() {
        let mut mgr = SubagentManager::new();
        let id1 = mgr.register("a");
        let id2 = mgr.register("b");
        let id3 = mgr.register("c");

        assert_eq!(id1, "subagent-0");
        assert_eq!(id2, "subagent-1");
        assert_eq!(id3, "subagent-2");
        assert_eq!(mgr.count(), 3);
    }

    #[test]
    fn default_creates_empty_manager() {
        let mgr = SubagentManager::default();
        assert_eq!(mgr.count(), 0);
        assert!(mgr.running_agents().is_empty());
    }
}

use crate::registry::{
    AgentCard, AgentRegistry, AgentStatus, DiscoverQuery, FirstMatchSelector, PeerSelector,
};

#[tokio::test]
async fn register_and_get_by_id() {
    let registry = AgentRegistry::new();
    let card = AgentCard::new("agent-a", "Alpha", "First agent", vec!["code-gen".into()]);
    registry.register(card).await;

    let found = registry.get("agent-a").await;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Alpha");
    assert_eq!(found.capabilities, vec!["code-gen"]);
}

#[tokio::test]
async fn get_returns_none_for_unknown_id() {
    let registry = AgentRegistry::new();
    assert!(registry.get("unknown").await.is_none());
}

#[tokio::test]
async fn register_and_discover_by_capability() {
    let registry = AgentRegistry::new();
    registry
        .register(AgentCard::new(
            "agent-a",
            "Alpha",
            "Code gen",
            vec!["code-gen".into()],
        ))
        .await;
    registry
        .register(AgentCard::new(
            "agent-b",
            "Beta",
            "Research",
            vec!["research".into()],
        ))
        .await;

    let query = DiscoverQuery::new().with_capability("research");
    let results = registry.discover(&query).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id, "agent-b");
}

#[tokio::test]
async fn discover_by_status() {
    let registry = AgentRegistry::new();
    registry
        .register(
            AgentCard::new("agent-a", "A", "...", vec![]).with_status(AgentStatus::Online),
        )
        .await;
    registry
        .register(
            AgentCard::new("agent-b", "B", "...", vec![]).with_status(AgentStatus::Offline),
        )
        .await;

    let query = DiscoverQuery::new().with_status(AgentStatus::Offline);
    let results = registry.discover(&query).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id, "agent-b");
}

#[tokio::test]
async fn discover_with_combined_filters() {
    let registry = AgentRegistry::new();
    registry
        .register(
            AgentCard::new("agent-a", "A", "...", vec!["code-gen".into()])
                .with_status(AgentStatus::Online),
        )
        .await;
    registry
        .register(
            AgentCard::new("agent-b", "B", "...", vec!["code-gen".into()])
                .with_status(AgentStatus::Busy),
        )
        .await;
    registry
        .register(
            AgentCard::new("agent-c", "C", "...", vec!["research".into()])
                .with_status(AgentStatus::Online),
        )
        .await;

    let query = DiscoverQuery::new()
        .with_capability("code-gen")
        .with_status(AgentStatus::Online);
    let results = registry.discover(&query).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].agent_id, "agent-a");
}

#[tokio::test]
async fn deregister_removes_agent() {
    let registry = AgentRegistry::new();
    registry
        .register(AgentCard::new("agent-a", "A", "...", vec![]))
        .await;

    assert!(registry.deregister("agent-a").await);
    assert!(registry.get("agent-a").await.is_none());
}

#[tokio::test]
async fn deregister_returns_false_for_unknown() {
    let registry = AgentRegistry::new();
    assert!(!registry.deregister("ghost").await);
}

#[tokio::test]
async fn update_status() {
    let registry = AgentRegistry::new();
    registry
        .register(AgentCard::new("agent-a", "A", "...", vec![]))
        .await;

    assert!(registry.update_status("agent-a", AgentStatus::Busy).await);
    let card = registry.get("agent-a").await.unwrap();
    assert_eq!(card.status, AgentStatus::Busy);
}

#[tokio::test]
async fn update_status_returns_false_for_unknown() {
    let registry = AgentRegistry::new();
    assert!(
        !registry
            .update_status("ghost", AgentStatus::Offline)
            .await
    );
}

#[tokio::test]
async fn list_all_returns_full_registry() {
    let registry = AgentRegistry::new();
    registry
        .register(AgentCard::new("a", "A", "...", vec![]))
        .await;
    registry
        .register(AgentCard::new("b", "B", "...", vec![]))
        .await;
    registry
        .register(AgentCard::new("c", "C", "...", vec![]))
        .await;

    let all = registry.list_all().await;
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn agent_card_has_capability_is_case_insensitive() {
    let card = AgentCard::new("a", "A", "...", vec!["Code-Gen".into()]);
    assert!(card.has_capability("code-gen"));
    assert!(card.has_capability("CODE-GEN"));
    assert!(!card.has_capability("research"));
}

#[tokio::test]
async fn agent_card_metadata() {
    let card = AgentCard::new("a", "A", "...", vec![])
        .with_metadata("cost", "0.01")
        .with_metadata("latency", "50ms");

    assert_eq!(card.metadata.get("cost").unwrap(), "0.01");
    assert_eq!(card.metadata.get("latency").unwrap(), "50ms");
}

#[tokio::test]
async fn first_match_selector_picks_first_online() {
    let selector = FirstMatchSelector;
    let candidates = vec![
        AgentCard::new("a", "A", "...", vec![]).with_status(AgentStatus::Offline),
        AgentCard::new("b", "B", "...", vec![]).with_status(AgentStatus::Online),
        AgentCard::new("c", "C", "...", vec![]).with_status(AgentStatus::Online),
    ];

    let selected = selector.select(&candidates, "some task").await;
    assert_eq!(selected, Some("b".to_string()));
}

#[tokio::test]
async fn first_match_selector_returns_none_when_all_offline() {
    let selector = FirstMatchSelector;
    let candidates = vec![
        AgentCard::new("a", "A", "...", vec![]).with_status(AgentStatus::Offline),
    ];

    let selected = selector.select(&candidates, "some task").await;
    assert!(selected.is_none());
}

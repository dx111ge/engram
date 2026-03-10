/// Severity scoring and ranking for detected black areas.

use crate::types::{BlackArea, BlackAreaKind};

/// Sort black areas by severity (descending) with tie-breaking by kind priority.
pub fn rank_gaps(gaps: &mut [BlackArea]) {
    gaps.sort_by(|a, b| {
        b.severity
            .partial_cmp(&a.severity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| kind_priority(&b.kind).cmp(&kind_priority(&a.kind)))
    });
}

/// Priority ranking for gap kinds (higher = more important for tie-breaking).
fn kind_priority(kind: &BlackAreaKind) -> u8 {
    match kind {
        BlackAreaKind::CoordinatedCluster => 6,
        BlackAreaKind::StructuralHole => 5,
        BlackAreaKind::AsymmetricCluster => 4,
        BlackAreaKind::TemporalGap => 3,
        BlackAreaKind::ConfidenceDesert => 2,
        BlackAreaKind::FrontierNode => 1,
    }
}

/// Filter gaps to only include those above a minimum severity.
pub fn filter_by_severity(gaps: Vec<BlackArea>, min_severity: f32) -> Vec<BlackArea> {
    gaps.into_iter().filter(|g| g.severity >= min_severity).collect()
}

/// Deduplicate gaps involving the same entities.
pub fn dedup_gaps(gaps: Vec<BlackArea>) -> Vec<BlackArea> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for gap in gaps {
        let mut key_entities = gap.entities.clone();
        key_entities.sort();
        let key = format!("{:?}:{:?}", gap.kind, key_entities);

        if seen.insert(key) {
            result.push(gap);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(kind: BlackAreaKind, severity: f32) -> BlackArea {
        BlackArea {
            kind,
            entities: vec!["test".into()],
            severity,
            suggested_queries: vec![],
            domain: None,
            detected_at: 0,
        }
    }

    #[test]
    fn ranks_by_severity() {
        let mut gaps = vec![
            gap(BlackAreaKind::FrontierNode, 0.3),
            gap(BlackAreaKind::StructuralHole, 0.9),
            gap(BlackAreaKind::TemporalGap, 0.6),
        ];

        rank_gaps(&mut gaps);

        assert_eq!(gaps[0].severity, 0.9);
        assert_eq!(gaps[1].severity, 0.6);
        assert_eq!(gaps[2].severity, 0.3);
    }

    #[test]
    fn filters_by_severity() {
        let gaps = vec![
            gap(BlackAreaKind::FrontierNode, 0.2),
            gap(BlackAreaKind::FrontierNode, 0.8),
        ];

        let filtered = filter_by_severity(gaps, 0.5);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].severity, 0.8);
    }

    #[test]
    fn dedup_removes_duplicates() {
        let gaps = vec![
            gap(BlackAreaKind::FrontierNode, 0.5),
            gap(BlackAreaKind::FrontierNode, 0.3), // same entity, same kind
        ];

        let deduped = dedup_gaps(gaps);
        assert_eq!(deduped.len(), 1);
    }
}

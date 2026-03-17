use super::*;

impl Graph {
    /// Get stats: (node_count, edge_count)
    pub fn stats(&self) -> (u64, u64) {
        self.brain.stats()
    }

    /// Reset the graph: recreate the .brain file and clear all in-memory indexes.
    /// Preserves: embedder, compute_planner, event_bus, confidence_thresholds.
    pub fn reset(&mut self) -> Result<()> {
        let path = self.brain.path().to_path_buf();
        // Reset the brain file header (zero counts, truncate WAL)
        self.brain.reset()?;
        // Clear all in-memory indexes
        self.label_index = HashIndex::new();
        self.adj_out = HashMap::new();
        self.adj_in = HashMap::new();
        self.type_registry = TypeRegistry::new(&path);
        self.props = PropertyStore::new(&path);
        self.fulltext = FullTextIndex::new();
        self.temporal = TemporalIndex::new();
        self.type_bitmap = BitmapIndex::new();
        self.tier_bitmap = BitmapIndex::new();
        self.sensitivity_bitmap = BitmapIndex::new();
        self.node_type_names = Vec::new();
        self.node_type_lookup = HashMap::new();
        self.hnsw = HnswIndex::new(&path);
        self.cooccurrence = CooccurrenceTracker::new(&path);
        self.source_types = HashMap::new();
        Ok(())
    }

    /// Deduplicate edges: for each (from, to, edge_type) triplet,
    /// keep the highest-confidence edge and soft-delete the rest.
    /// Returns the number of duplicate edges removed.
    pub fn dedup_edges(&mut self) -> u32 {
        let (_, edge_count) = self.brain.stats();
        if edge_count == 0 {
            return 0;
        }

        // Collect all active edges grouped by (from, to, edge_type)
        let mut groups: HashMap<(u64, u64, u32), Vec<(u64, f32)>> = HashMap::new();
        for slot in 0..edge_count {
            if let Ok(edge) = self.brain.read_edge(slot) {
                if edge.is_active() {
                    groups
                        .entry((edge.from_node, edge.to_node, edge.edge_type))
                        .or_default()
                        .push((slot, edge.confidence));
                }
            }
        }

        let prov = Provenance {
            source_type: SourceType::Derived,
            source_id: "dedup-edges".to_string(),
        };

        let mut removed = 0u32;
        for (_, mut slots) in groups {
            if slots.len() <= 1 {
                continue;
            }
            // Sort by confidence desc -- keep highest
            slots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            // Delete all but the first (highest confidence)
            for &(slot, _) in &slots[1..] {
                if self.delete_edge_by_slot(slot, &prov).unwrap_or(false) {
                    removed += 1;
                }
            }
        }

        removed
    }

    /// Flush and checkpoint everything: mmap, WAL, types, properties, vectors, co-occurrence.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.brain.checkpoint()?;
        self.type_registry.flush()?;
        self.props.flush()?;
        self.edge_props.flush()?;
        self.hnsw.flush().map_err(|e| StorageError::InvalidFile {
            reason: format!("vector flush failed: {e}"),
        })?;
        self.cooccurrence.flush().map_err(|e| StorageError::InvalidFile {
            reason: format!("co-occurrence flush failed: {e}"),
        })?;
        self.flush_node_type_names()?;
        Ok(())
    }
}

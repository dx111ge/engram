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

    /// Flush and checkpoint everything: mmap, WAL, types, properties, vectors, co-occurrence.
    pub fn checkpoint(&mut self) -> Result<()> {
        self.brain.checkpoint()?;
        self.type_registry.flush()?;
        self.props.flush()?;
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

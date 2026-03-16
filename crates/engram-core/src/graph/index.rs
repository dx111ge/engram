use super::*;

impl Graph {
    /// Store an embedding vector for a node slot directly.
    pub fn store_vector(&mut self, label: &str, vector: Vec<f32>) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        self.hnsw.insert(slot, vector);
        Ok(true)
    }

    /// Embed a node's text using the configured embedder and store the vector.
    pub fn embed_node(&mut self, label: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };

        let embedder = match &self.embedder {
            Some(e) => e,
            None => return Err(StorageError::InvalidFile {
                reason: "no embedder configured".into(),
            }),
        };

        // Build text: label + all property values
        let mut text = self.full_label(slot)?;
        if let Some(props) = self.props.get_all(slot) {
            for (k, v) in props {
                text.push(' ');
                text.push_str(k);
                text.push(' ');
                text.push_str(v);
            }
        }

        let vector = embedder.embed(&text).map_err(|e| StorageError::InvalidFile {
            reason: e.to_string(),
        })?;
        self.hnsw.insert(slot, vector);
        Ok(true)
    }

    /// Re-embed all active nodes. Call after changing the embedder model.
    /// Returns the number of nodes re-embedded.
    pub fn reindex(&mut self) -> Result<u32> {
        let embedder = match &self.embedder {
            Some(e) => e,
            None => {
                return Err(StorageError::InvalidFile {
                    reason: "no embedder configured".into(),
                });
            }
        };

        // Clear the HNSW index
        self.hnsw = HnswIndex::new(self.brain.path());

        let (node_count, _) = self.brain.stats();
        let mut count = 0u32;

        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if !node.is_active() {
                continue;
            }

            let mut text = self.full_label(slot)?;
            if let Some(props) = self.props.get_all(slot) {
                for (k, v) in props {
                    text.push(' ');
                    text.push_str(k);
                    text.push(' ');
                    text.push_str(v);
                }
            }

            match embedder.embed(&text) {
                Ok(vector) => {
                    self.hnsw.insert(slot, vector);
                    count += 1;
                }
                Err(e) => {
                    tracing::warn!("failed to embed node {}: {e}", node.label());
                }
            }
        }

        Ok(count)
    }
}

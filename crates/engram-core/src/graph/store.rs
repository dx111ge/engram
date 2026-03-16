use super::*;

impl Graph {
    /// Store a new node with a label. Returns node ID.
    /// Confidence is automatically set based on the provenance source type.
    /// Labels longer than 47 bytes are stored in the property region
    /// with the inline buffer holding a prefix for display.
    pub fn store(&mut self, label: &str, provenance: &Provenance) -> Result<u64> {
        // Dedup: if a node with this label already exists, return its ID
        if let Some(existing_id) = self.find_node_id(label)? {
            return Ok(existing_id);
        }

        let node_id = self.brain.store_node(label)?;
        let (node_count, _) = self.brain.stats();
        let slot = node_count - 1;

        // Store overflow label in property region if needed
        let node = self.brain.read_node(slot)?;
        if node.has_label_overflow() {
            self.props.set(slot, LABEL_OVERFLOW_KEY, label);
        }

        // Set source-based initial confidence and source_id
        let init_conf = initial_confidence(provenance.source_type);
        self.brain.update_node_field(slot, |n| {
            n.source_id = provenance.to_source_hash();
            n.confidence = init_conf;
        })?;
        self.source_types.insert(slot, provenance.source_type);

        self.label_index.insert(hash_label(label), slot);

        // Update search indexes (use full label, not truncated inline)
        self.fulltext.add_document(slot, label);
        let node = self.brain.read_node(slot)?;
        self.temporal.insert(slot, node.created_at, node.event_time);
        self.type_bitmap.insert(node.node_type, slot);
        self.tier_bitmap.insert(node.memory_tier as u32, slot);
        self.sensitivity_bitmap.insert(node.sensitivity as u32, slot);

        // Auto-embed if an embedder is configured (use full label)
        if let Some(ref embedder) = self.embedder {
            if let Ok(vec) = embedder.embed(label) {
                self.hnsw.insert(slot, vec);
            }
        }

        self.emit(GraphEvent::FactStored {
            node_id,
            label: Arc::from(label),
            confidence: init_conf,
            source: Arc::from(provenance.source_id.as_str()),
            entity_type: None,
        });

        Ok(node_id)
    }

    /// Store a node with explicit confidence.
    pub fn store_with_confidence(
        &mut self,
        label: &str,
        confidence: f32,
        provenance: &Provenance,
    ) -> Result<u64> {
        let node_id = self.store(label, provenance)?;
        if let Some(slot) = self.find_slot_by_label(label)? {
            let old_conf = self.brain.read_node(slot)?.confidence;
            let clamped = confidence.clamp(0.0, 1.0);
            self.brain.update_node_field(slot, |n| {
                n.confidence = clamped;
            })?;
            if (old_conf - clamped).abs() > f32::EPSILON {
                self.emit(GraphEvent::FactUpdated {
                    node_id,
                    label: Arc::from(label),
                    old_confidence: old_conf,
                    new_confidence: clamped,
                });
                self.check_threshold_crossing(node_id, label, old_conf, clamped);
            }
        }
        Ok(node_id)
    }

    /// Create a relationship between two nodes. Returns edge ID.
    pub fn relate(
        &mut self,
        from_label: &str,
        to_label: &str,
        relationship: &str,
        provenance: &Provenance,
    ) -> Result<u64> {
        let from_node = self.find_node_id(from_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;
        let to_node = self.find_node_id(to_label)?
            .ok_or_else(|| StorageError::NodeNotFound { id: 0 })?;

        let edge_type = self.type_registry.get_or_create(relationship);
        let edge_id = self.brain.store_edge(from_node, to_node, edge_type)?;

        let (_, edge_count) = self.brain.stats();
        let edge_slot = edge_count - 1;

        // Update edge source_id
        self.brain
            .update_edge_field(edge_slot, |e| e.source_id = provenance.to_source_hash())?;

        self.adj_out.entry(from_node).or_default().push(edge_slot);
        self.adj_in.entry(to_node).or_default().push(edge_slot);

        // Increment edge counters on both nodes
        if let Some(from_slot) = self.find_slot_by_id(from_node) {
            self.brain.update_node_field(from_slot, |n| {
                n.edge_out_count += 1;
            })?;
        }
        if let Some(to_slot) = self.find_slot_by_id(to_node) {
            self.brain.update_node_field(to_slot, |n| {
                n.edge_in_count += 1;
            })?;
        }

        let edge = self.brain.read_edge(edge_slot)?;
        self.emit(GraphEvent::EdgeCreated {
            edge_id,
            from: from_node,
            to: to_node,
            rel_type: Arc::from(relationship),
            confidence: edge.confidence,
        });

        Ok(edge_id)
    }

    /// Create a relationship with explicit confidence.
    pub fn relate_with_confidence(
        &mut self,
        from_label: &str,
        to_label: &str,
        relationship: &str,
        confidence: f32,
        provenance: &Provenance,
    ) -> Result<u64> {
        let edge_id = self.relate(from_label, to_label, relationship, provenance)?;
        let (_, edge_count) = self.brain.stats();
        let edge_slot = edge_count - 1;
        self.brain.update_edge_field(edge_slot, |e| {
            e.confidence = confidence.clamp(0.0, 1.0);
        })?;
        Ok(edge_id)
    }

    /// Create a relationship with explicit confidence and temporal bounds.
    /// `valid_from` and `valid_to` are date strings like "2000-05-07" (parsed to unix seconds).
    pub fn relate_with_temporal(
        &mut self,
        from_label: &str,
        to_label: &str,
        relationship: &str,
        confidence: f32,
        valid_from: Option<&str>,
        valid_to: Option<&str>,
        provenance: &Provenance,
    ) -> Result<u64> {
        let edge_id = self.relate(from_label, to_label, relationship, provenance)?;
        let (_, edge_count) = self.brain.stats();
        let edge_slot = edge_count - 1;
        self.brain.update_edge_field(edge_slot, |e| {
            e.confidence = confidence.clamp(0.0, 1.0);
            if let Some(vf) = valid_from {
                e.valid_from = parse_date_to_unix(vf);
            }
            if let Some(vt) = valid_to {
                e.valid_to = parse_date_to_unix(vt);
            }
        })?;
        Ok(edge_id)
    }

    /// Set a property on an edge (by slot).
    pub fn set_edge_property(&mut self, edge_slot: u64, key: &str, value: &str) {
        self.edge_props.set(edge_slot, key, value);
    }

    /// Get a property from an edge (by slot).
    pub fn get_edge_property(&self, edge_slot: u64, key: &str) -> Option<String> {
        self.edge_props.get(edge_slot, key).map(|s| s.to_string())
    }

    /// Get all properties for an edge (by slot).
    pub fn get_edge_properties(&self, edge_slot: u64) -> Option<std::collections::HashMap<String, String>> {
        self.edge_props.get_all(edge_slot).cloned()
    }

    /// Soft-delete a node by label. Sets confidence to 0, marks as deleted.
    pub fn delete(&mut self, label: &str, provenance: &Provenance) -> Result<bool> {
        let target_hash = hash_label(label);
        let slots = self.label_index.get(target_hash).to_vec();

        for slot in slots {
            let active = {
                let node = self.brain.read_node(slot)?;
                node.is_active()
            };
            if active && self.slot_label_eq(slot, label)? {
                let nid = self.brain.read_node(slot)?.id;
                let now = current_timestamp();
                self.brain.update_node_field(slot, |n| {
                    n.soft_delete(now);
                    n.source_id = provenance.to_source_hash();
                })?;
                self.label_index.remove(target_hash, slot);
                self.props.remove_all(slot);
                self.fulltext.remove_document(slot);
                self.temporal.remove(slot);
                self.hnsw.remove(slot);
                self.emit(GraphEvent::FactDeleted {
                    node_id: nid,
                    label: Arc::from(label),
                    source: Arc::from(provenance.source_id.as_str()),
                });
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Soft-delete an edge by from_label, to_label, and relationship type.
    /// Deletes the first matching active edge. Returns true if an edge was deleted.
    pub fn delete_edge(
        &mut self,
        from_label: &str,
        to_label: &str,
        rel_type: &str,
        provenance: &Provenance,
    ) -> Result<bool> {
        let from_id = match self.find_node_id(from_label)? {
            Some(id) => id,
            None => return Ok(false),
        };
        let to_id = match self.find_node_id(to_label)? {
            Some(id) => id,
            None => return Ok(false),
        };

        let edge_type = match self.type_registry.get(rel_type) {
            Some(t) => t,
            None => return Ok(false),
        };

        // Find matching edge slot in adj_out
        let slot = if let Some(edge_slots) = self.adj_out.get(&from_id) {
            let mut found = None;
            for &s in edge_slots {
                let edge = self.brain.read_edge(s)?;
                if edge.is_active()
                    && edge.from_node == from_id
                    && edge.to_node == to_id
                    && edge.edge_type == edge_type
                {
                    found = Some(s);
                    break;
                }
            }
            found
        } else {
            None
        };

        match slot {
            Some(s) => self.delete_edge_by_slot(s, provenance),
            None => Ok(false),
        }
    }

    /// Find an existing active edge between two nodes with a given relation.
    /// Returns the edge slot if found, None otherwise.
    pub fn find_edge_slot(
        &self,
        from_label: &str,
        to_label: &str,
        rel_type: &str,
    ) -> Result<Option<u64>> {
        let from_id = match self.find_node_id(from_label)? {
            Some(id) => id,
            None => return Ok(None),
        };
        let to_id = match self.find_node_id(to_label)? {
            Some(id) => id,
            None => return Ok(None),
        };
        let edge_type = match self.type_registry.get(rel_type) {
            Some(t) => t,
            None => return Ok(None),
        };

        if let Some(edge_slots) = self.adj_out.get(&from_id) {
            for &s in edge_slots {
                let edge = self.brain.read_edge(s)?;
                if edge.is_active()
                    && edge.from_node == from_id
                    && edge.to_node == to_id
                    && edge.edge_type == edge_type
                {
                    return Ok(Some(s));
                }
            }
        }
        Ok(None)
    }

    /// Update the confidence of an edge by slot index.
    pub fn update_edge_confidence(&mut self, slot: u64, confidence: f32) -> Result<()> {
        self.brain.update_edge_field(slot, |e| {
            e.confidence = confidence.clamp(0.0, 1.0);
        })
    }

    /// Get the confidence of an edge by slot index.
    pub fn edge_confidence(&self, slot: u64) -> Result<f32> {
        let edge = self.brain.read_edge(slot)?;
        Ok(edge.confidence)
    }

    /// Get the confidence of a node by label.
    pub fn node_confidence(&self, label: &str) -> Result<Option<f32>> {
        if let Some(slot) = self.find_slot_by_label(label)? {
            let node = self.brain.read_node(slot)?;
            Ok(Some(node.confidence))
        } else {
            Ok(None)
        }
    }

    /// Set the confidence of a node by label. Returns the old confidence.
    pub fn set_node_confidence(&mut self, label: &str, confidence: f32) -> Result<Option<f32>> {
        if let Some(slot) = self.find_slot_by_label(label)? {
            let old_conf = self.brain.read_node(slot)?.confidence;
            let clamped = confidence.clamp(0.0, 1.0);
            self.brain.update_node_field(slot, |n| {
                n.confidence = clamped;
            })?;
            if let Some(node_id) = self.find_node_id(label)? {
                if (old_conf - clamped).abs() > f32::EPSILON {
                    self.emit(GraphEvent::FactUpdated {
                        node_id,
                        label: Arc::from(label),
                        old_confidence: old_conf,
                        new_confidence: clamped,
                    });
                    self.check_threshold_crossing(node_id, label, old_conf, clamped);
                }
            }
            Ok(Some(old_conf))
        } else {
            Ok(None)
        }
    }

    /// Soft-delete an edge by its slot index. Returns true if the edge was deleted.
    pub fn delete_edge_by_slot(&mut self, slot: u64, _provenance: &Provenance) -> Result<bool> {
        let edge = self.brain.read_edge(slot)?;
        if edge.is_deleted() {
            return Ok(false);
        }

        let edge_id = edge.id;
        let from_id = edge.from_node;
        let to_id = edge.to_node;
        let rel_name = self.type_registry.name_or_default(edge.edge_type);

        self.brain.delete_edge(slot)?;

        // Remove from adjacency lists
        if let Some(slots) = self.adj_out.get_mut(&from_id) {
            slots.retain(|&s| s != slot);
        }
        if let Some(slots) = self.adj_in.get_mut(&to_id) {
            slots.retain(|&s| s != slot);
        }

        // Decrement edge counters on both nodes
        if let Some(from_slot) = self.find_slot_by_id(from_id) {
            self.brain.update_node_field(from_slot, |n| {
                n.edge_out_count = n.edge_out_count.saturating_sub(1);
            })?;
        }
        if let Some(to_slot) = self.find_slot_by_id(to_id) {
            self.brain.update_node_field(to_slot, |n| {
                n.edge_in_count = n.edge_in_count.saturating_sub(1);
            })?;
        }

        self.emit(GraphEvent::EdgeDeleted {
            edge_id,
            from: from_id,
            to: to_id,
            rel_type: Arc::from(rel_name.as_str()),
        });

        Ok(true)
    }

    // --- Property operations ---

    /// Set a property on a node.
    pub fn set_property(&mut self, label: &str, key: &str, value: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let nid = self.brain.read_node(slot)?.id;
        self.props.set(slot, key, value);
        // Re-index: rebuild fulltext for this slot with label + all prop values
        self.reindex_fulltext(slot)?;
        self.emit(GraphEvent::PropertyChanged {
            node_id: nid,
            label: Arc::from(label),
            key: Arc::from(key),
            value: Arc::from(value),
        });
        Ok(true)
    }

    /// Get a property value from a node.
    pub fn get_property(&self, label: &str, key: &str) -> Result<Option<String>> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(None),
        };
        Ok(self.props.get(slot, key).map(|s| s.to_string()))
    }

    /// Get all properties for a node.
    pub fn get_properties(&self, label: &str) -> Result<Option<HashMap<String, String>>> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(None),
        };
        Ok(self.props.get_all(slot).cloned())
    }

    /// Set a property with contradiction checking.
    /// Returns (success, contradictions). Contradictions are flagged but do NOT block the write.
    pub fn set_property_checked(
        &mut self,
        label: &str,
        key: &str,
        value: &str,
    ) -> Result<(bool, ConflictCheckResult)> {
        let conflicts = self.check_property_contradiction(label, key, value)?;
        let ok = self.set_property(label, key, value)?;
        Ok((ok, conflicts))
    }

    /// Set a node's type by name.
    pub fn set_node_type(&mut self, label: &str, type_name: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let old_node = self.brain.read_node(slot)?;
        let old_type = old_node.node_type;
        let new_type = self.get_or_create_node_type(type_name);

        self.type_bitmap.remove(old_type, slot);
        self.brain.update_node_field(slot, |n| n.node_type = new_type)?;
        self.type_bitmap.insert(new_type, slot);
        Ok(true)
    }

    /// Get a node's type name, if one has been set.
    pub fn get_node_type(&self, label: &str) -> Option<String> {
        let slot = self.find_slot_by_label(label).ok()??;
        let node = self.brain.read_node(slot).ok()?;
        self.node_type_names.get(node.node_type as usize).cloned()
    }

    /// Get the edge type name for an edge type ID.
    pub fn edge_type_name(&self, type_id: u32) -> String {
        self.type_registry.name_or_default(type_id)
    }

    /// Read an edge by slot and resolve labels/relationship into an EdgeView.
    pub fn read_edge_view(&self, edge_slot: u64) -> Result<EdgeView> {
        let edge = self.brain.read_edge(edge_slot)?;
        let from_label = self.label_for_id(edge.from_node)?;
        let to_label = self.label_for_id(edge.to_node)?;
        let rel_name = self.type_registry.name_or_default(edge.edge_type);
        Ok(EdgeView {
            from: from_label,
            to: to_label,
            relationship: rel_name,
            confidence: edge.confidence,
            valid_from: timestamp_to_date(edge.valid_from),
            valid_to: timestamp_to_date(edge.valid_to),
        })
    }

    /// Get all outgoing edges from a node.
    pub fn edges_from(&self, label: &str) -> Result<Vec<EdgeView>> {
        let node_id = match self.find_node_id(label)? {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let mut edges = Vec::new();
        if let Some(edge_slots) = self.adj_out.get(&node_id) {
            for &slot in edge_slots {
                let edge = self.brain.read_edge(slot)?;
                if edge.is_deleted() {
                    continue;
                }
                let target_label = self.label_for_id(edge.to_node)?;
                let rel_name = self.type_registry.name_or_default(edge.edge_type);
                edges.push(EdgeView {
                    from: label.to_string(),
                    to: target_label,
                    relationship: rel_name,
                    confidence: edge.confidence,
                    valid_from: timestamp_to_date(edge.valid_from),
                    valid_to: timestamp_to_date(edge.valid_to),
                });
            }
        }
        Ok(edges)
    }

    /// Get all incoming edges to a node.
    pub fn edges_to(&self, label: &str) -> Result<Vec<EdgeView>> {
        let node_id = match self.find_node_id(label)? {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let mut edges = Vec::new();
        if let Some(edge_slots) = self.adj_in.get(&node_id) {
            for &slot in edge_slots {
                let edge = self.brain.read_edge(slot)?;
                if edge.is_deleted() {
                    continue;
                }
                let source_label = self.label_for_id(edge.from_node)?;
                let rel_name = self.type_registry.name_or_default(edge.edge_type);
                edges.push(EdgeView {
                    from: source_label,
                    to: label.to_string(),
                    relationship: rel_name,
                    confidence: edge.confidence,
                    valid_from: timestamp_to_date(edge.valid_from),
                    valid_to: timestamp_to_date(edge.valid_to),
                });
            }
        }
        Ok(edges)
    }
}

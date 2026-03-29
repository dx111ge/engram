use super::*;

impl Graph {
    /// Reinforce a node -- called when a fact is accessed or used.
    /// Increments access_count, updates last_accessed, and boosts confidence.
    pub fn reinforce_access(&mut self, label: &str) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let (nid, old_conf) = {
            let node = self.brain.read_node(slot)?;
            (node.id, node.confidence)
        };
        let cap = self.source_cap(slot);
        let now = current_timestamp();
        self.brain.update_node_field(slot, |n| {
            n.access_count = n.access_count.saturating_add(1);
            n.last_accessed = now;
            n.confidence = reinforce::reinforce_access(n.confidence, cap);
            n.updated_at = now;
        })?;
        let new_conf = self.brain.read_node(slot)?.confidence;
        if (old_conf - new_conf).abs() > f32::EPSILON {
            self.emit(GraphEvent::FactUpdated {
                node_id: nid,
                label: Arc::from(label),
                old_confidence: old_conf,
                new_confidence: new_conf,
            });
            self.check_threshold_crossing(nid, label, old_conf, new_conf);
            // Cascade to facts if this is a Document/Source
            let _ = self.cascade_source_confidence(label);
        }
        Ok(true)
    }

    /// Confirm a node -- called when new evidence supports this fact.
    /// Larger confidence boost than access reinforcement.
    pub fn reinforce_confirm(&mut self, label: &str, provenance: &Provenance) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let (nid, old_conf) = {
            let node = self.brain.read_node(slot)?;
            (node.id, node.confidence)
        };
        let cap = self.source_cap(slot);
        let now = current_timestamp();
        self.brain.update_node_field(slot, |n| {
            n.access_count = n.access_count.saturating_add(1);
            n.last_accessed = now;
            n.confidence = reinforce::reinforce_confirm(n.confidence, cap);
            n.source_id = provenance.to_source_hash();
            n.updated_at = now;
        })?;
        let new_conf = self.brain.read_node(slot)?.confidence;
        if (old_conf - new_conf).abs() > f32::EPSILON {
            self.emit(GraphEvent::FactUpdated {
                node_id: nid,
                label: Arc::from(label),
                old_confidence: old_conf,
                new_confidence: new_conf,
            });
            self.check_threshold_crossing(nid, label, old_conf, new_conf);
            // Cascade to facts if this is a Document/Source
            let _ = self.cascade_source_confidence(label);
        }
        Ok(true)
    }

    /// Apply time-based decay to all active nodes.
    /// Returns the number of nodes whose confidence was reduced.
    pub fn apply_decay(&mut self) -> Result<u32> {
        let (node_count, _) = self.brain.stats();
        let now = current_timestamp();
        let mut decayed_count = 0u32;

        for slot in 0..node_count {
            let (active, nid, old_conf, last_acc) = {
                let node = self.brain.read_node(slot)?;
                (node.is_active(), node.id, node.confidence, node.last_accessed)
            };
            if !active || old_conf <= 0.0 {
                continue;
            }

            let new_conf = decay::apply_decay(old_conf, last_acc, now);
            if (new_conf - old_conf).abs() > f32::EPSILON {
                self.brain.update_node_field(slot, |n| {
                    n.confidence = new_conf;
                })?;
                decayed_count += 1;
                self.check_threshold_crossing(nid, "decay", old_conf, new_conf);
            }
        }

        self.emit(GraphEvent::DecayApplied {
            nodes_affected: decayed_count,
        });

        Ok(decayed_count)
    }

    /// Correct a fact -- mark it as wrong and propagate distrust to neighbors.
    /// The corrected node's confidence drops to 0. Connected nodes get penalized
    /// with damping (weaker with distance).
    pub fn correct(
        &mut self,
        label: &str,
        provenance: &Provenance,
        max_depth: u32,
    ) -> Result<Option<CorrectionResult>> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(None),
        };

        let now = current_timestamp();

        // Read old confidence for propagation base (copy values before mutable borrow)
        let node_ref = self.brain.read_node(slot)?;
        let base_penalty = node_ref.confidence;
        let node_id = node_ref.id;

        // Zero out the corrected node
        self.brain.update_node_field(slot, |n| {
            n.confidence = 0.0;
            n.source_id = provenance.to_source_hash();
            n.updated_at = now;
        })?;

        // BFS propagation of distrust
        let mut propagated = Vec::new();
        let mut visited: HashSet<u64> = HashSet::new();
        let mut queue: VecDeque<(u64, u32)> = VecDeque::new();
        visited.insert(node_id);

        // Seed with direct neighbors
        if let Some(edge_slots) = self.adj_out.get(&node_id) {
            for &edge_slot in edge_slots {
                let edge = self.brain.read_edge(edge_slot)?;
                if visited.insert(edge.to_node) {
                    queue.push_back((edge.to_node, 1));
                }
            }
        }

        while let Some((node_id, distance)) = queue.pop_front() {
            if distance > max_depth {
                continue;
            }

            if let Some(neighbor_slot) = self.find_slot_by_id(node_id) {
                let neighbor = self.brain.read_node(neighbor_slot)?;
                if !neighbor.is_active() {
                    continue;
                }

                let penalty = correction::propagated_penalty(base_penalty, distance);
                let old_conf = neighbor.confidence;
                let new_conf = (old_conf - penalty).max(0.0);

                if (new_conf - old_conf).abs() > f32::EPSILON {
                    self.brain.update_node_field(neighbor_slot, |n| {
                        n.confidence = new_conf;
                        n.updated_at = now;
                    })?;
                    propagated.push((neighbor_slot, old_conf, new_conf));
                }

                // Continue BFS
                if distance < max_depth {
                    if let Some(edge_slots) = self.adj_out.get(&node_id) {
                        for &edge_slot in edge_slots {
                            let edge = self.brain.read_edge(edge_slot)?;
                            if visited.insert(edge.to_node) {
                                queue.push_back((edge.to_node, distance + 1));
                            }
                        }
                    }
                }
            }
        }

        Ok(Some(CorrectionResult {
            corrected_slot: slot,
            propagated,
        }))
    }

    /// Record a co-occurrence between two labels.
    pub fn record_cooccurrence(&mut self, antecedent: &str, consequent: &str) {
        let now = current_timestamp();
        self.cooccurrence.record(antecedent, consequent, now);
    }

    /// Get co-occurrence stats for a pair.
    pub fn get_cooccurrence(&self, antecedent: &str, consequent: &str) -> Option<(u32, f32)> {
        self.cooccurrence
            .get(antecedent, consequent)
            .map(|s| (s.count, s.probability()))
    }

    /// Get all co-occurrences for an antecedent.
    pub fn cooccurrences_for(&self, antecedent: &str) -> Vec<(String, u32)> {
        self.cooccurrence
            .for_antecedent(antecedent)
            .into_iter()
            .map(|(cons, stats)| (cons.to_string(), stats.count))
            .collect()
    }

    // --- Contradiction detection ---

    /// Check for property contradictions when setting a property.
    /// Returns any contradictions found (does NOT prevent the write).
    pub fn check_property_contradiction(
        &self,
        label: &str,
        key: &str,
        new_value: &str,
    ) -> Result<ConflictCheckResult> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(ConflictCheckResult::none()),
        };

        if let Some(existing) = self.props.get(slot, key) {
            if contradiction::values_conflict(existing, new_value) {
                let c = Contradiction {
                    existing_slot: slot,
                    new_slot: slot,
                    reason: format!(
                        "property '{key}' conflict: existing='{existing}' vs new='{new_value}'"
                    ),
                    kind: ConflictKind::PropertyConflict,
                };
                return Ok(ConflictCheckResult::with(vec![c]));
            }
        }

        Ok(ConflictCheckResult::none())
    }

    // --- Evidence surfacing ---

    /// Search with evidence -- returns enriched results with co-occurrence stats,
    /// supporting facts, and contradictions.
    pub fn search_with_evidence(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<EnrichedResult>> {
        let results = self.search_text(query_str, limit)?;
        let mut enriched = Vec::with_capacity(results.len());

        for r in results {
            let evidence = self.gather_evidence(r.slot, &r.label)?;
            enriched.push(EnrichedResult {
                slot: r.slot,
                node_id: r.node_id,
                label: r.label,
                confidence: r.confidence,
                score: r.score,
                evidence,
            });
        }

        Ok(enriched)
    }

    /// Gather evidence for a specific node.
    fn gather_evidence(&self, slot: u64, label: &str) -> Result<Evidence> {
        let mut evidence = Evidence::empty();

        // Co-occurrence evidence
        let pairs = self.cooccurrence.for_antecedent(label);
        for (consequent, stats) in pairs {
            evidence.cooccurrences.push(CooccurrenceEvidence {
                antecedent: label.to_string(),
                consequent: consequent.to_string(),
                count: stats.count,
                probability: stats.probability(),
            });
        }

        // Supporting facts: nodes connected via outgoing edges
        let node = self.brain.read_node(slot)?;
        if let Some(edge_slots) = self.adj_out.get(&node.id) {
            for &edge_slot in edge_slots {
                let edge = self.brain.read_edge(edge_slot)?;
                if let Some(target_slot) = self.find_slot_by_id(edge.to_node) {
                    let target = self.brain.read_node(target_slot)?;
                    if target.is_active() {
                        let rel_name = self.type_registry.name_or_default(edge.edge_type);
                        evidence.supporting.push(SupportingFact {
                            slot: target_slot,
                            label: self.full_label(target_slot)?,
                            confidence: target.confidence,
                            relationship: rel_name,
                        });
                    }
                }
            }
        }

        // Property contradictions: check all properties for multi-valued conflicts
        if let Some(props) = self.props.get_all(slot) {
            for (key, value) in props {
                // Check if any other active node has the same property key with different value
                // and is connected to this node
                if let Some(edge_slots) = self.adj_out.get(&node.id) {
                    for &edge_slot in edge_slots {
                        let edge = self.brain.read_edge(edge_slot)?;
                        if let Some(target_slot) = self.find_slot_by_id(edge.to_node) {
                            if let Some(target_val) = self.props.get(target_slot, key) {
                                if target_val != value {
                                    let target = self.brain.read_node(target_slot)?;
                                    evidence.contradictions.push(ContradictingFact {
                                        slot: target_slot,
                                        label: self.full_label(target_slot)?,
                                        confidence: target.confidence,
                                        reason: format!(
                                            "property '{key}': '{value}' vs '{target_val}'"
                                        ),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(evidence)
    }

    // --- Tier management ---

    /// Run a tier sweep: evaluate all nodes and promote/demote based on stats.
    /// Returns a summary of changes.
    pub fn sweep_tiers(&mut self) -> Result<TierSweepResult> {
        let (node_count, _) = self.brain.stats();
        let now = current_timestamp();
        let mut result = TierSweepResult::default();

        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if !node.is_active() {
                continue;
            }

            result.evaluated += 1;
            let recommended = tier::recommended_tier(
                node.confidence,
                node.access_count,
                node.last_accessed,
                now,
                node.memory_tier,
            );

            if recommended != node.memory_tier {
                let old_tier = node.memory_tier;
                self.tier_bitmap.remove(old_tier as u32, slot);
                self.brain.update_node_field(slot, |n| {
                    n.memory_tier = recommended;
                    n.updated_at = now;
                })?;
                self.tier_bitmap.insert(recommended as u32, slot);

                if recommended == 0 {
                    result.promoted_to_core += 1;
                } else if recommended == 2 {
                    result.demoted_to_archival += 1;
                }
            }
        }

        self.emit(GraphEvent::TierSweepCompleted {
            promoted: result.promoted_to_core,
            demoted: result.demoted_to_archival,
            archived: result.demoted_to_archival,
        });

        Ok(result)
    }

    /// Manually set a node's tier (user override).
    pub fn set_tier(&mut self, label: &str, tier: u8) -> Result<bool> {
        let slot = match self.find_slot_by_label(label)? {
            Some(s) => s,
            None => return Ok(false),
        };
        let (nid, old_tier) = {
            let node = self.brain.read_node(slot)?;
            (node.id, node.memory_tier)
        };
        self.tier_bitmap.remove(old_tier as u32, slot);
        self.brain.update_node_field(slot, |n| {
            n.memory_tier = tier;
        })?;
        self.tier_bitmap.insert(tier as u32, slot);
        if old_tier != tier {
            self.emit(GraphEvent::TierChanged {
                node_id: nid,
                label: Arc::from(label),
                old_tier,
                new_tier: tier,
            });
        }
        Ok(true)
    }

    /// Cascade confidence changes from a Document/Source node to all facts extracted from it.
    /// fact_confidence = extraction_confidence * source_confidence.
    /// Skips facts where confidence_source is "human" (manually overridden).
    pub fn cascade_source_confidence(&mut self, source_label: &str) -> Result<u32> {
        // Check if this is a Document or Source node
        let node_type = self.get_node_type(source_label).unwrap_or_default().to_lowercase();
        if node_type != "document" && node_type != "source" {
            return Ok(0);
        }

        let source_conf = match self.get_node(source_label)? {
            Some(n) => n.confidence,
            None => return Ok(0),
        };

        // Find all facts linked via "extracted_from" (incoming edges to this document)
        let incoming = self.edges_to(source_label).unwrap_or_default();
        let fact_labels: Vec<String> = incoming.into_iter()
            .filter(|e| e.relationship == "extracted_from")
            .map(|e| e.from)
            .collect();

        let mut updated = 0u32;
        for fact_label in &fact_labels {
            // Skip human-overridden facts
            if let Ok(Some(props)) = self.get_properties(fact_label) {
                if props.get("confidence_source").map(|s| s.as_str()) == Some("human") {
                    continue;
                }
                // Get extraction_confidence
                let ext_conf: f32 = props.get("extraction_confidence")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.60);

                let new_conf = ext_conf * source_conf;
                if let Some(slot) = self.find_slot_by_label(fact_label)? {
                    let old_conf = self.brain.read_node(slot)?.confidence;
                    if (old_conf - new_conf).abs() > f32::EPSILON {
                        let nid = self.brain.read_node(slot)?.id;
                        self.brain.update_node_field(slot, |n| {
                            n.confidence = new_conf;
                            n.updated_at = current_timestamp();
                        })?;
                        self.emit(GraphEvent::FactUpdated {
                            node_id: nid,
                            label: Arc::from(fact_label.as_str()),
                            old_confidence: old_conf,
                            new_confidence: new_conf,
                        });
                        updated += 1;
                    }
                }
            }
        }
        Ok(updated)
    }
}

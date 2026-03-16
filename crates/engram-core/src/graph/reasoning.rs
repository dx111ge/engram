use super::*;

impl Graph {
    /// Forward chaining: evaluate rules against the graph and fire matching actions.
    /// Runs to fixed point -- repeats until no new facts are derived in a round.
    /// Returns a summary of what was inferred across all rounds.
    pub fn forward_chain(
        &mut self,
        rules: &[Rule],
        provenance: &Provenance,
    ) -> Result<InferenceResult> {
        let mut result = InferenceResult::default();
        let max_rounds = 10; // safety limit to prevent infinite loops

        for round in 0..max_rounds {
            let round_result = self.forward_chain_once(rules, provenance)?;
            let new_facts = round_result.edges_created + round_result.flags_raised;

            result.rules_evaluated += round_result.rules_evaluated;
            result.rules_fired += round_result.rules_fired;
            result.edges_created += round_result.edges_created;
            result.flags_raised += round_result.flags_raised;
            result.firings.extend(round_result.firings);

            if new_facts == 0 {
                break; // fixed point reached
            }

            if round == max_rounds - 1 {
                // Safety: avoid runaway rules
                break;
            }
        }

        Ok(result)
    }

    /// Single pass of forward chaining -- evaluate all rules once.
    fn forward_chain_once(
        &mut self,
        rules: &[Rule],
        provenance: &Provenance,
    ) -> Result<InferenceResult> {
        let mut result = InferenceResult::default();
        let derived_prov = Provenance {
            source_type: SourceType::Derived,
            source_id: provenance.source_id.clone(),
        };

        for rule in rules {
            result.rules_evaluated += 1;
            let matches = self.find_rule_matches(rule)?;

            for bindings in matches {
                result.rules_fired += 1;
                let mut actions_taken = Vec::new();

                for action in &rule.actions {
                    match action {
                        Action::CreateEdge {
                            from_var,
                            relationship,
                            to_var,
                            confidence_expr,
                        } => {
                            let from_label = match resolve_var(from_var, &bindings) {
                                Some(l) => l,
                                None => continue,
                            };
                            let to_label = match resolve_var(to_var, &bindings) {
                                Some(l) => l,
                                None => continue,
                            };

                            // Check if edge already exists
                            if self.edge_exists(&from_label, &to_label, relationship)? {
                                continue;
                            }

                            let conf = self.eval_confidence_expr(confidence_expr, &bindings)?;
                            self.relate(&from_label, &to_label, relationship, &derived_prov)?;
                            // Update edge confidence
                            let (_, edge_count) = self.brain.stats();
                            let edge_slot = edge_count - 1;
                            self.brain.update_edge_field(edge_slot, |e| {
                                e.confidence = conf;
                            })?;

                            actions_taken.push(format!(
                                "edge({from_label}, {relationship}, {to_label}, conf={conf:.2})"
                            ));
                            result.edges_created += 1;
                        }
                        Action::SetProperty {
                            node_var,
                            key,
                            value,
                        } => {
                            if let Some(label) = resolve_var(node_var, &bindings) {
                                // Skip if property already has this value
                                let existing = self.get_property(&label, key).ok().flatten();
                                if existing.as_deref() == Some(value.as_str()) {
                                    continue;
                                }
                                self.set_property(&label, key, value)?;
                                actions_taken.push(format!("prop({label}, {key}={value})"));
                            }
                        }
                        Action::Flag { node_var, reason } => {
                            if let Some(label) = resolve_var(node_var, &bindings) {
                                // Skip if already flagged with the same reason
                                let existing = self.get_property(&label, "_flag").ok().flatten();
                                if existing.as_deref() == Some(reason.as_str()) {
                                    continue;
                                }
                                self.set_property(&label, "_flag", reason)?;
                                actions_taken.push(format!("flag({label}: {reason})"));
                                result.flags_raised += 1;
                            }
                        }
                    }
                }

                result.firings.push(RuleFiring {
                    rule_name: rule.name.clone(),
                    bindings,
                    actions_taken,
                });
            }
        }

        Ok(result)
    }

    /// Backward chaining: try to prove a relationship exists between two nodes.
    /// Searches the graph for direct and transitive evidence.
    pub fn prove(
        &self,
        from_label: &str,
        to_label: &str,
        relationship: &str,
        max_depth: u32,
    ) -> Result<ProofResult> {
        // Direct edge check
        if self.edge_exists(from_label, to_label, relationship)? {
            let from_slot = self.find_slot_by_label(from_label)?;
            let conf = if let Some(slot) = from_slot {
                self.brain.read_node(slot)?.confidence
            } else {
                0.0
            };
            return Ok(ProofResult {
                supported: true,
                confidence: conf,
                chain: vec![ProofStep {
                    fact: format!("{from_label} -[{relationship}]-> {to_label}"),
                    confidence: conf,
                    evidence: vec!["direct edge".into()],
                    depth: 0,
                }],
            });
        }

        // Transitive search via BFS
        if max_depth > 0 {
            let from_id = match self.find_node_id(from_label)? {
                Some(id) => id,
                None => return Ok(ProofResult::unsupported()),
            };
            let to_id = match self.find_node_id(to_label)? {
                Some(id) => id,
                None => return Ok(ProofResult::unsupported()),
            };

            let mut visited: HashSet<u64> = HashSet::new();
            let mut queue: VecDeque<(u64, Vec<ProofStep>)> = VecDeque::new();
            visited.insert(from_id);
            queue.push_back((from_id, Vec::new()));

            while let Some((node_id, path)) = queue.pop_front() {
                if path.len() as u32 >= max_depth {
                    continue;
                }

                if let Some(edge_slots) = self.adj_out.get(&node_id) {
                    for &edge_slot in edge_slots {
                        let edge = self.brain.read_edge(edge_slot)?;
                        if edge.is_deleted() {
                            continue;
                        }
                        let edge_rel = self.type_registry.name_or_default(edge.edge_type);

                        if edge_rel != relationship {
                            continue;
                        }

                        let target = edge.to_node;
                        let target_label = self.label_for_id(target)?;
                        let mut new_path = path.clone();
                        let src_label = self.label_for_id(node_id)?;
                        new_path.push(ProofStep {
                            fact: format!("{src_label} -[{relationship}]-> {target_label}"),
                            confidence: edge.confidence,
                            evidence: vec![format!("edge slot {edge_slot}")],
                            depth: new_path.len() as u32,
                        });

                        if target == to_id {
                            // Found a transitive path!
                            let min_conf = new_path
                                .iter()
                                .map(|s| s.confidence)
                                .fold(f32::MAX, f32::min);
                            return Ok(ProofResult {
                                supported: true,
                                confidence: min_conf,
                                chain: new_path,
                            });
                        }

                        if visited.insert(target) {
                            queue.push_back((target, new_path));
                        }
                    }
                }
            }
        }

        Ok(ProofResult::unsupported())
    }

    // --- Inference helpers ---

    pub(crate) fn find_rule_matches(&self, rule: &Rule) -> Result<Vec<Bindings>> {
        // Start with edge conditions (most selective)
        let edge_conditions: Vec<&Condition> = rule
            .conditions
            .iter()
            .filter(|c| matches!(c, Condition::Edge { .. }))
            .collect();

        if edge_conditions.is_empty() {
            return self.match_node_conditions(rule);
        }

        // Recursively match edge conditions, building bindings as we go
        let initial = vec![Bindings::new()];
        let mut candidates = initial;

        for cond in &edge_conditions {
            if let Condition::Edge { from_var, relationship, to_var } = cond {
                let mut next_candidates = Vec::new();
                let (_, edge_count) = self.brain.stats();

                // Determine if from/to are quoted literals or variables.
                // Quoted literal: "Russia" means must match label "Russia" exactly.
                // Unquoted variable: Country means bind to whatever matches.
                let from_is_literal = from_var.starts_with('"') && from_var.ends_with('"');
                let to_is_literal = to_var.starts_with('"') && to_var.ends_with('"');
                let from_literal = if from_is_literal {
                    Some(from_var[1..from_var.len()-1].to_string())
                } else {
                    None
                };
                let to_literal = if to_is_literal {
                    Some(to_var[1..to_var.len()-1].to_string())
                } else {
                    None
                };

                for bindings in &candidates {
                    for edge_slot in 0..edge_count {
                        let edge = self.brain.read_edge(edge_slot)?;
                        if edge.is_deleted() {
                            continue;
                        }
                        let edge_rel = self.type_registry.name_or_default(edge.edge_type);
                        if edge_rel != *relationship {
                            continue;
                        }

                        let from_label = self.label_for_id(edge.from_node)?;
                        let to_label = self.label_for_id(edge.to_node)?;

                        // For literals: must match exactly
                        if let Some(ref lit) = from_literal {
                            if from_label != *lit {
                                continue;
                            }
                        }
                        if let Some(ref lit) = to_literal {
                            if to_label != *lit {
                                continue;
                            }
                        }

                        // For variables: check if already bound in current candidate
                        if !from_is_literal {
                            if let Some(bound) = bindings.get(from_var.as_str()) {
                                if *bound != from_label {
                                    continue;
                                }
                            }
                        }
                        if !to_is_literal {
                            if let Some(bound) = bindings.get(to_var.as_str()) {
                                if *bound != to_label {
                                    continue;
                                }
                            }
                        }

                        let mut new_bindings = bindings.clone();
                        // Only bind variables, not literals
                        if !from_is_literal {
                            new_bindings.insert(from_var.clone(), from_label);
                        }
                        if !to_is_literal {
                            new_bindings.insert(to_var.clone(), to_label);
                        }
                        next_candidates.push(new_bindings);
                    }
                }

                candidates = next_candidates;
            }
        }

        // Filter by non-edge conditions
        let non_edge: Vec<&Condition> = rule
            .conditions
            .iter()
            .filter(|c| !matches!(c, Condition::Edge { .. }))
            .collect();

        let mut results = Vec::new();
        for bindings in candidates {
            let mut all_match = true;
            for cond in &non_edge {
                if !self.check_condition(cond, &bindings)? {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                results.push(bindings);
            }
        }

        Ok(results)
    }

    fn match_node_conditions(&self, rule: &Rule) -> Result<Vec<Bindings>> {
        // For rules with only confidence/property conditions, scan all nodes
        let (node_count, _) = self.brain.stats();
        let mut results = Vec::new();

        for slot in 0..node_count {
            let node = self.brain.read_node(slot)?;
            if !node.is_active() {
                continue;
            }

            // Extract variable name from first condition
            let var_name = match &rule.conditions[0] {
                Condition::Confidence { var, .. } => var.clone(),
                Condition::Property { node_var, .. } => node_var.clone(),
                _ => continue,
            };

            let mut bindings = Bindings::new();
            bindings.insert(var_name, self.full_label(slot)?);

            let mut all_match = true;
            for cond in &rule.conditions {
                if !self.check_condition(cond, &bindings)? {
                    all_match = false;
                    break;
                }
            }

            if all_match {
                results.push(bindings);
            }
        }

        Ok(results)
    }

    pub(crate) fn check_condition(&self, cond: &Condition, bindings: &Bindings) -> Result<bool> {
        match cond {
            Condition::Edge {
                from_var,
                relationship,
                to_var,
            } => {
                // Resolve from: literal "Foo" -> "Foo", variable -> lookup in bindings
                let from_is_literal = from_var.starts_with('"') && from_var.ends_with('"');
                let from_label_owned;
                let from_label: &str = if from_is_literal {
                    from_label_owned = from_var[1..from_var.len()-1].to_string();
                    &from_label_owned
                } else {
                    match bindings.get(from_var.as_str()) {
                        Some(l) => l,
                        None => return Ok(false),
                    }
                };
                // Resolve to: literal "Foo" -> "Foo", variable -> lookup in bindings
                let to_is_literal = to_var.starts_with('"') && to_var.ends_with('"');
                let to_label_owned;
                let to_label: &str = if to_is_literal {
                    to_label_owned = to_var[1..to_var.len()-1].to_string();
                    &to_label_owned
                } else {
                    match bindings.get(to_var.as_str()) {
                        Some(l) => l,
                        None => {
                            // to_var not bound -- check if ANY edge of this type exists
                            return Ok(true); // optimistic -- full binding check happens later
                        }
                    }
                };
                self.edge_exists(from_label, to_label, relationship)
            }
            Condition::Property {
                node_var,
                key,
                value,
            } => {
                let label = match bindings.get(node_var.as_str()) {
                    Some(l) => l,
                    None => return Ok(false),
                };
                match self.get_property(label, key)? {
                    Some(v) => Ok(v == *value),
                    None => Ok(false),
                }
            }
            Condition::Confidence {
                var,
                op,
                threshold,
            } => {
                let label = match bindings.get(var.as_str()) {
                    Some(l) => l,
                    None => return Ok(false),
                };
                let node = match self.get_node(label)? {
                    Some(n) => n,
                    None => return Ok(false),
                };
                Ok(match op {
                    ConditionOp::Gt => node.confidence > *threshold,
                    ConditionOp::Gte => node.confidence >= *threshold,
                    ConditionOp::Lt => node.confidence < *threshold,
                    ConditionOp::Lte => node.confidence <= *threshold,
                })
            }
        }
    }

    pub(crate) fn edge_exists(&self, from_label: &str, to_label: &str, relationship: &str) -> Result<bool> {
        let from_id = match self.find_node_id(from_label)? {
            Some(id) => id,
            None => return Ok(false),
        };
        let to_id = match self.find_node_id(to_label)? {
            Some(id) => id,
            None => return Ok(false),
        };

        if let Some(edge_slots) = self.adj_out.get(&from_id) {
            for &edge_slot in edge_slots {
                let edge = self.brain.read_edge(edge_slot)?;
                if edge.is_deleted() {
                    continue;
                }
                if edge.to_node == to_id {
                    let rel = self.type_registry.name_or_default(edge.edge_type);
                    if rel == relationship {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    fn eval_confidence_expr(
        &self,
        expr: &ConfidenceExpr,
        bindings: &Bindings,
    ) -> Result<f32> {
        match expr {
            ConfidenceExpr::Literal(v) => Ok(*v),
            ConfidenceExpr::Min(a, b) => {
                let ca = self.binding_confidence(bindings, a)?;
                let cb = self.binding_confidence(bindings, b)?;
                Ok(ca.min(cb))
            }
            ConfidenceExpr::Product(a, b) => {
                let ca = self.binding_confidence(bindings, a)?;
                let cb = self.binding_confidence(bindings, b)?;
                Ok(ca * cb)
            }
        }
    }

    fn binding_confidence(&self, bindings: &Bindings, var: &str) -> Result<f32> {
        // Try to resolve the variable as a bound node label
        if let Some(label) = bindings.get(var) {
            if let Some(node) = self.get_node(label)? {
                return Ok(node.confidence);
            }
        }
        // Default
        Ok(0.5)
    }
}

use std::collections::HashMap;

/// Returns the default entity-type relation constraint templates.
///
/// Keys are `(head_type, tail_type)` pairs (lowercase), values are ordered
/// lists of plausible relation labels for that pair.
pub fn default_type_templates() -> HashMap<(String, String), Vec<String>> {
    let raw: Vec<(&str, &str, &[&str])> = vec![
        ("person", "organization", &["works_at", "leads", "founded", "member_of", "holds_position"]),
        ("person", "location", &["born_in", "lives_in", "citizen_of", "based_in"]),
        ("person", "person", &["married_to", "parent_of", "works_with", "succeeded_by"]),
        ("person", "event", &["participated_in", "organized"]),
        ("person", "product", &["created", "invented", "uses"]),
        ("organization", "location", &["headquartered_in", "operates_in", "located_in"]),
        ("organization", "organization", &["subsidiary_of", "partner_of", "acquired_by", "competes_with"]),
        ("organization", "product", &["produces", "sells", "developed"]),
        ("organization", "event", &["organized", "sponsored"]),
        ("product", "location", &["traded_in", "exported_to", "produced_in"]),
        ("product", "organization", &["manufactured_by", "developed_by", "sold_by"]),
        ("product", "product", &["component_of", "competes_with", "replaces"]),
        ("event", "location", &["occurred_in", "took_place_at"]),
        ("event", "event", &["preceded_by", "caused", "related_to"]),
        ("location", "location", &["part_of", "borders", "capital_of", "near"]),
    ];

    let mut map = HashMap::new();
    for (head, tail, rels) in raw {
        map.insert(
            (head.to_string(), tail.to_string()),
            rels.iter().map(|r| r.to_string()).collect(),
        );
    }
    map
}

/// Reverse-relation mapping for bidirectional lookup.
///
/// When `(B, A)` is queried but only `(A, B)` exists, the first relation from
/// `(A, B)` is transformed through this table. If no explicit reverse is
/// listed the original name is returned unchanged.
fn reverse_relation(rel: &str) -> String {
    match rel {
        "works_at" => "employs".to_string(),
        "leads" => "led_by".to_string(),
        "founded" => "founded_by".to_string(),
        "member_of" => "has_member".to_string(),
        "holds_position" => "position_held_by".to_string(),
        "born_in" => "birthplace_of".to_string(),
        "lives_in" => "residence_of".to_string(),
        "citizen_of" => "has_citizen".to_string(),
        "based_in" => "base_of".to_string(),
        "participated_in" => "had_participant".to_string(),
        "organized" => "organized_by".to_string(),
        "created" => "created_by".to_string(),
        "invented" => "invented_by".to_string(),
        "uses" => "used_by".to_string(),
        "headquartered_in" => "headquarters_of".to_string(),
        "operates_in" => "hosts_operations_of".to_string(),
        "located_in" => "location_of".to_string(),
        "subsidiary_of" => "parent_of".to_string(),
        "partner_of" => "partner_of".to_string(),
        "acquired_by" => "acquired".to_string(),
        "competes_with" => "competes_with".to_string(),
        "produces" => "produced_by".to_string(),
        "sells" => "sold_by".to_string(),
        "developed" => "developed_by".to_string(),
        "sponsored" => "sponsored_by".to_string(),
        "traded_in" => "trades".to_string(),
        "exported_to" => "imports_from".to_string(),
        "produced_in" => "produces_for".to_string(),
        "manufactured_by" => "manufactures".to_string(),
        "developed_by" => "developed".to_string(),
        "sold_by" => "sells".to_string(),
        "component_of" => "has_component".to_string(),
        "replaces" => "replaced_by".to_string(),
        "occurred_in" => "site_of".to_string(),
        "took_place_at" => "hosted".to_string(),
        "preceded_by" => "precedes".to_string(),
        "caused" => "caused_by".to_string(),
        "part_of" => "contains".to_string(),
        "borders" => "borders".to_string(),
        "capital_of" => "has_capital".to_string(),
        "near" => "near".to_string(),
        "married_to" => "married_to".to_string(),
        "parent_of" => "child_of".to_string(),
        "works_with" => "works_with".to_string(),
        "succeeded_by" => "succeeds".to_string(),
        other => other.to_string(),
    }
}

/// Infer the most likely relation label from entity types alone.
///
/// Returns the first default relation for `(head_type, tail_type)`.  If no
/// direct match exists the reversed pair `(tail_type, head_type)` is tried and
/// the relation name is reversed.  Falls back to `"related_to"`.
pub fn infer_from_types(head_type: &str, tail_type: &str) -> String {
    let templates = default_type_templates();
    let h = head_type.to_lowercase();
    let t = tail_type.to_lowercase();

    // Direct lookup
    if let Some(rels) = templates.get(&(h.clone(), t.clone())) {
        if let Some(first) = rels.first() {
            return first.clone();
        }
    }

    // Bidirectional: try reversed pair
    if let Some(rels) = templates.get(&(t, h)) {
        if let Some(first) = rels.first() {
            return reverse_relation(first);
        }
    }

    "related_to".to_string()
}

/// Infer the most likely relation label, with optional user-defined overrides.
///
/// `user_templates` maps `"head_type:tail_type"` (colon-separated, lowercase)
/// to a comma-separated list of relation labels.  User templates are checked
/// first (both direct and reversed) before falling back to the built-in
/// taxonomy.
pub fn infer_from_types_with_user(
    head_type: &str,
    tail_type: &str,
    user_templates: Option<&HashMap<String, String>>,
) -> String {
    let h = head_type.to_lowercase();
    let t = tail_type.to_lowercase();

    // Check user templates first
    if let Some(user) = user_templates {
        let direct_key = format!("{}:{}", h, t);
        if let Some(val) = user.get(&direct_key) {
            let first = val.split(',').next().unwrap_or("related_to").trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }

        // Bidirectional user lookup
        let reverse_key = format!("{}:{}", t, h);
        if let Some(val) = user.get(&reverse_key) {
            let first = val.split(',').next().unwrap_or("related_to").trim();
            if !first.is_empty() {
                return reverse_relation(first);
            }
        }
    }

    // Fall back to built-in taxonomy
    infer_from_types(head_type, tail_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_templates_person_org() {
        let result = infer_from_types("person", "organization");
        assert_eq!(result, "works_at");
    }

    #[test]
    fn test_default_templates_unknown_pair() {
        let result = infer_from_types("widget", "gadget");
        assert_eq!(result, "related_to");
    }

    #[test]
    fn test_templates_bidirectional() {
        // (organization, person) is not in the table directly, but
        // (person, organization) is -- the reverse should be returned.
        let result = infer_from_types("organization", "person");
        assert_eq!(result, "employs");
    }

    #[test]
    fn test_user_templates_override() {
        let mut user = HashMap::new();
        user.insert(
            "person:organization".to_string(),
            "employed_by,consults_for".to_string(),
        );

        let result =
            infer_from_types_with_user("person", "organization", Some(&user));
        assert_eq!(result, "employed_by");
    }
}

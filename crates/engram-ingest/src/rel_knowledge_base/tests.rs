#[cfg(test)]
mod tests {
    use crate::rel_knowledge_base::{sparql_escape, extract_qid, uri_to_label, extract_first_uri, extract_relations_from_sparql, wikidata_prop_to_rel_type};

    #[test]
    fn sparql_escape_handles_special_chars() {
        assert_eq!(sparql_escape(r#"foo"bar"#), r#"foo\"bar"#);
        assert_eq!(sparql_escape("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn extract_qid_from_uri() {
        assert_eq!(
            extract_qid("http://www.wikidata.org/entity/Q42"),
            "Q42"
        );
        assert_eq!(extract_qid("Q42"), "Q42");
    }

    #[test]
    fn uri_to_label_converts_camel_case() {
        assert_eq!(
            uri_to_label("http://www.wikidata.org/prop/direct/P31"),
            "p31"
        );
        assert_eq!(
            uri_to_label("http://schema.org/birthPlace"),
            "birth_place"
        );
    }

    #[test]
    fn extract_first_uri_from_sparql_json() {
        let json = serde_json::json!({
            "results": {
                "bindings": [{
                    "item": { "type": "uri", "value": "http://www.wikidata.org/entity/Q42" },
                    "itemLabel": { "type": "literal", "value": "Douglas Adams" }
                }]
            }
        });
        assert_eq!(
            extract_first_uri(&json),
            Some("http://www.wikidata.org/entity/Q42".to_string())
        );
    }

    #[test]
    fn extract_first_uri_empty_bindings() {
        let json = serde_json::json!({
            "results": { "bindings": [] }
        });
        assert_eq!(extract_first_uri(&json), None);
    }

    #[test]
    fn extract_relations_from_sparql_json() {
        let json = serde_json::json!({
            "results": {
                "bindings": [
                    {
                        "prop": { "type": "uri", "value": "http://www.wikidata.org/entity/P108" },
                        "propLabel": { "type": "literal", "value": "employer" }
                    },
                    {
                        "prop": { "type": "uri", "value": "http://www.wikidata.org/entity/P27" },
                        "propLabel": { "type": "literal", "value": "country of citizenship" }
                    }
                ]
            }
        });
        let rels = extract_relations_from_sparql(&json);
        assert_eq!(rels.len(), 2);
        assert_eq!(rels[0].1, "employer");
        assert_eq!(rels[1].1, "country of citizenship");
    }

    #[test]
    fn wikidata_prop_empty_returns_related_to() {
        assert_eq!(wikidata_prop_to_rel_type(""), "related_to");
    }

    #[test]
    fn wikidata_prop_known_labels() {
        assert_eq!(wikidata_prop_to_rel_type("position held"), "holds_position");
        assert_eq!(wikidata_prop_to_rel_type("country of citizenship"), "citizen_of");
        assert_eq!(wikidata_prop_to_rel_type("country"), "located_in");
    }

    #[test]
    fn wikidata_prop_uri_fallback() {
        assert_eq!(
            wikidata_prop_to_rel_type("http://www.wikidata.org/prop/direct/P39"),
            "holds_position"
        );
    }
}

/// Article topic classifier.
/// Classifies news articles into geopolitical topics based on title keywords.
/// Topics map to prediction categories for automatic evidence linking.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Classification {
    pub topics: Vec<String>,
    pub confidence: f32,
    pub tier: u8,
}

/// Keyword groups for topic classification.
/// Each topic has a set of trigger keywords -- if any appear in the title,
/// the article is classified under that topic.
struct TopicDef {
    name: &'static str,
    keywords: &'static [&'static str],
}

static TOPICS: &[TopicDef] = &[
    TopicDef { name: "sanctions", keywords: &[
        "sanction", "embargo", "freeze", "asset seizure", "swift", "export control",
        "price cap", "blacklist", "designat",
    ]},
    TopicDef { name: "military_buildup", keywords: &[
        "military buildup", "troop deploy", "deploy", "troops", "mobiliz", "conscript", "reservist",
        "military exercise", "wargame", "weapons stockpile",
    ]},
    TopicDef { name: "conflict", keywords: &[
        "offensive", "counterattack", "frontline", "battle", "casualt", "shelling",
        "strike", "bombard", "advance", "retreat", "liberat", "captur",
    ]},
    TopicDef { name: "ceasefire", keywords: &[
        "ceasefire", "cease-fire", "truce", "armistice", "peace deal",
        "cessation of hostilities",
    ]},
    TopicDef { name: "negotiations", keywords: &[
        "negotiat", "diplomat", "peace talk", "summit", "mediati", "framework",
        "peace plan", "roadmap",
    ]},
    TopicDef { name: "nuclear", keywords: &[
        "nuclear", "atomic", "warhead", "icbm", "tactical nuke", "nuclear posture",
        "escalation", "deterren",
    ]},
    TopicDef { name: "territorial", keywords: &[
        "annex", "occupation", "disputed territory", "sovereignty", "referendum",
        "border change", "territorial integrit",
    ]},
    TopicDef { name: "naval", keywords: &[
        "naval", "fleet", "warship", "submarine", "carrier", "maritime",
        "blockade", "strait", "sea lane",
    ]},
    TopicDef { name: "cyber", keywords: &[
        "cyber", "hack", "malware", "ransomware", "critical infrastructure",
        "gps jamming", "electronic warfare",
    ]},
    TopicDef { name: "hybrid_warfare", keywords: &[
        "hybrid warfare", "disinformation", "propaganda", "election interference",
        "influence operation", "proxy", "destabiliz",
    ]},
    TopicDef { name: "economic", keywords: &[
        "gdp", "inflation", "recession", "economic crisis", "currency",
        "trade deficit", "debt", "default",
    ]},
    TopicDef { name: "energy", keywords: &[
        "oil", "gas", "pipeline", "lng", "energy", "opec", "refiner",
        "fuel", "electricity",
    ]},
    TopicDef { name: "alliance", keywords: &[
        "nato", "alliance", "mutual defense", "security guarantee", "article 5",
        "collective defense", "membership",
    ]},
    TopicDef { name: "humanitarian", keywords: &[
        "refugee", "civilian casualties", "humanitarian", "war crime",
        "genocide", "ethnic cleansing", "icc", "tribunal",
    ]},
    TopicDef { name: "arms_transfer", keywords: &[
        "arms", "weapons", "ammunition", "military aid", "defense package",
        "f-16", "atacms", "himars", "patriot", "leopard",
    ]},
    TopicDef { name: "intelligence", keywords: &[
        "intelligence", "espionage", "spy", "surveillance", "intercept",
        "classified", "leak",
    ]},
    TopicDef { name: "regime_change", keywords: &[
        "regime change", "coup", "revolution", "uprising", "protest",
        "opposition", "succession", "power transfer", "rally", "rallies",
        "demonstration", "demonstrat", "unrest", "crackdown", "dissident",
        "reform", "riot", "civil unrest", "political crisis", "impeach",
        "ousted", "overthrow", "junta", "martial law", "emergency decree",
        "authoritarian", "repression", "suppress",
    ]},
    TopicDef { name: "trade", keywords: &[
        "trade", "import", "export", "tariff", "evasion", "circumvent",
        "parallel import", "shadow fleet",
    ]},
];

/// Classify an article by title. Returns matched topics.
pub fn classify(title: &str) -> Vec<String> {
    let lower = title.to_lowercase();
    let mut matched = Vec::new();

    for topic in TOPICS {
        for keyword in topic.keywords {
            if lower.contains(keyword) {
                matched.push(topic.name.to_string());
                break;
            }
        }
    }

    matched
}

/// Classify with full result including domain confidence.
pub fn classify_full(title: &str, domain: &str) -> Classification {
    let topics = classify(title);
    let (confidence, tier) = crate::tiers::domain_confidence(domain);

    Classification {
        topics,
        confidence,
        tier,
    }
}

/// Generate relationship type based on topic and prediction context.
/// Returns "supports" or "weakens" depending on the topic-prediction pair.
pub fn topic_prediction_relationship(topic: &str, prediction_category: &str) -> &'static str {
    // Default heuristic: conflict/buildup topics support military predictions
    // ceasefire/negotiation topics weaken conflict predictions
    match (topic, prediction_category) {
        ("ceasefire", "conflict") => "weakens",
        ("ceasefire", "military") => "weakens",
        ("ceasefire", "political") => "weakens",
        ("negotiations", "conflict") => "weakens",
        ("negotiations", "military") => "weakens",
        ("nuclear", "conflict") => "weakens",  // deterrence effect
        ("nuclear", "military") => "weakens",
        ("arms_transfer", "conflict") => "supports",  // enables offense
        ("regime_change", "political") => "supports",
        ("regime_change", "conflict") => "supports",
        ("humanitarian", _) => "supports",  // escalation signal
        _ => "supports",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_sanctions() {
        let topics = classify("EU approves new sanctions package against Russia");
        assert!(topics.contains(&"sanctions".to_string()));
    }

    #[test]
    fn classify_military() {
        let topics = classify("Russia deploys additional troops near Finnish border");
        assert!(topics.contains(&"military_buildup".to_string()));
    }

    #[test]
    fn classify_ceasefire() {
        let topics = classify("Trump proposes ceasefire framework for Ukraine");
        assert!(topics.contains(&"ceasefire".to_string()));
    }

    #[test]
    fn classify_multi_topic() {
        let topics = classify("NATO sanctions on nuclear arms transfers");
        assert!(topics.len() >= 2);
    }

    #[test]
    fn classify_unrelated() {
        let topics = classify("Local sports team wins championship");
        assert!(topics.is_empty());
    }
}

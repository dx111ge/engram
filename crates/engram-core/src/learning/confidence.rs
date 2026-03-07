/// Confidence model — source-based initial scoring and caps.
///
/// Each source type has an initial confidence and a maximum cap.
/// Reinforcement can never push confidence above the source cap.

use crate::graph::SourceType;

/// Initial confidence assigned to a new fact based on its source.
pub fn initial_confidence(source: SourceType) -> f32 {
    match source {
        SourceType::Sensor => 0.95,
        SourceType::Api => 0.90,
        SourceType::User => 0.80,
        SourceType::Derived => 0.50,
        SourceType::Llm => 0.30,
        SourceType::Correction => 0.90,
    }
}

/// Maximum confidence a fact from this source can ever reach.
pub fn confidence_cap(source: SourceType) -> f32 {
    match source {
        SourceType::Sensor => 0.99,
        SourceType::Api => 0.95,
        SourceType::User => 0.95,
        SourceType::Derived => 0.80,
        SourceType::Llm => 0.70,
        SourceType::Correction => 0.95,
    }
}

/// Resolve SourceType from the u32 stored in the _reserved region or node_type field.
pub fn source_type_from_u32(val: u32) -> SourceType {
    match val {
        0 => SourceType::User,
        1 => SourceType::Sensor,
        2 => SourceType::Llm,
        3 => SourceType::Api,
        4 => SourceType::Derived,
        5 => SourceType::Correction,
        _ => SourceType::User,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensor_highest_initial() {
        assert!(initial_confidence(SourceType::Sensor) > initial_confidence(SourceType::User));
        assert!(initial_confidence(SourceType::User) > initial_confidence(SourceType::Llm));
    }

    #[test]
    fn caps_above_initial() {
        for source in [SourceType::User, SourceType::Sensor, SourceType::Llm,
                        SourceType::Api, SourceType::Derived, SourceType::Correction] {
            assert!(confidence_cap(source) >= initial_confidence(source));
        }
    }

    #[test]
    fn roundtrip_source_type() {
        for val in 0..6u32 {
            let st = source_type_from_u32(val);
            assert_eq!(st as u32, val);
        }
    }
}

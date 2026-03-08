//! engram-inference: rule-based reasoning engine.
//!
//! Implements forward and backward chaining over the knowledge graph:
//! - Forward chaining -- apply rules to derive new facts, iterate to fixed point
//! - Backward chaining -- prove relationships by finding evidence chains
//! - Rule evaluation -- pattern matching on edges, properties, and confidence
//! - Contradiction detection -- identify conflicting facts in the graph

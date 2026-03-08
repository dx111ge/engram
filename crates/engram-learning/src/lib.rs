//! engram-learning: confidence lifecycle and knowledge evolution.
//!
//! Manages how knowledge confidence changes over time:
//! - Reinforcement -- boost confidence on access or independent confirmation
//! - Decay -- time-based confidence reduction for unused knowledge
//! - Correction -- zero confidence on contradiction, propagate distrust to neighbors
//! - Co-occurrence -- track entity co-occurrence for relationship suggestions
//! - Evidence accumulation -- passive statistics, never invents knowledge

/// Learning engine — confidence lifecycle, reinforcement, decay, correction.
///
/// Core principle: engram never invents knowledge. Learning is purely about
/// tracking how much to trust existing facts, not creating new ones.

pub mod cooccurrence;
pub mod confidence;
pub mod contradiction;
pub mod correction;
pub mod decay;
pub mod evidence;
pub mod inference;
pub mod reinforce;
pub mod rules;
pub mod tier;

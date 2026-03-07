/// Learning engine — confidence lifecycle, reinforcement, decay, correction.
///
/// Core principle: engram never invents knowledge. Learning is purely about
/// tracking how much to trust existing facts, not creating new ones.

pub mod cooccurrence;
pub mod confidence;
pub mod correction;
pub mod decay;
pub mod reinforce;

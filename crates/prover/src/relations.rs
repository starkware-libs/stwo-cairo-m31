use stwo_prover::relation;

pub const MEMORY_ID_SIZE: usize = 1;
pub const VALUE_SIZE: usize = 4;
pub const N_MEMORY_ELEMS: usize = MEMORY_ID_SIZE + VALUE_SIZE;
pub const STATE_SIZE: usize = 3;

relation!(MemoryRelation, N_MEMORY_ELEMS);
relation!(StateRelation, STATE_SIZE);

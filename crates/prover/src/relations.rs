use stwo_prover::relation;

pub const MEMORY_ID_SIZE: usize = 1;
pub const VALUE_SIZE: usize = 4;
const N_MEMORY_ELEMS: usize = MEMORY_ID_SIZE + VALUE_SIZE;

relation!(MemoryRelation, N_MEMORY_ELEMS);

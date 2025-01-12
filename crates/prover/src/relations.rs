use stwo_prover::relation;

pub const ADDR_SIZE: usize = 1;
pub const VALUE_SIZE: usize = 4;
pub const N_MEMORY_ELEMS: usize = ADDR_SIZE + VALUE_SIZE;
pub const STATE_SIZE: usize = 3;

relation!(MemoryRelation, N_MEMORY_ELEMS);
relation!(StateRelation, STATE_SIZE);

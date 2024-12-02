pub mod addr_to_f31;

pub const LOG_MEMORY_ADDRESS_BOUND: u32 = 20;
pub const MEMORY_ADDRESS_BOUND: usize = 1 << LOG_MEMORY_ADDRESS_BOUND;

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use stwo_prover::core::fields::m31::BaseField;

    use crate::components::memory::addr_to_f31;
    use crate::input::mem::{MemConfig, MemoryBuilder};
    use crate::input::vm_import::MemEntry;

    #[test]
    fn test_memory_trace_prover() {
        let memory = MemoryBuilder::from_iter(
            MemConfig::default(),
            (0..10).map(|i| MemEntry {
                addr: i,
                val: [i; 4],
            }),
        )
        .build();
        let mut claim_generator = addr_to_f31::ClaimGenerator::new(&memory);
        let address_usages = [0, 1, 1, 2, 2, 2]
            .into_iter()
            .map(BaseField::from)
            .collect_vec();
        let expected_f31_mult: [u32; 16] = [1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        for addr in address_usages {
            let value_index = memory.address_to_value_index[addr.0 as usize];
            claim_generator.add_inputs(value_index);
        }

        assert_eq!(claim_generator.multiplicities, expected_f31_mult);
    }
}

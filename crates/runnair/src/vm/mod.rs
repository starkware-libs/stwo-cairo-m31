pub mod add_ap;
pub mod assert;
pub mod call;
pub mod deref;
pub mod jmp;
pub mod jnz;
pub mod operand;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use num_traits::Zero;
use serde::Deserialize;
use serde_json;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use self::add_ap::*;
use self::assert::*;
use self::call::*;
use self::deref::*;
use self::jmp::*;
use self::jnz::*;
use crate::memory::relocatable::Relocatable;
use crate::memory::{MaybeRelocatableAddr, Memory};

#[derive(Clone, Copy, Debug)]
pub struct State {
    ap: MaybeRelocatableAddr,
    fp: MaybeRelocatableAddr,
    pc: MaybeRelocatableAddr,
}

impl State {
    pub fn advance(self) -> Self {
        Self {
            ap: self.ap,
            fp: self.fp,
            pc: self.pc + M31(1),
        }
    }

    pub fn advance_and_increment_ap(self) -> Self {
        Self {
            ap: self.ap + M31(1),
            fp: self.fp,
            pc: self.pc + M31(1),
        }
    }
}

pub(crate) type InstructionArgs = [M31; 3];

#[derive(Clone, Copy, Debug)]
pub struct Instruction {
    pub op: M31,
    pub args: InstructionArgs,
}

impl From<QM31> for Instruction {
    fn from(instruction: QM31) -> Self {
        let [op, args @ ..] = instruction.to_m31_array();
        Self { op, args }
    }
}

impl<T: Into<M31>> From<[T; 4]> for Instruction {
    fn from(instruction: [T; 4]) -> Self {
        let [op, args @ ..] = instruction;
        Self {
            op: op.into(),
            args: args.map(|x| x.into()),
        }
    }
}

// TODO: add hints.
#[derive(Debug, Deserialize)]
#[serde(try_from = "ProgramRaw")]
pub struct Program {
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Deserialize)]

struct ProgramRaw {
    data: Vec<[String; 4]>,
}

impl TryFrom<ProgramRaw> for Program {
    type Error = serde_json::Error;

    fn try_from(raw_program: ProgramRaw) -> Result<Self, Self::Error> {
        let instructions: Vec<_> = raw_program
            .data
            .into_iter()
            .map(|instruction| {
                let raw_instruction = instruction.map(|x| m31_from_hex_str(&x));
                Instruction::from(raw_instruction)
            })
            .collect();

        Ok(Self { instructions })
    }
}

impl Program {
    fn from_compiled_file(path: PathBuf) -> Self {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        let raw_program: ProgramRaw = serde_json::from_reader(reader).unwrap();
        Program::try_from(raw_program).unwrap()
    }
}

#[derive(Debug)]
pub struct VM {
    memory: Memory,
    state: State,
}

impl VM {
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn state(&self) -> &State {
        &self.state
    }
}

impl VM {
    const FINAL_FP: (isize, u32) = (3, 0);
    const FINAL_PC: (isize, u32) = (4, 0);

    pub(crate) fn create_for_main_execution(program: Program) -> Self {
        let program_segment = 0;
        let execution_segment = 1;
        let output_segment = 2;

        // Prepare memory.

        // Segment 0: program.
        let program_memory_segment =
            program
                .instructions
                .iter()
                .enumerate()
                .map(|(index, instruction)| {
                    let args = instruction.args;
                    let encoded_instruction =
                        QM31::from_m31_array([instruction.op, args[0], args[1], args[2]]);
                    let instruction_address =
                        Relocatable::from((program_segment, u32::try_from(index).unwrap()));

                    (instruction_address, encoded_instruction)
                });

        let mut memory = Memory::from_iter(program_memory_segment);

        // Segment 1: execution.
        let execution_memory_segment = [
            // Pointer to output cell.
            ((execution_segment, 0), (output_segment, 0)),
            // Final `fp`, `pc`; we never return from main.
            ((execution_segment, 1), Self::FINAL_FP),
            ((execution_segment, 2), Self::FINAL_PC),
        ]
        .map(|(address, value)| (Relocatable::from(address), Relocatable::from(value)));
        memory.extend(execution_memory_segment);

        // Segments 3, 4: write final `fp`, `pc`.
        let final_pointers = [
            (Self::FINAL_FP, QM31::zero()),
            (Self::FINAL_PC, QM31::zero()),
        ]
        .map(|(address, value)| (Relocatable::from(address), value));
        memory.extend(final_pointers);

        // Prepare state.

        let initial_stack = Relocatable::from((execution_segment, 3));
        let pc = Relocatable::from((program_segment, 0));
        let state = State {
            ap: initial_stack.into(),
            fp: initial_stack.into(),
            pc: pc.into(),
        };

        Self { memory, state }
    }
}

// Utils.

// TODO(alont): autogenerate this.
pub fn opcode_to_instruction(opcode: usize) -> fn(&mut Memory, State, InstructionArgs) -> State {
    match opcode {
        0 => addap_add_ap_ap,
        1 => addap_add_ap_fp,
        2 => addap_add_fp_ap,
        3 => addap_add_fp_fp,
        4 => addap_add_imm_ap,
        5 => addap_add_imm_fp,
        6 => addap_deref_ap,
        7 => addap_deref_fp,
        8 => addap_double_deref_ap,
        9 => addap_double_deref_fp,
        10 => addap_imm,
        11 => addap_mul_ap_ap,
        12 => addap_mul_ap_fp,
        13 => addap_mul_fp_ap,
        14 => addap_mul_fp_fp,
        15 => addap_mul_imm_ap,
        16 => addap_mul_imm_fp,
        17 => assert_ap_add_ap_ap,
        18 => assert_ap_add_ap_ap_appp,
        19 => assert_ap_add_ap_fp,
        20 => assert_ap_add_ap_fp_appp,
        21 => assert_ap_add_fp_ap,
        22 => assert_ap_add_fp_ap_appp,
        23 => assert_ap_add_fp_fp,
        24 => assert_ap_add_fp_fp_appp,
        25 => assert_ap_add_imm_ap,
        26 => assert_ap_add_imm_ap_appp,
        27 => assert_ap_add_imm_fp,
        28 => assert_ap_add_imm_fp_appp,
        29 => assert_ap_deref_ap,
        30 => assert_ap_deref_ap_appp,
        31 => assert_ap_deref_fp,
        32 => assert_ap_deref_fp_appp,
        33 => assert_ap_double_deref_ap,
        34 => assert_ap_double_deref_ap_appp,
        35 => assert_ap_double_deref_fp,
        36 => assert_ap_double_deref_fp_appp,
        37 => assert_ap_imm,
        38 => assert_ap_imm_appp,
        39 => assert_ap_mul_ap_ap,
        40 => assert_ap_mul_ap_ap_appp,
        41 => assert_ap_mul_ap_fp,
        42 => assert_ap_mul_ap_fp_appp,
        43 => assert_ap_mul_fp_ap,
        44 => assert_ap_mul_fp_ap_appp,
        45 => assert_ap_mul_fp_fp,
        46 => assert_ap_mul_fp_fp_appp,
        47 => assert_ap_mul_imm_ap,
        48 => assert_ap_mul_imm_ap_appp,
        49 => assert_ap_mul_imm_fp,
        50 => assert_ap_mul_imm_fp_appp,
        51 => assert_fp_add_ap_ap,
        52 => assert_fp_add_ap_ap_appp,
        53 => assert_fp_add_ap_fp,
        54 => assert_fp_add_ap_fp_appp,
        55 => assert_fp_add_fp_ap,
        56 => assert_fp_add_fp_ap_appp,
        57 => assert_fp_add_fp_fp,
        58 => assert_fp_add_fp_fp_appp,
        59 => assert_fp_add_imm_ap,
        60 => assert_fp_add_imm_ap_appp,
        61 => assert_fp_add_imm_fp,
        62 => assert_fp_add_imm_fp_appp,
        63 => assert_fp_deref_ap,
        64 => assert_fp_deref_ap_appp,
        65 => assert_fp_deref_fp,
        66 => assert_fp_deref_fp_appp,
        67 => assert_fp_double_deref_ap,
        68 => assert_fp_double_deref_ap_appp,
        69 => assert_fp_double_deref_fp,
        70 => assert_fp_double_deref_fp_appp,
        71 => assert_fp_imm,
        72 => assert_fp_imm_appp,
        73 => assert_fp_mul_ap_ap,
        74 => assert_fp_mul_ap_ap_appp,
        75 => assert_fp_mul_ap_fp,
        76 => assert_fp_mul_ap_fp_appp,
        77 => assert_fp_mul_fp_ap,
        78 => assert_fp_mul_fp_ap_appp,
        79 => assert_fp_mul_fp_fp,
        80 => assert_fp_mul_fp_fp_appp,
        81 => assert_fp_mul_imm_ap,
        82 => assert_fp_mul_imm_ap_appp,
        83 => assert_fp_mul_imm_fp,
        84 => assert_fp_mul_imm_fp_appp,
        85 => call_abs_ap,
        86 => call_abs_fp,
        87 => call_abs_imm,
        88 => call_rel_ap,
        89 => call_rel_fp,
        90 => call_rel_imm,
        91 => jmp_abs_add_ap_ap,
        92 => jmp_abs_add_ap_ap_appp,
        93 => jmp_abs_add_ap_fp,
        94 => jmp_abs_add_ap_fp_appp,
        95 => jmp_abs_add_fp_ap,
        96 => jmp_abs_add_fp_ap_appp,
        97 => jmp_abs_add_fp_fp,
        98 => jmp_abs_add_fp_fp_appp,
        99 => jmp_abs_add_imm_ap,
        100 => jmp_abs_add_imm_ap_appp,
        101 => jmp_abs_add_imm_fp,
        102 => jmp_abs_add_imm_fp_appp,
        103 => jmp_abs_deref_ap,
        104 => jmp_abs_deref_ap_appp,
        105 => jmp_abs_deref_fp,
        106 => jmp_abs_deref_fp_appp,
        107 => jmp_abs_double_deref_ap,
        108 => jmp_abs_double_deref_ap_appp,
        109 => jmp_abs_double_deref_fp,
        110 => jmp_abs_double_deref_fp_appp,
        111 => jmp_abs_imm,
        112 => jmp_abs_imm_appp,
        113 => jmp_abs_mul_ap_ap,
        114 => jmp_abs_mul_ap_ap_appp,
        115 => jmp_abs_mul_ap_fp,
        116 => jmp_abs_mul_ap_fp_appp,
        117 => jmp_abs_mul_fp_ap,
        118 => jmp_abs_mul_fp_ap_appp,
        119 => jmp_abs_mul_fp_fp,
        120 => jmp_abs_mul_fp_fp_appp,
        121 => jmp_abs_mul_imm_ap,
        122 => jmp_abs_mul_imm_ap_appp,
        123 => jmp_abs_mul_imm_fp,
        124 => jmp_abs_mul_imm_fp_appp,
        125 => jmp_rel_add_ap_ap,
        126 => jmp_rel_add_ap_ap_appp,
        127 => jmp_rel_add_ap_fp,
        128 => jmp_rel_add_ap_fp_appp,
        129 => jmp_rel_add_fp_ap,
        130 => jmp_rel_add_fp_ap_appp,
        131 => jmp_rel_add_fp_fp,
        132 => jmp_rel_add_fp_fp_appp,
        133 => jmp_rel_add_imm_ap,
        134 => jmp_rel_add_imm_ap_appp,
        135 => jmp_rel_add_imm_fp,
        136 => jmp_rel_add_imm_fp_appp,
        137 => jmp_rel_deref_ap,
        138 => jmp_rel_deref_ap_appp,
        139 => jmp_rel_deref_fp,
        140 => jmp_rel_deref_fp_appp,
        141 => jmp_rel_double_deref_ap,
        142 => jmp_rel_double_deref_ap_appp,
        143 => jmp_rel_double_deref_fp,
        144 => jmp_rel_double_deref_fp_appp,
        145 => jmp_rel_imm,
        146 => jmp_rel_imm_appp,
        147 => jmp_rel_mul_ap_ap,
        148 => jmp_rel_mul_ap_ap_appp,
        149 => jmp_rel_mul_ap_fp,
        150 => jmp_rel_mul_ap_fp_appp,
        151 => jmp_rel_mul_fp_ap,
        152 => jmp_rel_mul_fp_ap_appp,
        153 => jmp_rel_mul_fp_fp,
        154 => jmp_rel_mul_fp_fp_appp,
        155 => jmp_rel_mul_imm_ap,
        156 => jmp_rel_mul_imm_ap_appp,
        157 => jmp_rel_mul_imm_fp,
        158 => jmp_rel_mul_imm_fp_appp,
        159 => jnz_ap_ap,
        160 => jnz_ap_ap_appp,
        161 => jnz_ap_fp,
        162 => jnz_ap_fp_appp,
        163 => jnz_fp_ap,
        164 => jnz_fp_ap_appp,
        165 => jnz_fp_fp,
        166 => jnz_fp_fp_appp,
        167 => jnz_imm_ap,
        168 => jnz_imm_ap_appp,
        169 => jnz_imm_fp,
        170 => jnz_imm_fp_appp,
        171 => ret,
        _ => panic!("Unknown opcode: {}.", opcode),
    }
}

pub(crate) fn resolve_addresses<const N: usize>(
    state: State,
    bases: &[&str; N],
    offsets: &[M31; N],
) -> [MaybeRelocatableAddr; N] {
    assert!(
        bases.len() <= 3,
        "The number of bases and offsets should not exceed 3"
    );

    std::array::from_fn(|i| {
        let base = bases[i];
        let base_address = match base {
            "ap" => state.ap,
            "fp" => state.fp,
            _ => panic!("Invalid base: {}", base),
        };
        base_address + offsets[i]
    })
}

pub(crate) fn m31_from_hex_str(x: &str) -> M31 {
    M31(u32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap())
}

pub(crate) fn get_crate_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.to_path_buf()
}

pub(crate) fn get_tests_data_dir() -> PathBuf {
    get_crate_dir().join("tests").join("data")
}

pub(crate) fn run_fibonacci() {
    let program_path = get_tests_data_dir().join("fibonacci_compiled.json");
    let program = Program::from_compiled_file(program_path);
    VM::create_for_main_execution(program);
}

#[cfg(test)]
mod test {
    use crate::vm::run_fibonacci;

    #[test]
    fn test_runner() {
        run_fibonacci()
    }
}

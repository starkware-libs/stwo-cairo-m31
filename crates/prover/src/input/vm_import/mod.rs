mod json;

use std::io::Read;
use std::path::Path;

use bytemuck::{bytes_of_mut, Pod, Zeroable};
use cairo_vm::vm::trace::trace_entry::RelocatedTraceEntry;
use json::{PrivateInput, PublicInput};
use thiserror::Error;
use tracing::{span, Level};

use super::instructions::Instructions;
use super::mem::MemConfig;
use super::CairoInput;
use crate::input::mem::MemoryBuilder;

#[derive(Debug, Error)]
pub enum VmImportError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] sonic_rs::Error),
    #[error("No memory segments")]
    NoMemorySegments,
}

pub fn import_from_vm_output(
    pub_json: &Path,
    priv_json: &Path,
) -> Result<CairoInput, VmImportError> {
    let _span = span!(Level::INFO, "import_from_vm_output").entered();
    let pub_data: PublicInput = sonic_rs::from_str(&std::fs::read_to_string(pub_json)?)?;
    let priv_data: PrivateInput = sonic_rs::from_str(&std::fs::read_to_string(priv_json)?)?;

    let end_addr = pub_data
        .memory_segments
        .values()
        .map(|v| v.stop_ptr)
        .max()
        .ok_or(VmImportError::NoMemorySegments)?;
    assert!(end_addr < (1 << 32));
    let mem_config = MemConfig::new((1 << 20) - 1, end_addr as u32);

    let mem_path = priv_json.parent().unwrap().join(&priv_data.memory_path);
    let trace_path = priv_json.parent().unwrap().join(&priv_data.trace_path);

    let mut trace_file = std::io::BufReader::new(std::fs::File::open(trace_path)?);
    let mut mem_file = std::io::BufReader::new(std::fs::File::open(mem_path)?);
    let mut mem = MemoryBuilder::from_iter(mem_config, MemEntryIter(&mut mem_file));
    let instructions = Instructions::from_iter(TraceIter(&mut trace_file), &mut mem);

    let public_mem_addresses = pub_data
        .public_memory
        .iter()
        .map(|entry| entry.address as u32)
        .collect();

    Ok(CairoInput {
        instructions,
        memory: mem.build(),
        public_mem_addresses,
    })
}

/// A single entry from the trace file.
/// Note: This struct must be kept in sync with the Cairo VM's trace output file.
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct TraceEntry {
    pub ap: u64,
    pub fp: u64,
    pub pc: u64,
}

impl From<RelocatedTraceEntry> for TraceEntry {
    fn from(value: RelocatedTraceEntry) -> Self {
        Self {
            ap: value.ap as u64,
            fp: value.fp as u64,
            pc: value.pc as u64,
        }
    }
}

pub struct TraceIter<'a, R: Read>(pub &'a mut R);
impl<R: Read> Iterator for TraceIter<'_, R> {
    type Item = TraceEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let mut entry = TraceEntry::default();
        self.0
            .read_exact(bytes_of_mut(&mut entry))
            .ok()
            .map(|_| entry)
    }
}

/// A single entry from the memory file.
/// Note: This struct must be kept in sync with the Cairo VM's memory output file.
/// FIXME: this is shit
#[repr(C)]
#[derive(Copy, Clone, Default, Pod, Zeroable)]
pub struct MemEntry {
    pub addr: u32,
    pub val: [u32; 4],
}

pub struct MemEntryIter<'a, R: Read>(pub &'a mut R);
impl<R: Read> Iterator for MemEntryIter<'_, R> {
    type Item = MemEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let mut entry = MemEntry::default();
        self.0
            .read_exact(bytes_of_mut(&mut entry))
            .ok()
            .map(|_| entry)
    }
}

use binaryninja::{
    basic_block::BasicBlock,
    binary_view::BinaryView,
    command::Command,
    low_level_il::block::LowLevelILBlock,
    low_level_il::expression::{ExpressionHandler, LowLevelILExpressionKind},
    low_level_il::function::{Finalized, NonSSA},
    low_level_il::instruction::{InstructionHandler, LowLevelILInstructionKind},
    function::Function,
};
use log::debug;

use crate::{search_for_code_entries, CodeEntryDescription, CodeEntryDestRange};

pub struct ThemidaSpotterCommand;

impl Command for ThemidaSpotterCommand {
    fn action(&self, view: &BinaryView) {
        let mut target_sections: Vec<CodeEntryDestRange> = Vec::new();

        for section in view.sections().iter() {
            let name = section.name().to_string_lossy().into_owned();
            // Note: Themida/WinLicense 3.x only
            if name == ".boot" || name == ".themida" || name == ".winlice" || name == ".vlizer" {
                target_sections.push(section.start()..section.end());
            }
        }

        search_for_code_entries(view, search_for_themida_code_entries, target_sections)
    }

    fn valid(&self, _view: &BinaryView) -> bool {
        true
    }
}

fn search_for_themida_code_entries(
    bv: &BinaryView,
    func: &Function,
    themida_section_ranges: &[CodeEntryDestRange],
) -> Option<CodeEntryDescription> {
    debug!("Processing '{:?}'", func.symbol().full_name());

    // Check if we're in the correct section (i.e., out of Themida's sections)
    let func_addr = func.start();
    if themida_section_ranges
        .iter()
        .any(|r| r.contains(&func_addr))
    {
        return None;
    }

    let llil_func = func.low_level_il().ok()?;
    // Iterate over all basic blocks
    for llil_bb in llil_func.basic_blocks().iter() {
        // Check only the last instruction as we're looking for a JMP
        if let Some(llil_inst) = llil_bb.iter().last() {
            // Match `jmp imm` instruction
            if let LowLevelILInstructionKind::TailCall(op) = llil_inst.kind() {
                if let LowLevelILExpressionKind::ConstPtr(const_operation) = op.target().kind() {
                    let jmp_destination = const_operation.value();
                    // Check if jmp destination is inside of Themida's section
                    if themida_section_ranges
                        .iter()
                        .any(|r| r.contains(&jmp_destination))
                    {
                        // We're in an obfuscated code entry
                        // We now need to figure out whether it's mutated or virtualized
                        if destination_is_vmenter(bv, jmp_destination) {
                            debug!(
                                "Themida VMEnter detected at 0x{:x} ('{:?}')",
                                op.address(),
                                func.symbol().full_name(),
                            );
                            return Some(CodeEntryDescription::VMEnter(op.address()));
                        }

                        // Doesn't look virtualized, assume it's mutated
                        debug!(
                            "Themida MUTEnter detected at 0x{:x} ('{:?}')",
                            op.address(),
                            func.symbol().full_name(),
                        );
                        return Some(CodeEntryDescription::MUTEnter(llil_inst.address()));
                    }
                }
            }
        }
    }

    None
}

/// Return `true` if the given destination VA looks like a VMEnter routine.
/// Return `false` otherwise.
fn destination_is_vmenter(bv: &BinaryView, destination_addr: u64) -> bool {
    // Iterate over all potential functions
    for code_entry_func in bv.functions_at(destination_addr).into_iter() {
        if let Ok(llil_code_entry_func) = code_entry_func.low_level_il() {
            // Check if function looks like a VMEnter routine
            if function_is_vm_enter(llil_code_entry_func.as_ref()) {
                return true;
            }
        }
    }

    false
}

/// Return `true` if the given LLIL function looks like Themida's VMEnter routine.
///
/// This checks if the first instruction is `pushfd` and that the function exits
/// with a `jmp [reg]` instruction.
fn function_is_vm_enter(function: &binaryninja::low_level_il::LowLevelILRegularFunction) -> bool {
    if let Some(first_block) = function.basic_blocks().iter().next() {
        let first_block: &BasicBlock<LowLevelILBlock<'_, Finalized, NonSSA>> = first_block.as_ref();
        // Check if first block looks like the start of a VMEnter and one basic
        // block looks like the end of a VMEnter
        if block_is_vmenter_start(first_block)
            && function
                .basic_blocks()
                .iter()
                .any(|block| {
                    let block: &BasicBlock<LowLevelILBlock<'_, Finalized, NonSSA>> = block.as_ref();
                    block_is_vmenter_end(block)
                })
        {
            return true;
        }
    }

    false
}

/// Return `true` if the given basic block looks like the first basic block of
/// a VMEnter routine (i.e., starts with a `pushfd` instruction).
/// Return `false` otherwise.
fn block_is_vmenter_start(block: &BasicBlock<LowLevelILBlock<'_, Finalized, NonSSA>>) -> bool {
    if let Some(first_inst) = block.iter().next() {
        // Match a `pushfd` instruction (VMEnter)
        if instruction_is_pushfd(&first_inst) {
            return true;
        }
    }

    false
}

/// Return `true` if the given LLIL instruction corresponds to a `pushfd` instruction.
/// Return `false` otherwise.
fn instruction_is_pushfd(instruction: &binaryninja::low_level_il::LowLevelILRegularInstruction<'_>) -> bool {
    // LLIL instruction should be a push
    if let LowLevelILInstructionKind::Push(op) = instruction.kind() {
        // Operand should be a `or` (with many flags)
        if let LowLevelILExpressionKind::Or(_) = op.operand().kind() {
            return true;
        }
    }

    false
}

/// Return `true` if the given basic block looks like the final basic block of
/// a VMEnter routine (i.e., ends with a `jmp [reg]` instruction).
/// Return `false` otherwise.
fn block_is_vmenter_end(block: &BasicBlock<LowLevelILBlock<'_, Finalized, NonSSA>>) -> bool {
    // Check if last instruction is `jmp [rax/eax]`
    if let Some(last_ins) = block.iter().last() {
        if let LowLevelILInstructionKind::Jump(jmp_operation) = last_ins.kind() {
            if let LowLevelILExpressionKind::Load(load_operation) = jmp_operation.target().kind() {
                if let LowLevelILExpressionKind::Reg(_) = load_operation.source_expr().kind() {
                    return true;
                }
            }
        }
    }

    false
}

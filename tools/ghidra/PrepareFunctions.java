// Define explicitly selected function entries without whole-program analysis.
//@category wfcli

import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;

public class PrepareFunctions extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] entries = getScriptArgs();
        if (entries.length == 0 || entries.length > 1024) {
            throw new IllegalArgumentException("expected 1..1024 function entry addresses");
        }
        for (String value : entries) {
            monitor.checkCancelled();
            Address entry = toAddr(value);
            if (!currentProgram.getMemory().getExecuteSet().contains(entry)) {
                throw new IllegalArgumentException("entry is not executable: " + entry);
            }
            Function existing = getFunctionContaining(entry);
            if (existing != null && !existing.getEntryPoint().equals(entry)) {
                throw new IllegalArgumentException("entry is inside existing function: " + entry);
            }
            AddressSet starts = new AddressSet(entry);
            if (existing != null) {
                InstructionIterator instructions =
                    currentProgram.getListing().getInstructions(existing.getBody(), true);
                while (instructions.hasNext()) {
                    Instruction instruction = instructions.next();
                    if (instruction.getFallThrough() != null) {
                        starts.add(instruction.getFallThrough());
                    }
                    if (!instruction.getFlowType().isCall()) {
                        for (Address target : instruction.getFlows()) {
                            if (getFunctionAt(target) == null) {
                                starts.add(target);
                            }
                        }
                    }
                }
            }
            DisassembleCommand command = new DisassembleCommand(starts, null, true);
            if (!command.applyTo(currentProgram, monitor)) {
                throw new IllegalStateException("could not disassemble function at " + entry);
            }
            boolean defined = existing == null ? createFunction(entry, null) != null
                : CreateFunctionCmd.fixupFunctionBody(currentProgram, existing, monitor);
            if (!defined) {
                throw new IllegalStateException("could not define function at " + entry);
            }
        }
    }
}

// Export build-keyed reverse-engineering evidence for wfinspect.
//@category wfcli

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryBlock;
import ghidra.program.model.scalar.Scalar;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.ArrayDeque;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public class ExportWfinspectReport extends GhidraScript {
    private static final int SCHEMA_VERSION = 1;
    private static final int PRODUCER_VERSION = 1;
    private static final int MAX_RESULTS = 100_000;
    private static final int MAX_DECOMPILE_CHARS = 4 * 1024 * 1024;

    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) {
            throw new IllegalArgumentException(
                "expected output path followed by optional KIND=VALUE queries");
        }

        DecompInterface decompiler = new DecompInterface();
        decompiler.openProgram(currentProgram);
        List<Map<String, Object>> queries = new ArrayList<>();
        try {
            for (int index = 1; index < args.length; index++) {
                queries.add(runQuery(args[index], decompiler));
                monitor.checkCancelled();
            }
        } finally {
            decompiler.dispose();
        }

        Map<String, Object> report = new LinkedHashMap<>();
        report.put("schema", "wfinspect.ghidra-report");
        report.put("schema_version", SCHEMA_VERSION);
        report.put("producer", Map.of(
            "name", "ExportWfinspectReport",
            "version", PRODUCER_VERSION));
        report.put("program", programMetadata());
        report.put("queries", queries);

        Path output = Path.of(args[0]);
        if (output.getParent() != null) {
            Files.createDirectories(output.getParent());
        }
        Gson gson = new GsonBuilder().disableHtmlEscaping().setPrettyPrinting().create();
        Files.writeString(output, gson.toJson(report) + "\n", StandardCharsets.UTF_8);
        println("Exported " + queries.size() + " query result(s) to " + output);
    }

    private Map<String, Object> programMetadata() {
        Map<String, Object> program = new LinkedHashMap<>();
        program.put("name", currentProgram.getName());
        program.put("path", currentProgram.getExecutablePath());
        program.put("format", currentProgram.getExecutableFormat());
        program.put("sha256", currentProgram.getExecutableSHA256());
        program.put("image_base", currentProgram.getImageBase().toString());
        program.put("language", currentProgram.getLanguageID().toString());
        program.put("compiler", currentProgram.getCompilerSpec().getCompilerSpecID().toString());
        return program;
    }

    private Map<String, Object> runQuery(String argument, DecompInterface decompiler)
            throws Exception {
        int separator = argument.indexOf('=');
        if (separator <= 0 || separator == argument.length() - 1) {
            throw new IllegalArgumentException("invalid query: " + argument);
        }
        String kind = argument.substring(0, separator);
        String value = argument.substring(separator + 1);
        return switch (kind) {
            case "function" -> exportFunction(value, decompiler);
            case "graph" -> exportGraph(value);
            case "xrefs" -> exportXrefs(value);
            case "lea", "branches" -> exportRelativeReferences(kind, value);
            case "string" -> exportString(value);
            case "bytes" -> exportBytes("bytes", value, HexFormat.of().parseHex(value));
            case "scalar" -> exportScalar(value);
            case "pointers" -> exportPointers(value);
            default -> throw new IllegalArgumentException("unknown query kind: " + kind);
        };
    }

    private Map<String, Object> exportFunction(String value, DecompInterface decompiler) {
        Address requested = toAddr(value);
        Function function = getFunctionAt(requested);
        if (function == null) {
            function = getFunctionContaining(requested);
        }
        Map<String, Object> query = query("function", value);
        if (function == null) {
            query.put("found", false);
            return query;
        }

        query.put("found", true);
        query.put("entry", function.getEntryPoint().toString());
        query.put("name", function.getName());
        List<Map<String, String>> instructions = new ArrayList<>();
        InstructionIterator iterator =
            currentProgram.getListing().getInstructions(function.getBody(), true);
        while (iterator.hasNext() && instructions.size() < MAX_RESULTS && !monitor.isCancelled()) {
            Instruction instruction = iterator.next();
            instructions.add(Map.of(
                "address", instruction.getAddress().toString(),
                "text", instruction.toString()));
        }
        query.put("instructions", instructions);
        query.put("instructions_truncated", iterator.hasNext());

        DecompileResults result = decompiler.decompileFunction(function, 180, monitor);
        if (result.decompileCompleted()) {
            String source = result.getDecompiledFunction().getC();
            boolean truncated = source.length() > MAX_DECOMPILE_CHARS;
            query.put("decompilation", truncated ? source.substring(0, MAX_DECOMPILE_CHARS) : source);
            query.put("decompilation_truncated", truncated);
        } else {
            query.put("decompile_error", result.getErrorMessage());
        }
        return query;
    }

    private Map<String, Object> exportXrefs(String value) {
        Map<String, Object> query = query("xrefs", value);
        query.put("references", referencesTo(toAddr(value)));
        return query;
    }

    private Map<String, Object> exportRelativeReferences(String kind, String value) throws Exception {
        if (!currentProgram.getLanguageID().toString().startsWith("x86:LE:64:")) {
            throw new IllegalArgumentException("relative reference search requires x86-64");
        }
        boolean lea = kind.equals("lea");
        int length = lea ? 7 : 5;
        Address target = toAddr(value);
        List<Map<String, Object>> matches = new ArrayList<>();
        List<String> skipped = new ArrayList<>();
        boolean truncated = false;
        for (MemoryBlock block : currentProgram.getMemory().getBlocks()) {
            if (!block.isExecute() || !block.isInitialized()) {
                continue;
            }
            if (block.getSize() > 64 * 1024 * 1024) {
                skipped.add(block.getName());
                continue;
            }
            byte[] bytes = new byte[(int) block.getSize()];
            block.getBytes(block.getStart(), bytes);
            for (int index = 0; index + length <= bytes.length; index++) {
                if ((index & 0xffff) == 0) {
                    monitor.checkCancelled();
                }
                int opcode = Byte.toUnsignedInt(bytes[index]);
                if (lea ? (opcode != 0x48 && opcode != 0x4c)
                        || bytes[index + 1] != (byte) 0x8d
                        || (Byte.toUnsignedInt(bytes[index + 2]) & 0xc7) != 5
                        : opcode != 0xe8 && opcode != 0xe9) {
                    continue;
                }
                int offset = index + length - 4;
                int displacement = Byte.toUnsignedInt(bytes[offset])
                    | (Byte.toUnsignedInt(bytes[offset + 1]) << 8)
                    | (Byte.toUnsignedInt(bytes[offset + 2]) << 16)
                    | (bytes[offset + 3] << 24);
                Address from = block.getStart().add(index);
                if (!from.getAddressSpace().equals(target.getAddressSpace())
                        || from.getOffset() + length + displacement != target.getOffset()) {
                    continue;
                }
                if (matches.size() == MAX_RESULTS) {
                    truncated = true;
                    break;
                }
                Map<String, Object> match = addressRecord(from);
                match.put("operation", lea ? "lea" : opcode == 0xe8 ? "call" : "jump");
                matches.add(match);
            }
            if (truncated) {
                break;
            }
        }
        Map<String, Object> query = query(kind, value);
        query.put("candidates", matches);
        query.put("skipped_blocks", skipped);
        query.put("truncated", truncated);
        return query;
    }

    private Map<String, Object> exportGraph(String value) throws Exception {
        String[] fields = value.split(":", -1);
        if (fields.length != 3) {
            throw new IllegalArgumentException("graph query requires ADDRESS:DEPTH:MAX_FUNCTIONS");
        }
        int maxDepth = Integer.decode(fields[1]);
        int maxFunctions = Integer.decode(fields[2]);
        if (maxDepth < 0 || maxDepth > 16 || maxFunctions < 1 || maxFunctions > 1024) {
            throw new IllegalArgumentException("graph depth must be 0..16 and function limit 1..1024");
        }
        Function root = getFunctionContaining(toAddr(fields[0]));
        Map<String, Object> query = query("graph", value);
        query.put("found", root != null);
        if (root == null) {
            return query;
        }

        record Pending(Function function, int depth) {}
        ArrayDeque<Pending> pending = new ArrayDeque<>();
        HashSet<Address> scheduled = new HashSet<>();
        Map<Address, String> frontier = new LinkedHashMap<>();
        List<Map<String, Object>> nodes = new ArrayList<>();
        List<Map<String, Object>> edges = new ArrayList<>();
        pending.add(new Pending(root, 0));
        scheduled.add(root.getEntryPoint());
        int scanned = 0;
        int unresolved = 0;
        int incomplete = 0;
        boolean truncated = false;
        while (!pending.isEmpty() && nodes.size() < maxFunctions && scanned < MAX_RESULTS) {
            monitor.checkCancelled();
            Pending next = pending.removeFirst();
            Function function = next.function();
            Map<String, Object> node = new LinkedHashMap<>();
            node.put("entry", function.getEntryPoint().toString());
            node.put("name", function.getName());
            node.put("depth", next.depth());
            node.put("body_bytes", function.getBody().getNumAddresses());
            List<String> missingFlows = new ArrayList<>();
            nodes.add(node);
            InstructionIterator instructions =
                currentProgram.getListing().getInstructions(function.getBody(), true);
            int startCount = scanned;
            while (instructions.hasNext() && scanned < MAX_RESULTS) {
                monitor.checkCancelled();
                Instruction instruction = instructions.next();
                scanned++;
                if (!function.getBody().contains(instruction.getMaxAddress())) {
                    missingFlows.add(instruction.getAddress().toString());
                }
                Address fallthrough = instruction.getFallThrough();
                if (fallthrough != null && (!function.getBody().contains(fallthrough)
                        || getInstructionAt(fallthrough) == null)) {
                    missingFlows.add(fallthrough.toString());
                }
                Address[] targets = instruction.getFlows();
                boolean call = instruction.getFlowType().isCall();
                boolean jump = instruction.getFlowType().isJump();
                if (jump) {
                    for (Address target : targets) {
                        if (getFunctionAt(target) == null &&
                                (!function.getBody().contains(target)
                                 || getInstructionAt(target) == null)) {
                            missingFlows.add(target.toString());
                        }
                    }
                }
                if ((call || jump) && instruction.getFlowType().isComputed()
                        && targets.length == 0) {
                    Map<String, Object> edge = graphEdge(function, instruction,
                        call ? "indirect_call" : "indirect_jump");
                    edge.put("resolution", "unresolved");
                    edges.add(edge);
                    unresolved++;
                }
                boolean tail = instruction.getFlowType().isJump()
                    && instruction.getFlowType().isUnConditional()
                    && !instruction.getFlowType().isComputed();
                if (!call && !tail) {
                    continue;
                }
                for (Address target : targets) {
                    if (tail && function.getBody().contains(target)) {
                        continue;
                    }
                    Map<String, Object> edge = graphEdge(function, instruction,
                        call ? "call" : "tail_jump");
                    edge.put("target", target.toString());
                    Function callee = getFunctionAt(target);
                    edge.put("resolution", callee == null ? "undefined_function" : "analyzed");
                    edges.add(edge);
                    if (callee == null) {
                        unresolved++;
                    } else if (!scheduled.contains(target)) {
                        if (next.depth() == maxDepth) {
                            frontier.put(target, "depth_limit");
                        } else {
                            scheduled.add(target);
                            frontier.remove(target);
                            pending.addLast(new Pending(callee, next.depth() + 1));
                        }
                    }
                }
            }
            node.put("instructions_truncated", instructions.hasNext());
            node.put("instructions_scanned", scanned - startCount);
            if (scanned == startCount && !function.getBody().isEmpty()) {
                missingFlows.add(function.getEntryPoint().toString());
            }
            node.put("unexplored_flow", missingFlows);
            if (!missingFlows.isEmpty()) {
                incomplete++;
            }
            truncated |= instructions.hasNext();
        }
        for (Pending next : pending) {
            frontier.put(next.function().getEntryPoint(),
                scanned >= MAX_RESULTS ? "instruction_limit" : "function_limit");
        }
        query.put("root", root.getEntryPoint().toString());
        query.put("limits", Map.of("depth", maxDepth, "functions", maxFunctions,
            "instructions", MAX_RESULTS));
        query.put("nodes", nodes);
        query.put("edges", edges);
        query.put("instructions_scanned", scanned);
        query.put("unresolved_edges", unresolved);
        query.put("incomplete_functions", incomplete);
        query.put("frontier", frontier.entrySet().stream().map(entry -> Map.of(
            "entry", entry.getKey().toString(), "reason", entry.getValue())).toList());
        query.put("truncated", truncated || !frontier.isEmpty());
        return query;
    }

    private Map<String, Object> graphEdge(Function function, Instruction instruction, String kind) {
        Map<String, Object> edge = new LinkedHashMap<>();
        edge.put("from", function.getEntryPoint().toString());
        edge.put("site", instruction.getAddress().toString());
        edge.put("instruction", instruction.toString());
        edge.put("kind", kind);
        edge.put("computed", instruction.getFlowType().isComputed());
        return edge;
    }

    private Map<String, Object> exportString(String value) throws Exception {
        return exportBytes("string", value, value.getBytes(StandardCharsets.UTF_8));
    }

    private Map<String, Object> exportBytes(String kind, String value, byte[] needle) throws Exception {
        if (needle.length == 0 || needle.length > 4096) {
            throw new IllegalArgumentException("search pattern must contain 1..4096 bytes");
        }
        Map<String, Object> query = query(kind, value);
        Memory memory = currentProgram.getMemory();
        List<Map<String, Object>> matches = new ArrayList<>();
        Address cursor = memory.getMinAddress();
        while (cursor != null && matches.size() < MAX_RESULTS && !monitor.isCancelled()) {
            Address found = memory.findBytes(cursor, needle, null, true, monitor);
            if (found == null) {
                break;
            }
            Map<String, Object> match = addressRecord(found);
            match.put("references", referencesTo(found));
            matches.add(match);
            cursor = found.equals(memory.getMaxAddress()) ? null : found.next();
        }
        query.put("matches", matches);
        query.put("truncated", matches.size() == MAX_RESULTS);
        return query;
    }

    private Map<String, Object> exportScalar(String value) {
        long target = Long.decode(value);
        Map<String, Object> query = query("scalar", value);
        List<Map<String, Object>> matches = new ArrayList<>();
        InstructionIterator instructions = currentProgram.getListing().getInstructions(true);
        while (instructions.hasNext() && matches.size() < MAX_RESULTS && !monitor.isCancelled()) {
            Instruction instruction = instructions.next();
            for (int operand = 0; operand < instruction.getNumOperands(); operand++) {
                for (Object object : instruction.getOpObjects(operand)) {
                    if (!(object instanceof Scalar scalar)
                            || scalar.getUnsignedValue() != target) {
                        continue;
                    }
                    Map<String, Object> match = instructionRecord(instruction);
                    match.put("operand", operand);
                    matches.add(match);
                }
            }
        }
        query.put("matches", matches);
        query.put("truncated", matches.size() == MAX_RESULTS);
        return query;
    }

    private Map<String, Object> exportPointers(String value) throws Exception {
        String[] fields = value.split(":", -1);
        if (fields.length != 3) {
            throw new IllegalArgumentException(
                "pointers query requires START:COUNT:WIDTH");
        }
        Address start = toAddr(fields[0]);
        int count = Integer.decode(fields[1]);
        int width = Integer.decode(fields[2]);
        if (count < 1 || count > MAX_RESULTS || (width != 4 && width != 8)) {
            throw new IllegalArgumentException(
                "pointer count must be 1..100000 and width must be 4 or 8");
        }

        Memory memory = currentProgram.getMemory();
        List<Map<String, Object>> entries = new ArrayList<>();
        for (int index = 0; index < count; index++) {
            Address slot = start.add((long) index * width);
            long raw;
            Address target;
            if (width == 4) {
                raw = Integer.toUnsignedLong(memory.getInt(slot));
                target = currentProgram.getImageBase().add(raw);
            } else {
                raw = memory.getLong(slot);
                target = toAddr(raw);
            }
            Map<String, Object> entry = new LinkedHashMap<>();
            entry.put("index", index);
            entry.put("slot", slot.toString());
            entry.put("raw", String.format("0x%x", raw));
            entry.put("target", target.toString());
            Function function = getFunctionAt(target);
            if (function != null) {
                entry.put("function", function.getName());
            }
            entries.add(entry);
        }
        Map<String, Object> query = query("pointers", value);
        query.put("entries", entries);
        return query;
    }

    private List<Map<String, Object>> referencesTo(Address target) {
        List<Map<String, Object>> references = new ArrayList<>();
        ReferenceIterator iterator = currentProgram.getReferenceManager().getReferencesTo(target);
        while (iterator.hasNext() && references.size() < MAX_RESULTS && !monitor.isCancelled()) {
            Reference reference = iterator.next();
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("from", reference.getFromAddress().toString());
            result.put("type", reference.getReferenceType().toString());
            Function function = getFunctionContaining(reference.getFromAddress());
            if (function != null) {
                result.put("function_entry", function.getEntryPoint().toString());
                result.put("function", function.getName());
            }
            references.add(result);
        }
        return references;
    }

    private Map<String, Object> instructionRecord(Instruction instruction) {
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("address", instruction.getAddress().toString());
        result.put("text", instruction.toString());
        Function function = getFunctionContaining(instruction.getAddress());
        if (function != null) {
            result.put("function_entry", function.getEntryPoint().toString());
            result.put("function", function.getName());
        }
        return result;
    }

    private Map<String, Object> addressRecord(Address address) {
        Map<String, Object> record = new LinkedHashMap<>();
        record.put("address", address.toString());
        MemoryBlock block = currentProgram.getMemory().getBlock(address);
        if (block != null && block.isExecute()) {
            record.put("analyzed_boundary", getInstructionAt(address) != null);
            var functions = currentProgram.getFunctionManager().getFunctions(address, false);
            if (functions.hasNext()) {
                record.put("preceding_function", functions.next().getEntryPoint().toString());
            }
        }
        return record;
    }

    private Map<String, Object> query(String kind, String input) {
        Map<String, Object> query = new LinkedHashMap<>();
        query.put("kind", kind);
        query.put("input", input);
        return query;
    }
}

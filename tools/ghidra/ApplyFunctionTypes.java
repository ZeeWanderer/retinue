// Apply reviewed function types to a matching analysis project.
//@category wfcli

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import ghidra.app.cmd.function.ApplyFunctionSignatureCmd;
import ghidra.app.script.GhidraScript;
import ghidra.app.util.parser.FunctionSignatureParser;
import ghidra.program.model.data.FunctionDefinitionDataType;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.SourceType;
import java.nio.file.Files;
import java.nio.file.Path;

public class ApplyFunctionTypes extends GhidraScript {
    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) {
            throw new IllegalArgumentException("expected a function-types JSON file");
        }
        Path path = Path.of(args[0]);
        if (Files.size(path) > 1024 * 1024) {
            throw new IllegalArgumentException("type annotations exceed 1 MiB");
        }
        JsonObject input = JsonParser.parseString(Files.readString(path)).getAsJsonObject();
        String sha256 = input.get("sha256").getAsString();
        if (!sha256.matches("[0-9a-fA-F]{64}") ||
                !sha256.equalsIgnoreCase(currentProgram.getExecutableSHA256())) {
            throw new IllegalArgumentException("type annotations do not match executable SHA-256");
        }
        var entries = input.getAsJsonArray("functions");
        if (entries == null || entries.isEmpty() || entries.size() > 1024) {
            throw new IllegalArgumentException("expected 1..1024 function annotations");
        }
        var parser = new FunctionSignatureParser(currentProgram.getDataTypeManager(), null);
        int transaction = currentProgram.startTransaction("Apply reviewed function types");
        boolean applied = false;
        try {
            for (JsonElement element : entries) {
                monitor.checkCancelled();
                JsonObject entry = element.getAsJsonObject();
                Function function = getFunctionAt(toAddr(entry.get("address").getAsString()));
                if (function == null) {
                    throw new IllegalArgumentException("annotation address is not a function entry");
                }
                if (!entry.has("signature")) {
                    throw new IllegalArgumentException("expected a complete function signature");
                }
                FunctionDefinitionDataType signature = parser.parse(function.getSignature(),
                    entry.get("signature").getAsString());
                var command = new ApplyFunctionSignatureCmd(function.getEntryPoint(),
                    signature, SourceType.USER_DEFINED);
                if (!command.applyTo(currentProgram, monitor)) {
                    throw new IllegalStateException(command.getStatusMsg());
                }
            }
            applied = true;
        } finally {
            currentProgram.endTransaction(transaction, applied);
        }
        println("Applied " + entries.size() + " function type annotations from " + path);
    }
}

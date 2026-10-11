// Export analysis observations into an explicitly supplied PRIVATE report directory.
// @category ATV
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.address.*;
import ghidra.program.util.DefinedDataIterator;
import com.google.gson.*;
import java.nio.file.*;
import java.nio.*;
import java.security.MessageDigest;
import java.util.*;

public class AtvRecover extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length == 0) throw new IllegalArgumentException("Supply private output directory, then optional function addresses");
        Path output = Paths.get(args[0]);
        Files.createDirectories(output);
        Gson gson = new GsonBuilder().setPrettyPrinting().create();
        JsonObject report = new JsonObject();
        report.addProperty("language", currentProgram.getLanguageID().toString());
        report.addProperty("compiler", currentProgram.getCompilerSpec().getCompilerSpecID().toString());
        report.addProperty("source_sha256", currentProgram.getExecutableSHA256());
        byte[] source = Files.readAllBytes(Paths.get(currentProgram.getExecutablePath()));
        ByteBuffer elf = ByteBuffer.wrap(source).order(ByteOrder.LITTLE_ENDIAN);
        if (elf.getInt(0) != 0x464c457f || source[4] != 1 || source[5] != 1) {
            throw new IllegalStateException("Expected little-endian ELF32 source");
        }
        int headers = elf.getInt(28);
        int stride = Short.toUnsignedInt(elf.getShort(42));
        int count = Short.toUnsignedInt(elf.getShort(44));
        JsonArray loads = new JsonArray();
        for (int i = 0; i < count; i++) {
            int at = headers + i * stride;
            if (elf.getInt(at) != 1) continue;
            int offset = elf.getInt(at + 4), length = elf.getInt(at + 16);
            long address = Integer.toUnsignedLong(elf.getInt(at + 8));
            byte[] original = Arrays.copyOfRange(source, offset, offset + length);
            byte[] loaded = new byte[length];
            if (length > 0 && currentProgram.getMemory().getBytes(toAddr(address), loaded) != length) {
                throw new IllegalStateException("Incomplete mapped segment");
            }
            boolean identical = Arrays.equals(original, loaded);
            JsonObject load = new JsonObject();
            load.addProperty("address", Long.toHexString(address));
            load.addProperty("file_offset", offset);
            load.addProperty("bytes", length);
            load.addProperty("byte_identical", identical);
            load.addProperty("sha256", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(loaded)));
            loads.add(load);
            if (!identical) throw new IllegalStateException("Loaded program differs from source segment");
        }
        report.add("loaded_segments", loads);
        report.addProperty("caveat", "Ghidra analysis observations, not verified function boundaries, original source, or runtime behavior.");
        JsonArray functions = new JsonArray();
        FunctionManager manager = currentProgram.getFunctionManager();
        for (Function f : manager.getFunctions(true)) {
            monitor.checkCancelled();
            JsonObject row = new JsonObject();
            row.addProperty("address", f.getEntryPoint().toString());
            row.addProperty("name", f.getName());
            row.addProperty("bytes", f.getBody().getNumAddresses());
            row.addProperty("external", f.isExternal());
            row.addProperty("thunk", f.isThunk());
            JsonArray ranges = new JsonArray();
            for (AddressRange range : f.getBody().getAddressRanges()) {
                ranges.add(range.getMinAddress() + "-" + range.getMaxAddress());
            }
            row.add("ranges", ranges);
            functions.add(row);
        }
        report.add("functions", functions);
        JsonArray strings = new JsonArray();
        for (Data data : DefinedDataIterator.byDataInstance(currentProgram, d -> d.hasStringValue())) {
            monitor.checkCancelled();
            JsonObject row = new JsonObject();
            row.addProperty("address", data.getAddress().toString());
            row.addProperty("value", String.valueOf(data.getValue()));
            JsonArray refs = new JsonArray();
            for (Reference ref : currentProgram.getReferenceManager().getReferencesTo(data.getAddress())) {
                JsonObject r = new JsonObject();
                r.addProperty("from", ref.getFromAddress().toString());
                r.addProperty("type", ref.getReferenceType().toString());
                Function f = manager.getFunctionContaining(ref.getFromAddress());
                if (f != null) r.addProperty("function", f.getEntryPoint().toString());
                refs.add(r);
            }
            row.add("references", refs);
            strings.add(row);
        }
        report.add("strings", strings);
        Files.writeString(output.resolve("inventory.json"), gson.toJson(report));
        if (args.length > 1) {
            DecompInterface decompiler = new DecompInterface();
            try {
                if (!decompiler.openProgram(currentProgram)) throw new IllegalStateException(decompiler.getLastMessage());
                for (int i = 1; i < args.length; i++) {
                    if (args[i].startsWith("data:")) {
                        Address dataAddress = toAddr(args[i].substring(5));
                        JsonObject dataContext = new JsonObject();
                        dataContext.addProperty("address", dataAddress.toString());
                        JsonArray references = new JsonArray();
                        for (int offset = 0; offset < 16; offset++) {
                            Address component = dataAddress.add(offset);
                            for (Reference ref : currentProgram.getReferenceManager().getReferencesTo(component)) {
                                JsonObject item = new JsonObject();
                                item.addProperty("offset", offset);
                                item.addProperty("from", ref.getFromAddress().toString());
                                item.addProperty("type", ref.getReferenceType().toString());
                                Function caller = manager.getFunctionContaining(ref.getFromAddress());
                                if (caller != null) item.addProperty("function", caller.getEntryPoint().toString());
                                references.add(item);
                            }
                        }
                        dataContext.add("references", references);
                        Files.writeString(output.resolve(dataAddress + ".data.json"), gson.toJson(dataContext));
                        continue;
                    }
                    Address address = toAddr(args[i]);
                    Function f = manager.getFunctionContaining(address);
                    boolean requestedDiscovery = false;
                    if (f == null && currentProgram.getMemory().contains(address)) {
                        // Explicit requested entries may be present only in vtables.
                        // Changes are session-local when the wrapper uses -readOnly.
                        disassemble(address);
                        f = createFunction(address, null);
                        requestedDiscovery = f != null;
                    }
                    if (f == null) { println("No analyzed function at " + address); continue; }
                    DecompileResults result = decompiler.decompileFunction(f, 60, monitor);
                    String stem = f.getEntryPoint().toString();
                    JsonObject context = new JsonObject();
                    context.addProperty("entry", stem);
                    context.addProperty("discovered_at_requested_address", requestedDiscovery);
                    JsonArray incoming = new JsonArray();
                    for (Reference ref : currentProgram.getReferenceManager().getReferencesTo(f.getEntryPoint())) {
                        JsonObject item = new JsonObject();
                        item.addProperty("from", ref.getFromAddress().toString());
                        item.addProperty("type", ref.getReferenceType().toString());
                        Function caller = manager.getFunctionContaining(ref.getFromAddress());
                        if (caller != null) item.addProperty("function", caller.getEntryPoint().toString());
                        incoming.add(item);
                    }
                    context.add("incoming", incoming);
                    JsonArray outgoing = new JsonArray();
                    for (Instruction instruction : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                        for (Reference ref : instruction.getReferencesFrom()) {
                            if (!ref.getReferenceType().isCall()) continue;
                            JsonObject item = new JsonObject();
                            item.addProperty("from", ref.getFromAddress().toString());
                            item.addProperty("to", ref.getToAddress().toString());
                            item.addProperty("type", ref.getReferenceType().toString());
                            outgoing.add(item);
                        }
                    }
                    context.add("calls", outgoing);
                    Files.writeString(output.resolve(stem + ".json"), gson.toJson(context));
                    if (result.decompileCompleted()) {
                        Files.writeString(output.resolve(stem + ".c"), result.getDecompiledFunction().getC());
                    } else {
                        Files.writeString(output.resolve(stem + ".error.txt"), result.getErrorMessage());
                    }
                    StringBuilder assembly = new StringBuilder();
                    for (Instruction instruction : currentProgram.getListing().getInstructions(f.getBody(), true)) {
                        assembly.append(instruction.getAddress()).append(" ");
                        for (byte b : instruction.getBytes()) assembly.append(String.format("%02x", b & 255));
                        assembly.append("  ").append(instruction).append("\n");
                    }
                    Files.writeString(output.resolve(stem + ".asm"), assembly.toString());
                }
            } finally { decompiler.dispose(); }
        }
        println("Exported " + functions.size() + " analyzed functions and " + strings.size() + " strings to private report directory.");
    }
}

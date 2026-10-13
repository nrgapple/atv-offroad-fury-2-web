// Map the exact supplied game module at the address verified in the boot loader.
// Research only: no original code is executed. Output and databases stay private.
// @category ATV
import ghidra.app.script.GhidraScript;
import ghidra.program.model.mem.MemoryBlock;
import java.nio.file.*;
import java.nio.*;
import java.io.ByteArrayInputStream;
import java.security.MessageDigest;
import java.util.*;

public class MxLoadModule extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) throw new IllegalArgumentException("Supply private GAME_REL.BIN path");
        if (!currentProgram.getLanguageID().toString().equals("r5900:LE:32:default")
            || !currentProgram.getExecutableSHA256().equals("4b74c0c76ffe0838e00c1b9f75b31e1bb91adae06ec9da1416355b363572102a")) {
            throw new IllegalStateException("Expected exact SLUS_211.04 boot program");
        }
        byte[] data = Files.readAllBytes(Paths.get(args[0]));
        String hash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
        if (!hash.equals("bea3eb36260f5ab64d9de1919318593a6a58b08fb2c160e9450f5a0e1e334957")) {
            throw new IllegalStateException("Unexpected game module identity");
        }
        ByteBuffer header = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN);
        long base = Integer.toUnsignedLong(header.getInt(8));
        if (base != 0x45c580L || currentProgram.getMemory().getInt(toAddr(0x3d3a88)) != base) {
            throw new IllegalStateException("Boot module destination/header disagree");
        }
        MemoryBlock block = currentProgram.getMemory().getBlock("MX_GAME_MODULE");
        if (block == null) {
            block = currentProgram.getMemory().createInitializedBlock("MX_GAME_MODULE", toAddr(base),
                new ByteArrayInputStream(data), data.length, monitor, false);
            block.setRead(true); block.setWrite(true); block.setExecute(true);
        }
        byte[] loaded = new byte[data.length];
        currentProgram.getMemory().getBytes(toAddr(base), loaded);
        if (!Arrays.equals(loaded, data)) throw new IllegalStateException("Module mapped bytes differ");
        // The boot loader zeroes this tail after its exact file-size read.
        int bss = header.getInt(20);
        if (currentProgram.getMemory().getBlock("MX_GAME_MODULE_BSS") == null) {
            currentProgram.getMemory().createInitializedBlock("MX_GAME_MODULE_BSS", toAddr(base + data.length),
                bss, (byte)0, monitor, false);
        }
        Set<Long> entries = new TreeSet<>();
        entries.add(base + 0x80);
        for (int i = 0x80; i < Math.min(header.getInt(12), data.length); i += 4) {
            int word = header.getInt(i);
            if ((word >>> 26) == 3) {
                long target = Integer.toUnsignedLong((word & 0x03ffffff) << 2);
                if (target >= base + 0x80 && target < base + header.getInt(12)) entries.add(target);
            }
        }
        for (long entry : entries) {
            monitor.checkCancelled();
            addEntryPoint(toAddr(entry));
        }
        println("Mapped exact GAME_REL.BIN at 0045c580; " + data.length
            + " original bytes match; " + entries.size() + " call-target candidates, not verified identities.");
    }
}

// Configure only the private analysis database; original input bytes stay unchanged.
// @category ATV
import ghidra.app.script.GhidraScript;

public class AtvPrepare extends GhidraScript {
    public void run() throws Exception {
        if (!currentProgram.getLanguageID().toString().equals("r5900:LE:32:default")) {
            throw new IllegalStateException("Expected the Emotion Engine R5900 language");
        }
        // The EE extension documents this analyzer as a source of decompilation failures.
        setAnalysisOption(currentProgram, "Decompiler Parameter ID", "false");
        println("Exact R5900 language selected; original executable is not modified.");
    }
}

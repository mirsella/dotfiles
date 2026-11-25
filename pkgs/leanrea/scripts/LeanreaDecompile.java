// Emits one analyzed Ghidra function as C-like pseudocode.
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import java.nio.file.Files;
import java.nio.file.Path;

public class LeanreaDecompile extends GhidraScript {
    @Override
    public void run() throws Exception {
        var args = getScriptArgs();
        var function = findFunction(args[0]);
        var decompiler = new DecompInterface();
        try {
            if (!decompiler.openProgram(currentProgram)) {
                throw new IllegalStateException(decompiler.getLastMessage());
            }
            var result = decompiler.decompileFunction(function, 30, monitor);
            var code = result.getDecompiledFunction();
            if (code == null) {
                throw new IllegalStateException(result.getErrorMessage());
            }
            Files.writeString(Path.of(args[1]), code.getC());
        } finally {
            decompiler.dispose();
        }
    }

    private Function findFunction(String selector) throws Exception {
        var manager = currentProgram.getFunctionManager();
        Function match = null;
        if (selector.startsWith("0x")) {
            var address = currentProgram.getAddressFactory().getAddress(selector.substring(2));
            if (address != null) {
                match = manager.getFunctionAt(address);
            }
        } else {
            for (var function : manager.getFunctions(true)) {
                monitor.checkCancelled();
                if (!selector.equals(function.getName()) && !selector.equals(function.getName(true))) {
                    continue;
                }
                if (match != null) {
                    throw new IllegalArgumentException(
                        "Ambiguous function " + selector + "; use a qualified name or Ghidra entry address");
                }
                match = function;
            }
        }
        if (match == null) {
            throw new IllegalArgumentException("No function found for " + selector);
        }
        return match;
    }
}

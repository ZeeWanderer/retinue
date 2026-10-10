//@category wfcli.tests
package tests;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;

public class SeedPartialFunction extends GhidraScript {
    @Override
    protected void run() throws Exception {
        Address entry = toAddr(getScriptArgs()[0]);
        getFunctionAt(entry).setBody(new AddressSet(entry));
    }
}

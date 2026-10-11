-module(wfcli_mcp_cli).

-export([command/0, main/1]).

command() ->
    #{help => "serve MCP over standard input and output", handler => {?MODULE, main},
      notes => "\nNewline-delimited JSON-RPC. Diagnostics use standard error.\n"
               "Closing the connection cancels its outstanding daemon requests.\n"}.

main(_Args) ->
    retinue_mcp_cli:run(wfcli_mcp).

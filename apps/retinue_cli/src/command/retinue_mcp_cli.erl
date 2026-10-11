-module(retinue_mcp_cli).
-export([command/0, run/1]).

command() ->
    #{help => "serve platform and game tools over MCP", handler => fun(_) -> run(retinue_mcp) end}.

run(Adapter) ->
    ok = io:setopts(standard_io, [binary, {encoding, unicode}]),
    ok = io:setopts(standard_error, [{encoding, unicode}]),
    case retinue_mcp_server:run(Adapter) of
        ok -> ok;
        {error, Reason} -> retinue_cli_runtime:fail(io_lib:format("MCP failed: ~p", [Reason]))
    end.

-module(wfcli_mcp).
-export([name/0, instructions/0, definitions/0, call/2, resources/0, read/1]).

name() -> <<"warframe-mcp">>.
instructions() -> <<"Use query for Warframe data and read wfcli://query-language when composing filters.">>.
definitions() -> wfcli_mcp_tools:definitions().
call(Name, Args) -> wfcli_mcp_tools:call(Name, Args).
resources() -> wfcli_mcp_resources:list().
read(Uri) -> wfcli_mcp_resources:read(Uri).

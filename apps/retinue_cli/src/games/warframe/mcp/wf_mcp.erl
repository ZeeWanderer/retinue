-module(wf_mcp).
-export([name/0, instructions/0, definitions/0, call/2, resources/0, read/1]).

name() -> <<"warframe-mcp">>.
instructions() -> <<"Use query for Warframe data and read wfcli://query-language when composing filters.">>.
definitions() -> wf_mcp_tools:definitions().
call(Name, Args) -> wf_mcp_tools:call(Name, Args).
resources() -> wf_mcp_resources:list().
read(Uri) -> wf_mcp_resources:read(Uri).

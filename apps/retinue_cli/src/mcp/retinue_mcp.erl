-module(retinue_mcp).
-export([name/0, instructions/0, definitions/0, call/2, resources/0, read/1]).

name() -> <<"retinue-mcp">>.
instructions() -> <<"Select game-prefixed tools for game data. Read wfcli://query-language for Warframe queries.">>.
definitions() ->
    [#{<<"name">> => <<"daemon_status">>, <<"description">> => <<"Inspect Retinue daemon and game services">>,
       <<"inputSchema">> => #{<<"type">> => <<"object">>, <<"properties">> => #{}}},
     #{<<"name">> => <<"poe2_session">>, <<"description">> => <<"PoE2 native session: scene, area, character, XP, gold, panels, read issues and recent changes">>,
       <<"inputSchema">> => #{<<"type">> => <<"object">>, <<"properties">> => #{}}} |
     [Definition#{<<"name">> := <<"warframe_", (maps:get(<<"name">>, Definition))/binary>>}
      || Definition <- wf_mcp_tools:definitions()]].

call(<<"daemon_status">>, _Args) -> retinue_client:call(status);
call(<<"poe2_session">>, _Args) -> retinue_client:call({game, poe2, status});
call(<<"warframe_", Name/binary>>, Args) -> wf_mcp_tools:call(Name, Args);
call(Name, _Args) -> {error, {unknown_tool, Name}}.

resources() -> wf_mcp_resources:list().
read(Uri) -> wf_mcp_resources:read(Uri).

%%%-------------------------------------------------------------------
%% MCP boundary and daemon integration tests.
%%%-------------------------------------------------------------------
-module(wf_mcp_eunit).

-include_lib("eunit/include/eunit.hrl").

mcp_help_describes_stdio_boundary_test() ->
    Help = unicode:characters_to_binary(wf_test_cli:help(["mcp"])),
    ?assertNotEqual(nomatch, binary:match(Help, <<"wfcli mcp">>)),
    ?assertNotEqual(nomatch, binary:match(Help, <<"standard input and output">>)).

initialize_negotiates_current_protocol_test() ->
    Message = #{<<"method">> => <<"initialize">>,
                <<"params">> => #{<<"protocolVersion">> => <<"2025-11-25">>}},
    {ok, Result} = retinue_mcp_server:request(wf_mcp, Message),
    ?assertEqual(<<"2025-11-25">>, maps:get(<<"protocolVersion">>, Result)),
    ?assert(maps:is_key(<<"tools">>, maps:get(<<"capabilities">>, Result))),
    ?assert(maps:is_key(<<"resources">>, maps:get(<<"capabilities">>, Result))).

initialize_falls_back_for_unknown_protocol_test() ->
    Message = #{<<"method">> => <<"initialize">>,
                <<"params">> => #{<<"protocolVersion">> => <<"unknown">>}},
    {ok, Result} = retinue_mcp_server:request(wf_mcp, Message),
    ?assertEqual(<<"2025-11-25">>, maps:get(<<"protocolVersion">>, Result)).

initialize_rejects_non_object_params_test() ->
    lists:foreach(fun(Params) ->
        ?assertMatch({error, -32602, _, _}, retinue_mcp_server:request(wf_mcp,
                     #{<<"method">> => <<"initialize">>, <<"params">> => Params}))
    end, [null, [], <<"invalid">>, 1]).

tool_definitions_expose_headless_surface_test() ->
    Names = [maps:get(<<"name">>, Definition) || Definition <- wf_mcp_tools:definitions()],
    ?assertEqual([<<"query">>, <<"forma_plan">>, <<"daemon_status">>,
                  <<"update_knowledge">>], Names),
    Query = hd(wf_mcp_tools:definitions()),
    Properties = maps:get(<<"properties">>, maps:get(<<"inputSchema">>, Query)),
    ?assertNot(maps:is_key(<<"output_format">>, Properties)),
    ?assertNot(maps:is_key(<<"visualize">>, Properties)).

json_normalizes_erlang_service_terms_test() ->
    Value = #{status => running, 1 => "slot", tuple => {ok, 2}, numbers => [1, 2]},
    {ok, Decoded} = retinue_json:decode(retinue_json:encode(Value)),
    ?assertEqual(<<"running">>, maps:get(<<"status">>, Decoded)),
    ?assertEqual(<<"slot">>, maps:get(<<"1">>, Decoded)),
    ?assertEqual([<<"ok">>, 2], maps:get(<<"tuple">>, Decoded)),
    ?assertEqual([1, 2], maps:get(<<"numbers">>, Decoded)).

dataset_resource_uses_protocol_contract_test() ->
    {ok, <<"application/json">>, Text} = wf_mcp_resources:read(<<"wfcli://datasets">>),
    {ok, Data} = retinue_json:decode(Text),
    ?assertEqual([<<"worldstate">>, <<"mods">>, <<"items">>, <<"codex">>,
                  <<"enemies">>, <<"drops">>],
                 maps:get(<<"default">>, Data)),
    All = maps:get(<<"all">>, Data),
    ?assertEqual(11, length(All)),
    ?assert(lists:member(<<"diagnostics">>, All)),
    ?assert(lists:member(<<"incidents">>, All)),
    ?assert(lists:member(<<"captures">>, All)).

worldstate_schema_resource_is_packaged_test() ->
    {ok, <<"application/json">>, Text} =
        wf_mcp_resources:read(<<"wfcli://schema/worldstate">>),
    {ok, Data} = retinue_json:decode(Text),
    ?assert(length(maps:get(<<"columns">>, Data)) > 10),
    ?assert(maps:is_key(<<"fissure">>, maps:get(<<"types">>, Data))).

invalid_tool_arguments_are_data_errors_test() ->
    ?assertMatch({error, {invalid_arguments, _}},
                 wf_mcp_tools:call(<<"query">>, #{})),
    ?assertMatch({error, {unknown_tool, _}},
                 wf_mcp_tools:call(<<"missing">>, #{})).

daemon_backed_tools_test_() ->
    {setup,
     fun setup_daemon_tools/0,
     fun cleanup_daemon_tools/1,
     fun(Cache) -> fun() ->
         Root = filename:absname("."),
         QueryArgs = #{<<"query">> => <<"type=Fissure">>,
                       <<"datasets">> => [<<"worldstate">>],
                       <<"cache">> => unicode:characters_to_binary(Cache),
                       <<"ttl">> => 999999999,
                       <<"cwd">> => unicode:characters_to_binary(Root)},
         {ok, QueryResult} = wf_mcp_tools:call(<<"query">>, QueryArgs),
         [Dataset] = maps:get(datasets, QueryResult),
         ?assertEqual(worldstate, maps:get(dataset, Dataset)),
         {ok, Worldstate} = maps:get(reply, Dataset),
         ?assert(maps:get(entries, Worldstate) =/= []),

         {ok, Catalog} = wf_mcp_tools:call(<<"query">>,
             #{<<"query">> => <<"dataset=items">>, <<"limit">> => 1,
               <<"exports_dir">> => <<"apps/retinue_cli/test/fixtures/exports">>,
               <<"cwd">> => unicode:characters_to_binary(Root)}),
         [#{reply := {ok, #{results := CatalogResult}}}] = maps:get(datasets, Catalog),
         ?assert(maps:get(total, CatalogResult) > 1),
         ?assertEqual(1, maps:get(shown, CatalogResult)),
         ?assertEqual(1, length(maps:get(slice, CatalogResult))),
         ?assertNot(maps:is_key(all, CatalogResult)),

         FormaArgs = #{<<"configs">> =>
                           [<<"apps/retinue_cli/test/fixtures/simple_capacity.yml">>],
                       <<"cwd">> => unicode:characters_to_binary(Root)},
         {ok, FormaResult} = wf_mcp_tools:call(<<"forma_plan">>, FormaArgs),
         ?assertMatch([_], maps:get(results, FormaResult))
     end end}.

setup_daemon_tools() ->
    ok = retinue_test_daemon:start(),
    Fixture = "apps/retinue_cli/test/fixtures/worldstate_sample.json",
    Cache = filename:join(
              "/tmp", "wfcli-mcp-worldstate-" ++
              integer_to_list(erlang:unique_integer([positive])) ++ ".json"),
    {ok, Binary} = file:read_file(Fixture),
    ok = file:write_file(Cache, Binary),
    Cache.

cleanup_daemon_tools(Cache) ->
    retinue_test_daemon:stop(),
    _ = file:delete(Cache),
    _ = file:delete(Cache ++ ".lock"),
    ok.

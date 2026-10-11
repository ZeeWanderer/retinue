%%%-------------------------------------------------------------------
%% Common Test for unified query command.
%%%-------------------------------------------------------------------
-module(wf_query_SUITE).

-export([all/0,
         init_per_suite/1,
         end_per_suite/1,
         query_combines_sources/1,
         query_selects_drops/1,
         query_format_alias/1,
         query_archimedea_semantic_fields/1,
         query_raw_worldstate_paths/1,
         global_json_outputs/1,
         json_multiple_datasets/1,
         incident_queries_and_capture_status/1,
         json_worldstate_watch/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [query_combines_sources,
     query_selects_drops,
     query_format_alias,
     query_archimedea_semantic_fields,
     query_raw_worldstate_paths,
     global_json_outputs,
     json_multiple_datasets,
     incident_queries_and_capture_status,
     json_worldstate_watch].

init_per_suite(Config) ->
    {ok, _} = application:ensure_all_started(retinue_cli),
    ok = retinue_test_daemon:start(),
    Config.

end_per_suite(_Config) ->
    retinue_test_daemon:stop(),
    ok.

query_combines_sources(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["query",
            "--cache", Cache,
            "--ttl", "999999999",
            "--exports-dir", Dir,
            "--knowledge-dir", fixture_knowledge_dir(),
            "test"
        ])
    end),
    ?assert(string:find(Output, "== Worldstate ==") =/= nomatch),
    ?assert(string:find(Output, "== Items ==") =/= nomatch),
    ?assert(string:find(Output, "== Drops ==") =/= nomatch),
    ?assert(string:find(Output, "Test Gun") =/= nomatch).

query_selects_drops(_Config) ->
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["query",
            "--exports-dir", fixture_dir(),
            "--knowledge-dir", fixture_knowledge_dir(),
            "dataset=drops|codex test mod"
        ])
    end),
    ?assert(string:find(Output, "== Drops ==") =/= nomatch),
    ?assert(string:find(Output, "Test Mod") =/= nomatch),
    ?assert(string:find(Output, "== Codex ==") =/= nomatch),
    ?assert(string:find(Output, "== Worldstate ==") =:= nomatch),
    ?assert(string:find(Output, "== Items ==") =:= nomatch).

query_format_alias(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Dir = fixture_dir(),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["query",
            "--cache", Cache,
            "--ttl", "999999999",
            "--exports-dir", Dir,
            "--format", "table",
            "--knowledge-dir", fixture_knowledge_dir(),
            "test"
        ])
    end),
    ?assert(string:find(Output, "== Worldstate ==") =/= nomatch),
    ?assert(string:find(Output, "== Mods ==") =/= nomatch).

query_archimedea_semantic_fields(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["query",
            "--cache", Cache,
            "--ttl", "999999999",
            "--format", "block",
            "dataset=worldstate type=archimedea archimedea=deep deviation~sealed"
        ])
    end),
    ?assert(string:find(Output, "Deep Archimedea") =/= nomatch),
    ?assert(string:find(Output, "Sealed Armor") =/= nomatch),
    ?assert(string:find(Output, "Commanding Culverins") =/= nomatch),
    ?assertEqual(nomatch, string:find(Output, "Temporal Archimedea")).

query_raw_worldstate_paths(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["query",
            "--cache", Cache,
            "--ttl", "999999999",
            "dataset=worldstate type=raw_worldstate data.Conquests.1.Type=CT_HEX "
            "extract=data.Conquests.1.Missions.*.missionType "
            "extract=data.Conquests.1.Variables.*"
        ])
    end),
    ?assert(string:find(Output, "Raw worldstate") =/= nomatch),
    ?assert(string:find(Output, "MT_ENDLESS_CAPTURE") =/= nomatch),
    ?assert(string:find(Output, "Exhaustion") =/= nomatch).

global_json_outputs(_Config) ->
    lists:foreach(fun(Args) -> ?assert(is_map(json_command(["--json" | Args]))) end,
        [["daemon", "status"], ["companion", "status"], ["companion", "capture", "status"],
         ["player"], ["notifications"], ["paths", "wfcli"], ["completion", "status"],
         ["diagnostics", "unresolved"]]).

json_multiple_datasets(_Config) ->
    Data = json_command(["query", "--json", "--limit", "1", "--exports-dir", fixture_dir(),
                         "--knowledge-dir", fixture_knowledge_dir(),
                         "dataset=codex|drops test"]),
    Datasets = maps:get(<<"datasets">>, Data),
    ?assertEqual([<<"codex">>, <<"drops">>], [maps:get(<<"dataset">>, D) || D <- Datasets]),
    [?assertEqual(1, maps:get(<<"shown">>, maps:get(<<"results">>, D))) || D <- Datasets].

incident_queries_and_capture_status(_Config) ->
    Path = retinue_incident_log:path(),
    {ok, Millis} = retinue_time:parse("2026-09-30T12:00:00Z"),
    Event = #{<<"timestamp_ms">> => Millis, <<"level">> => <<"warn">>,
              <<"event">> => <<"test.incident">>, <<"message">> => <<"adapter unavailable">>},
    ok = file:write_file(Path, [json:encode(Event), $\n]),
    #{<<"datasets">> := [#{<<"results">> := Results}]} =
        json_command(["query", "--json", "dataset=incidents application=daemon "
                      "timestamp>=2026-09-30T11:00:00Z event=test.incident"]),
    ?assertEqual(1, maps:get(<<"total">>, Results)),
    [Row] = maps:get(<<"slice">>, Results),
    ?assertEqual(<<"2026-09-30T12:00:00.000Z">>, maps:get(<<"timestamp">>, Row)),
    ?assertEqual(Millis, maps:get(<<"timestamp_ms">>, maps:get(<<"data">>, Row))),
    ?assert(maps:is_key(<<"logs">>, Results)),
    Text = capture_output(fun() -> retinue_cli:main(warframe, [
        "--utc", "query", "dataset=incidents event=test.incident"]) end),
    ?assertNotEqual(nomatch, string:find(Text, "2026-09-30T12:00:00.000Z")),
    #{<<"datasets">> := [#{<<"results">> := Captures}]} =
        json_command(["query", "dataset=captures", "--json"]),
    ?assert(maps:get(<<"total">>, Captures) >= 1).

json_worldstate_watch(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Common = ["--cache", Cache, "--ttl", "999999999", "--raw"],
    #{<<"entries">> := [Entry | _]} = json_command(["--json", "fissures" | Common]),
    ?assertNotEqual(nomatch, binary:match(maps:get(<<"expiry">>, maps:get(<<"row_map">>, Entry)), <<"Z">>)),
    Watch = json_command(["watch", "--json", "--once", "--spec", "fissures" | Common]),
    ?assertMatch(#{<<"timestamp">> := _, <<"specs">> := [_]}, Watch),
    #{<<"specs">> := [#{<<"entries">> := [WatchExtract | _]}]} = json_command([
        "watch", "--json", "--once", "--spec",
        "fissures:extract=Node" | Common]),
    ?assertMatch(#{<<"extracts">> := #{<<"Node">> := [_]}}, WatchExtract),
    #{<<"datasets">> := [#{<<"entries">> := [Extract]}]} = json_command([
        "query", "--json", "dataset=worldstate type=raw_worldstate extract=Conquests.0.Type" | Common]),
    ?assertMatch(#{<<"extracts">> := #{<<"Conquests.0.Type">> := [<<"CT_LAB">>]}}, Extract).

json_command(Args) ->
    {ok, Data} = retinue_json:decode(capture_output(fun() -> retinue_cli:main(warframe, Args) end)),
    Data.

fixture_dir() ->
    filename:join([code:lib_dir(retinue_cli), "test", "fixtures", "exports"]).

fixture_knowledge_dir() ->
    filename:join([code:lib_dir(retinue_cli), "test", "fixtures", "knowledge"]).

sample_cache() ->
    File = filename:join([code:lib_dir(retinue_cli), "test", "fixtures", "worldstate_sample.json"]),
    {ok, Bin} = file:read_file(File),
    BaseTmp = case os:getenv("TMPDIR") of false -> "/tmp"; undefined -> "/tmp"; V -> V end,
    Tmp = filename:join([BaseTmp, "wf_worldstate_cache.json"]),
    {Tmp, Bin}.

capture_output(Fun) ->
    Capturer = spawn(fun() -> io_capture_loop([]) end),
    Old = group_leader(),
    group_leader(Capturer, self()),
    try
        _ = Fun()
    after
        group_leader(Old, self())
    end,
    Capturer ! {get, self()},
    receive
        {captured, Output} -> to_list(Output)
    after 1000 ->
        ""
    end.

io_capture_loop(Acc) ->
    receive
        {io_request, From, ReplyAs, Request} ->
            {NewAcc, Reply} = handle_io_request(Request, Acc),
            From ! {io_reply, ReplyAs, Reply},
            io_capture_loop(NewAcc);
        {get, Requestor} ->
            Requestor ! {captured, unicode:characters_to_list(lists:reverse(Acc))}
    end.

handle_io_request({put_chars, Chars}, Acc) ->
    %% Preserve the error in CT's log if the command halts the VM.
    io:put_chars(Chars),
    {[Chars | Acc], ok};
handle_io_request({put_chars, _Enc, Chars}, Acc) ->
    handle_io_request({put_chars, Chars}, Acc);
handle_io_request({put_chars, Enc, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, Enc, apply(Mod, Fun, Args)}, Acc);
handle_io_request({put_chars, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, apply(Mod, Fun, Args)}, Acc);
handle_io_request({format, Format, Args}, Acc) ->
    handle_io_request({put_chars, io_lib:format(Format, Args)}, Acc);
handle_io_request({fwrite, Format, Args}, Acc) ->
    handle_io_request({put_chars, io_lib:format(Format, Args)}, Acc);
handle_io_request({requests, Reqs}, Acc) when is_list(Reqs) ->
    lists:foldl(
      fun(Req, {Acc0, _Reply0}) -> handle_io_request(Req, Acc0) end,
      {Acc, ok},
      Reqs);
handle_io_request(_Req, Acc) ->
    {Acc, ok}.

to_list(V) when is_binary(V) -> binary_to_list(V);
to_list(V) when is_atom(V) -> atom_to_list(V);
to_list(V) when is_list(V) -> V;
to_list(V) -> io_lib:format("~p", [V]).

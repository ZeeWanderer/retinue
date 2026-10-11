%%%-------------------------------------------------------------------
%% EUnit tests for knowledge query behavior.
%%%-------------------------------------------------------------------
-module(wfcli_knowledge_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

codex_hides_excluded_entries_by_default_test() ->
    with_local_backend(fun() ->
        {ok, _Parsed, Results} = query_command(
          "codex", ["--exports-dir", fixture_exports(), "codex entry"]),
        ?assertEqual(["Visible Codex Entry"], result_names(Results))
    end).

codex_can_include_excluded_entries_test() ->
    with_local_backend(fun() ->
        {ok, _Parsed, Results} = query_command(
          "codex", ["--exports-dir", fixture_exports(), "--include-excluded", "codex entry"]),
        ?assertEqual(["Excluded Codex Entry", "Visible Codex Entry"], result_names(Results))
    end).

enemy_numeric_and_faction_filters_test() ->
    with_local_backend(fun() ->
        {ok, _Parsed, Results} = query_command(
          "enemies", ["--knowledge-dir", fixture_knowledge(), "faction=Grineer", "armor>=50"]),
        ?assertEqual(["Test Lancer"], result_names(Results)),
        ?assertEqual("fixture-sha256", maps:get(version, maps:get(source_meta, Results)))
    end).

drop_reverse_query_test() ->
    with_local_backend(fun() ->
        {ok, _Parsed, Results} = query_command(
          "drops", ["--knowledge-dir", fixture_knowledge(), "test mod"]),
        [Entry] = maps:get(slice, Results),
        ?assertEqual("Test Lancer", maps:get(enemy, maps:get(data, Entry)))
    end).

drop_boolean_query_test() ->
    with_local_backend(fun() ->
        {ok, _Parsed, Results} = query_command(
          "drops", ["--knowledge-dir", fixture_knowledge(),
                    "item~Test Mod OR item~Test Resource"]),
        ?assertEqual(2, maps:get(total, Results))
    end).

drop_boolean_not_test() ->
    with_local_backend(fun() ->
        {ok, _Parsed, Results} = query_command(
          "drops", ["--knowledge-dir", fixture_knowledge(),
                    "test NOT item~Resource"]),
        [Entry] = maps:get(slice, Results),
        ?assertEqual("Test Mod", maps:get(item, maps:get(data, Entry)))
    end).

typed_request_matches_focused_cli_test() ->
    with_local_backend(fun() ->
        Args = ["--exports-dir", fixture_exports(), "visible OR excluded"],
        FocusedQuery = wfcli_test_cli:catalog("codex", Args),
        {ok, _FocusedPrepared, FocusedResults} =
            wfcli_knowledge_query:query("codex", FocusedQuery),
        {ok, Ast} = wfcli_query_parse:parse_arguments(["visible", "OR", "excluded"]),
        Typed = #{query_ast => Ast, exports_dir => fixture_exports()},
        {ok, _TypedPrepared, TypedResults} = wfcli_knowledge_query:query("codex", Typed),
        ?assertEqual(result_names(FocusedResults), result_names(TypedResults))
    end).

query_command(Command, Args) ->
    Query = wfcli_test_cli:catalog(Command, Args),
    wfcli_knowledge_query:query(Command, Query).

with_local_backend(Fun) ->
    Previous = application:get_env(retinue_cli, use_daemon),
    application:set_env(retinue_cli, use_daemon, false),
    try Fun()
    after
        case Previous of
            {ok, Value} -> application:set_env(retinue_cli, use_daemon, Value);
            undefined -> application:unset_env(retinue_cli, use_daemon)
        end
    end.

result_names(Results) ->
    [maps:get(name, E) || E <- maps:get(slice, Results)].

fixture_exports() -> filename:join(["apps", "retinue_cli", "test", "fixtures", "exports"]).
fixture_knowledge() -> filename:join(["apps", "retinue_cli", "test", "fixtures", "knowledge"]).

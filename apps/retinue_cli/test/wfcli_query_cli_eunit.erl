%%%-------------------------------------------------------------------
%% EUnit tests for query CLI arg parsing.
%%%-------------------------------------------------------------------
-module(wfcli_query_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

query_has_no_default_limit_test() ->
    ?assertNot(maps:is_key(limit, wfcli_test_cli:parse(["query", "braton"]))).

explicit_limit_is_parsed_test() ->
    ?assertEqual(7, maps:get(limit, wfcli_test_cli:parse(["query", "--limit", "7", "test"]))).

limit_rejects_trailing_garbage_test() ->
    ?assertMatch({error, _, _, _}, wfcli_test_cli:result(["query", "--limit", "7x", "test"])).

daemon_contract_tracks_market_source_test() ->
    ?assertEqual(1, retinue_protocol:handshake_version()),
    ?assertEqual(1, maps:get(market, retinue_protocol:interfaces())),
    ?assertEqual(wfcli_market_service, wfcli_protocol:owner(#{source => market})).

worldstate_pagination_uses_explicit_limit_test() ->
    {ok, Result} = wfcli_query_service:paginate_worldstate(
                     {ok, #{entries => [first, second, third]}},
                     #{offset => 1, limit => 1}),
    ?assertEqual([second], maps:get(entries, Result)).

worldstate_pagination_is_unlimited_by_default_test() ->
    {ok, Result} = wfcli_query_service:paginate_worldstate(
                     {ok, #{entries => [first, second, third]}}, #{}),
    ?assertEqual([first, second, third], maps:get(entries, Result)).

dataset_selector_defaults_to_public_sources_test() ->
    {ok, ["braton"], Datasets, false} = wfcli_query_service:select_datasets(["braton"]),
    ?assertEqual([worldstate, mods, items, codex, enemies, drops], Datasets).

dataset_selector_is_removed_from_quoted_query_test() ->
    {ok, ["serration"], Datasets, true} =
        wfcli_query_service:select_datasets(["dataset=codex|drops serration"]),
    ?assertEqual([codex, drops], Datasets).

dataset_selector_accepts_all_test() ->
    {ok, [], Datasets, true} = wfcli_query_service:select_datasets(["dataset:all"]),
    ?assertEqual([worldstate, mods, items, codex, enemies, drops, player, market,
                  diagnostics, incidents, captures], Datasets).

dataset_selector_accepts_player_test() ->
    {ok, [], [player], true} = wfcli_query_service:select_datasets(["dataset=player"]).

dataset_selector_accepts_market_test() ->
    {ok, [], [market], true} = wfcli_query_service:select_datasets(["dataset=market"]).

dataset_selector_accepts_diagnostics_test() ->
    {ok, [], [diagnostics], true} =
        wfcli_query_service:select_datasets(["dataset=diagnostics"]).

dataset_selector_accepts_default_plus_optional_test() ->
    {ok, [], Datasets, true} = wfcli_query_service:select_datasets(["dataset=default|drops"]),
    ?assertEqual([worldstate, mods, items, codex, enemies, drops], Datasets).

dataset_selector_rejects_unknown_test() ->
    ?assertMatch({error, _}, wfcli_query_service:select_datasets(["dataset=fish"])).

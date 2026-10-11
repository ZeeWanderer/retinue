%%%-------------------------------------------------------------------
%% EUnit tests for typed catalog daemon replies.
%%%-------------------------------------------------------------------
-module(wfcli_catalog_query_eunit).

-include_lib("eunit/include/eunit.hrl").

export_reply_returns_daemon_prepared_query_test() ->
    Prepared = #{query => {term, "toxin"}, compiled_sort => []},
    Results = #{total => 1},
    ?assertEqual({ok, Prepared, Results},
      wfcli_exports_query:decode_daemon_reply(
        {ok, #{command => "mods", query => Prepared, results => Results}})).

knowledge_reply_returns_daemon_prepared_query_test() ->
    Prepared = #{query => {term, "toxin"}, compiled_sort => []},
    Results = #{total => 1},
    ?assertEqual({ok, Prepared, Results},
      wfcli_knowledge_query:decode_daemon_reply(
        {ok, #{command => "codex", query => Prepared, results => Results}})).

pagination_bounds_result_payload_test() ->
    Entries = [wfcli_entity_exports:build_item(#{name => "Item", uniqueName => integer_to_list(N)}, #{})
               || N <- lists:seq(1, 1000)],
    Result = wfcli_entity_query:execute(Entries, match_all, [], wfcli_entity_exports, item, 10, 1),
    ?assertEqual(1000, maps:get(total, Result)),
    ?assertEqual(1, maps:get(shown, Result)),
    ?assertEqual([lists:nth(11, Entries)], maps:get(slice, Result)),
    ?assertNot(maps:is_key(all, Result)),
    ?assert(byte_size(retinue_json:encode(Result)) < 4096),
    Empty = wfcli_entity_query:execute(Entries, match_all, [], wfcli_entity_exports, item, 1000, 1),
    ?assertEqual(1000, maps:get(total, Empty)),
    ?assertEqual([], maps:get(slice, Empty)).

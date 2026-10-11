%%%-------------------------------------------------------------------
%% EUnit tests for query sort parsing.
%%%-------------------------------------------------------------------
-module(wfcli_query_sort_eunit).

-include_lib("eunit/include/eunit.hrl").

parse_default_sort_test() ->
    Spec = wfcli_query_sort:parse("expiry"),
    ?assertEqual("expiry", maps:get(key, Spec)),
    ?assertEqual(asc, maps:get(dir, Spec)).

parse_desc_prefix_test() ->
    Spec = wfcli_query_sort:parse("-window"),
    ?assertEqual("window", maps:get(key, Spec)),
    ?assertEqual(desc, maps:get(dir, Spec)).

parse_suffix_test() ->
    Spec = wfcli_query_sort:parse("name:desc"),
    ?assertEqual("name", maps:get(key, Spec)),
    ?assertEqual(desc, maps:get(dir, Spec)).

compare_direction_test() ->
    ?assert(wfcli_query_sort:compare(asc, "a", "b")),
    ?assert(wfcli_query_sort:compare(desc, "b", "a")).

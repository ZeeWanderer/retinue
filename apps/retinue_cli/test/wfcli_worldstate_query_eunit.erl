%%%-------------------------------------------------------------------
%% EUnit tests for worldstate query parsing/matching.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_query_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("retinue_daemon/include/wfcli_worldstate.hrl").

parse_and_match_test() ->
    Entry = #{
        type => alert,
        id => "alert-1",
        name => "Test Alert",
        data => #{<<"MissionInfo">> => #{<<"missionReward">> => #{<<"asString">> => <<"Reward">>}}},
        haystack => "test alert reward"
    },
    Parsed = wfcli_worldstate_query:parse("type=alert data.MissionInfo.missionReward.asString~Reward"),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed)).

sort_token_parsed_test() ->
    Parsed = wfcli_worldstate_query:parse("sort=-expiry"),
    [Spec] = maps:get(sort, Parsed, []),
    ?assertEqual("expiry", maps:get(key, Spec)),
    ?assertEqual(desc, maps:get(dir, Spec)).

extract_path_test() ->
    Entry = #{
        type => alert,
        id => "alert-1",
        name => "Test Alert",
        data => #{<<"MissionInfo">> => #{<<"missionReward">> => #{<<"asString">> => <<"Reward">>}}}
    },
    Parsed = wfcli_worldstate_query:parse("extract=data.MissionInfo.missionReward.asString"),
    Extracts = wfcli_worldstate_query:extract(Entry, maps:get(extracts, Parsed, [])),
    ?assertEqual([{"MissionInfo.missionReward.asString", "Reward"}], Extracts).

text_or_group_test() ->
    Entry = #{haystack => "foo bar"},
    Parsed = wfcli_worldstate_query:parse("foo OR baz"),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed)).

boolean_precedence_test() ->
    Entry = #{haystack => "foo"},
    Parsed = wfcli_worldstate_query:parse("foo OR bar baz"),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed)),
    Grouped = wfcli_worldstate_query:parse("(foo OR bar) baz"),
    ?assert(not wfcli_worldstate_query:match(Entry, Grouped)).

numeric_filters_test() ->
    Entry = #{
        data => #{<<"Foo">> => <<"10">>},
        haystack => ""
    },
    Parsed1 = wfcli_worldstate_query:parse("data.Foo>=9"),
    Parsed2 = wfcli_worldstate_query:parse("data.Foo>10"),
    Parsed3 = wfcli_worldstate_query:parse("data.Foo<=10"),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed1)),
    ?assert(not wfcli_worldstate_query:match(Entry, Parsed2)),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed3)).

neq_missing_key_test() ->
    Entry = #{data => #{}, haystack => ""},
    Parsed = wfcli_worldstate_query:parse("data.Missing!=foo"),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed)).

raw_root_exposes_unparsed_absolute_paths_test() ->
    Raw = #{<<"FutureSection">> => [#{<<"kind">> => <<"new">>, <<"value">> => 42}]},
    Ws = #ws{raw = Raw, opts = #{}},
    Parsed = wfcli_worldstate_query:parse(
               "type=raw_worldstate data.FutureSection.kind=new "
               "extract=data.FutureSection.*.value"),
    [Entry] = wfcli_worldstate_results:query_parsed(Ws, Parsed, undefined),
    ?assertEqual(raw_worldstate, maps:get(type, Entry)),
    ?assertEqual([{"FutureSection.*.value", "42"}],
                 wfcli_worldstate_query:extract(Entry, maps:get(extracts, Parsed))).

raw_root_is_not_added_to_control_only_queries_test() ->
    Ws = #ws{raw = #{<<"FutureSection">> => #{<<"value">> => 42}}, opts = #{}},
    Parsed = wfcli_worldstate_query:parse("sort=type"),
    ?assertEqual([], wfcli_worldstate_results:query_parsed(Ws, Parsed, undefined)).

fuzz_parse_does_not_crash_test() ->
    _ = rand:seed(exsplus, {3, 4, 5}),
    lists:foreach(
      fun(_Idx) ->
          Token = random_token(),
          Parsed = wfcli_worldstate_query:parse(Token),
          ?assert(is_map(Parsed))
      end,
      lists:seq(1, 50)).

fuzz_extract_paths_test() ->
    _ = rand:seed(exsplus, {6, 7, 8}),
    Entry = #{
        data => #{
            <<"Foo">> => #{<<"Bar">> => [1, 2, 3]},
            <<"List">> => [#{<<"Val">> => "a"}, #{<<"Val">> => "b"}]
        }
    },
    lists:foreach(
      fun(_Idx) ->
          Path = random_extract_path(),
          Parsed = wfcli_worldstate_query:parse("extract=" ++ Path),
          Extracts = wfcli_worldstate_query:extract(Entry, maps:get(extracts, Parsed, [])),
          ?assert(is_list(Extracts))
      end,
      lists:seq(1, 50)).

random_token() ->
    Keys = ["type", "name", "data.Foo", "data.List.Val", "extract"],
    Ops = ["=", "!=", ">=", "<=", ">", "<", "~", ":"],
    Key = rand_pick(Keys),
    Op = rand_pick(Ops),
    Val = rand_pick(["alert", "foo", "10", "alpha|beta", "data.Foo"]),
    Key ++ Op ++ Val.

random_extract_path() ->
    Segs = ["Foo", "Bar", "List", "Val", "*", "0", "1"],
    Parts = [rand_pick(Segs) || _ <- lists:seq(1, rand_int(2, 5))],
    string:join(Parts, ".").

rand_pick(List) ->
    lists:nth(rand_int(1, length(List)), List).

rand_int(Min, Max) ->
    Min + rand:uniform(Max - Min + 1) - 1.

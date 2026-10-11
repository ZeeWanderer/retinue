%%%-------------------------------------------------------------------
%% EUnit tests for suggestion helpers.
%%%-------------------------------------------------------------------
-module(wfcli_cli_suggest_eunit).

-include_lib("eunit/include/eunit.hrl").

suggests_close_match_test() ->
    Suggest = retinue_cli_suggest:suggest("modz", ["mods", "items", "alerts"]),
    ?assert(string:find(Suggest, "mods") =/= nomatch).

suggest_match_returns_best_test() ->
    {ok, Best} = retinue_cli_suggest:suggest_match("modz", ["mods", "items", "alerts"]),
    ?assertEqual("mods", Best).

no_suggestion_for_distant_test() ->
    Suggest = retinue_cli_suggest:suggest("xyz", ["mods", "items", "alerts"]),
    ?assertEqual("", Suggest).

suggest_fuzzy_variants_test() ->
    Candidates = ["mods", "items", "alerts", "worldstate", "exports"],
    Variants = [
        {"mod", "mods"},
        {"modes", "mods"},
        {"itmes", "items"},
        {"alerst", "alerts"},
        {"worldstte", "worldstate"},
        {"export", "exports"}
    ],
    lists:foreach(
      fun({Input, Expect}) ->
          Suggest = retinue_cli_suggest:suggest(Input, Candidates),
          ?assert(string:find(Suggest, Expect) =/= nomatch)
      end,
      Variants).

suggest_fuzz_does_not_crash_test() ->
    Candidates = ["mods", "items", "alerts"],
    _ = rand:seed(exsplus, {7, 8, 9}),
    lists:foreach(
      fun(_Idx) ->
          Str = random_flag(),
          Suggest = retinue_cli_suggest:suggest(Str, Candidates),
          ?assert(is_list(Suggest))
      end,
      lists:seq(1, 50)).

suggest_near_miss_always_returns_hint_test() ->
    Candidates = ["mods", "items", "alerts"],
    Inputs = ["mod", "m0ds", "itmes", "alets", "alert"],
    lists:foreach(
      fun(Input) ->
          Suggest = retinue_cli_suggest:suggest(Input, Candidates),
          ?assert(string:find(Suggest, "did you mean") =/= nomatch)
      end,
      Inputs).

random_flag() ->
    Len = rand:uniform(8),
    lists:flatten([rand_pick() || _ <- lists:seq(1, Len)]).

rand_pick() ->
    lists:nth(rand:uniform(6), ["-", "_", "x", "?", "!", "a"]).

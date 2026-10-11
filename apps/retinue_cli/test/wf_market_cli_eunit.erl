-module(wf_market_cli_eunit).
-include_lib("eunit/include/eunit.hrl").

market_has_no_silent_default_limit_test() ->
    ?assertNot(maps:is_key(limit, wf_test_cli:parse(["market", "saryn"]))).

market_limit_and_ttl_are_explicit_test() ->
    Args = wf_test_cli:parse(["market", "--limit", "5", "--ttl", "120", "saryn"]),
    ?assertEqual(5, maps:get(limit, Args)),
    ?assertEqual(120, maps:get(ttl, Args)),
    ?assertEqual(["saryn"], maps:get(query_tokens, Args)).

market_rejects_bad_limits_test_() ->
    [?_assertMatch({error, _, _, _}, wf_test_cli:result(["market", "--limit", Value]))
     || Value <- ["many", "101", "-1", "1x"]].

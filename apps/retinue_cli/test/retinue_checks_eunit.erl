-module(retinue_checks_eunit).
-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

unavailable_ledger_does_not_crash_daemon_test() ->
    ?assertEqual(undefined, whereis(retinue_checks)),
    ?assertMatch({reply, {error, {checks_unavailable, _}}, #{}},
                 retinue_daemon:handle_call(diagnostic_checks, unused, #{})).

deduplication_recovery_and_version_test() ->
    Ctx = context(<<"0.2.1">>),
    One = merge(#{}, Ctx, fail, 1000),
    ?assertEqual(One, merge(One, Ctx, fail, 1000)),
    Two = merge(One, Ctx, fail, 2000),
    [Failure] = maps:values(Two),
    ?assertEqual(2, maps:get(<<"failed_observations">>, Failure)),
    ?assertEqual(<<"open">>, retinue_checks:effective_state(Failure, <<"0.2.1">>)),
    ?assertEqual(<<"unverified">>, retinue_checks:effective_state(Failure, <<"0.3.0">>)),
    Fixed = merge(Two, Ctx, pass, 3000),
    [Resolved] = maps:values(Fixed),
    ?assertEqual(<<"resolved">>, maps:get(<<"state">>, Resolved)),
    ?assertEqual(Fixed, merge(Fixed, Ctx, pass, 4000)),
    ?assertEqual(Fixed, merge(Fixed, context(<<"older">>), fail, 1500)),
    [Regression] = maps:values(merge(Fixed, context(<<"0.3.0">>), fail, 5000)),
    ?assertEqual(1000, maps:get(<<"first_failed_at">>, Regression)),
    ?assertEqual(Ctx, maps:get(<<"first_failure_context">>, Regression)),
    ?assertEqual(3, maps:get(<<"failed_observations">>, Regression)).

coalesced_failure_and_recovery_test() ->
    Ctx = context(<<"0.2.1">>),
    Healthy = merge(#{}, Ctx, pass, 1000),
    Check = (evidence(pass, 3000))#{<<"last_failure">> => #{
        <<"checked_at">> => 2000, <<"error">> => <<"brief failure">>}},
    Result = retinue_checks:merge(Healthy, <<"warframe.companion">>, Ctx, #{<<"reader">> => Check}, 4000),
    [Recovered] = maps:values(Result),
    ?assertEqual(<<"resolved">>, maps:get(<<"state">>, Recovered)),
    ?assertEqual(2000, maps:get(<<"first_failed_at">>, Recovered)),
    ?assertEqual(3000, maps:get(<<"last_success_at">>, Recovered)),
    ?assertEqual(Result, retinue_checks:merge(Result, <<"warframe.companion">>, Ctx,
        #{<<"reader">> => Check}, 5000)).

invalid_reports_test() ->
    Time = erlang:system_time(millisecond),
    ?assert(retinue_checks:valid_report(context(<<"test">>), #{<<"reader">> => evidence(pass, Time)})),
    ?assertNot(retinue_checks:valid_report(#{<<"session">> => []}, #{})),
    ?assertNot(retinue_checks:valid_report(#{}, #{<<"reader">> => (evidence(pass, Time))#{<<"error">> := <<"error">>}})),
    ?assertNot(retinue_checks:valid_report(#{}, maps:from_list([
        {integer_to_binary(N), evidence(pass, Time)} || N <- lists:seq(1, 65)]))).

retention_and_input_bounds_test() ->
    Ctx = context(<<"0.2.1">>),
    Resolved = merge(merge(#{}, Ctx, fail, 1000), Ctx, pass, 2000),
    ?assertEqual(#{}, retinue_checks:prune(Resolved, 31 * 86400000)),
    ?assertEqual(1, map_size(retinue_checks:prune(merge(#{}, Ctx, fail, 1000), 31 * 86400000))),
    All = lists:foldl(fun(N, Acc) ->
        retinue_checks:merge(Acc, <<"poe2.companion">>, Ctx,
            #{integer_to_binary(N) => evidence(fail, N)}, 100000)
    end, #{}, lists:seq(1, 600)),
    ?assertEqual(512, map_size(retinue_checks:prune(All, 100000))),
    ?assertEqual(#{}, retinue_checks:merge(#{}, <<"x">>, Ctx,
        #{<<"unknown">> => #{<<"state">> => <<"pass">>}, <<"future">> => evidence(fail, 10000)}, 1)).

healthy_runs_do_not_write_test_() ->
    {setup, fun setup/0, fun cleanup/1, fun(Path) -> fun() ->
        Time = erlang:system_time(millisecond),
        retinue_checks:observe(<<"warframe.companion">>, context(<<"0.2.1">>),
                               #{<<"reader">> => evidence(pass, Time)}),
        #{checks := [_]} = retinue_checks:report(),
        gen_server:stop(retinue_checks),
        ?assertNot(filelib:is_file(Path)),
        {ok, _} = retinue_checks:start_link(),
        retinue_checks:observe(<<"warframe.companion">>, context(<<"0.2.1">>),
                               #{<<"reader">> => evidence(fail, Time + 1)}),
        #{checks := [_]} = retinue_checks:report(),
        gen_server:stop(retinue_checks),
        {ok, Info} = file:read_file_info(Path),
        ?assertEqual(8#600, Info#file_info.mode band 8#777),
        ?assertMatch({ok, Checks} when map_size(Checks) =:= 1, retinue_checks:read()),
        {ok, _} = retinue_checks:start_link(),
        retinue_checks:observe(<<"warframe.companion">>, context(<<"0.2.1">>),
                               #{<<"reader">> => evidence(pass, Time + 2)}),
        #{checks := [Recovered]} = retinue_checks:report(),
        ?assertEqual(<<"resolved">>, maps:get(<<"last_result">>, Recovered)),
        gen_server:stop(retinue_checks),
        {ok, Saved} = file:read_file(Path),
        {ok, _} = retinue_checks:start_link(),
        lists:foreach(fun(N) ->
            retinue_checks:observe(<<"warframe.companion">>, context(<<"0.2.1">>),
                                   #{<<"reader">> => evidence(pass, Time + N + 3)})
        end, lists:seq(1, 1000)),
        #{checks := [_]} = retinue_checks:report(),
        ?assertEqual(false, maps:get(dirty, sys:get_state(retinue_checks))),
        gen_server:stop(retinue_checks),
        ?assertEqual({ok, Saved}, file:read_file(Path))
    end end}.

corrupt_store_is_reported_test_() ->
    {setup, fun setup/0, fun cleanup/1, fun(Path) -> fun() ->
        gen_server:stop(retinue_checks),
        ok = filelib:ensure_dir(Path),
        ok = file:write_file(Path, <<"{\"schema\":1,\"checks\":[{}]}">>),
        ?assertEqual({error, invalid_check_store}, retinue_checks:read()),
        {ok, _} = retinue_checks:start_link(),
        ?assertMatch(#{storage_error := invalid_check_store}, retinue_checks:report())
    end end}.

setup() ->
    Path = "/tmp/retinue-checks-" ++ integer_to_list(erlang:unique_integer([positive])) ++ "/checks.json",
    application:set_env(retinue_daemon, checks_file, Path),
    {ok, _} = retinue_checks:start_link(), Path.
cleanup(Path) ->
    case whereis(retinue_checks) of undefined -> ok; _ -> gen_server:stop(retinue_checks) end,
    application:unset_env(retinue_daemon, checks_file),
    file:del_dir_r(filename:dirname(Path)).
context(Version) -> #{<<"version">> => Version, <<"session">> => <<"test:1">>, <<"game_build">> => null}.
evidence(State, Time) -> #{<<"state">> => atom_to_binary(State), <<"checked_at">> => Time,
    <<"error">> => case State of fail -> <<"anchor missing">>; pass -> null end}.
merge(Checks, Ctx, State, Time) ->
    retinue_checks:merge(Checks, <<"warframe.companion">>, Ctx, #{<<"reader">> => evidence(State, Time)}, Time).

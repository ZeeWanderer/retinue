%%%-------------------------------------------------------------------
%% EUnit tests for shared daemon activity and idle shutdown.
%%%-------------------------------------------------------------------
-module(retinue_activity_eunit).

-include_lib("eunit/include/eunit.hrl").

initial_idle_timeout_notifies_test() ->
    with_activity(#{daemon_idle_shutdown => true, daemon_idle_timeout_ms => 20},
                  false, fun await_idle/1).

initial_policy_uses_retinue_environment_test_() ->
    [fun() ->
         with_activity(#{daemon_idle_shutdown => Configured}, Environment,
                       fun(_Pid) ->
                           ?assertMatch(#{idle_policy := Expected}, retinue_activity:status())
                       end)
     end || {Configured, Environment, Expected} <-
                [{true, "persistent", persistent}, {true, false, idle},
                 {false, false, persistent}]].

runtime_idle_policy_test() ->
    with_activity(fun(Pid) ->
        ok = retinue_activity:set_idle_policy({idle, 20}),
        ?assertMatch(#{idle_policy := idle, idle_timeout_ms := 20}, retinue_activity:status()),
        ok = retinue_activity:set_idle_policy(persistent),
        ?assertMatch(#{idle_policy := persistent}, retinue_activity:status()),
        assert_no_idle(Pid, 60),
        ok = retinue_activity:set_idle_policy(idle),
        ?assertMatch(#{idle_policy := idle, idle_timeout_ms := 200}, retinue_activity:status()),
        await_idle(Pid)
    end).

invalid_policy_keeps_state_test() ->
    with_activity(fun(_Pid) ->
        ok = retinue_activity:activity_start(),
        State = sys:get_state(retinue_activity),
        lists:foreach(fun(Policy) ->
            ?assertEqual({error, {invalid_idle_policy, Policy}},
                         retinue_activity:set_idle_policy(Policy)),
            ?assertEqual(State, sys:get_state(retinue_activity))
        end, [invalid, {idle, 0}, {idle, -1}, {idle, 1.5}, {idle, infinity}])
    end).

nested_activity_is_counted_per_owner_test() ->
    with_activity(fun(Pid) ->
        ok = retinue_activity:activity_start(),
        ok = retinue_activity:activity_start(),
        ?assertMatch(#{external_activity := 2}, retinue_activity:status()),
        ?assertMatch({monitors, [{process, _}]}, process_info(Pid, monitors)),
        ok = retinue_activity:set_idle_policy({idle, 20}),
        ok = retinue_activity:activity_end(),
        ?assertMatch(#{external_activity := 1}, retinue_activity:status()),
        assert_no_idle(Pid, 60),
        ok = retinue_activity:set_idle_policy(persistent),
        ok = retinue_activity:set_idle_policy({idle, 20}),
        ?assertMatch(#{external_activity := 1}, retinue_activity:status()),
        assert_no_idle(Pid, 60),
        ok = retinue_activity:activity_end(),
        ?assertMatch(#{external_activity := 0}, retinue_activity:status()),
        ?assertEqual({monitors, []}, process_info(Pid, monitors)),
        await_idle(Pid)
    end).

unmatched_release_cannot_release_another_owner_test() ->
    with_activity(fun(_Pid) ->
        Owner = start_owner(2),
        try
            ok = retinue_activity:activity_end(),
            ?assertMatch(#{external_activity := 2}, retinue_activity:status()),
            ok = retinue_activity:activity_start(),
            ?assertMatch(#{external_activity := 3}, retinue_activity:status()),
            ok = retinue_activity:activity_end(),
            ok = retinue_activity:activity_end(),
            ?assertMatch(#{external_activity := 2}, retinue_activity:status())
        after
            exit(Owner, kill)
        end
    end).

owner_down_releases_all_nested_holds_test() ->
    with_activity(fun(Pid) ->
        Owner = start_owner(3),
        try
            ok = retinue_activity:activity_start(),
            ok = retinue_activity:set_idle_policy({idle, 20}),
            exit(Owner, kill),
            await_count(1, 100),
            assert_no_idle(Pid, 60),
            ok = retinue_activity:activity_end(),
            await_idle(Pid),
            ?assertMatch(#{external_activity := 0}, retinue_activity:status())
        after
            exit(Owner, kill)
        end
    end).

last_owner_down_arms_idle_test() ->
    with_activity(fun(Pid) ->
        Owner = start_owner(2),
        try
            ok = retinue_activity:set_idle_policy({idle, 20}),
            assert_no_idle(Pid, 60),
            exit(Owner, kill),
            await_idle(Pid),
            ?assertMatch(#{external_activity := 0}, retinue_activity:status())
        after
            exit(Owner, kill)
        end
    end).

canceled_timeouts_are_ignored_test() ->
    with_activity(fun(Pid) ->
        ok = retinue_activity:set_idle_policy({idle, 2000}),
        #{idle_timer := OldTimer} = sys:get_state(Pid),
        ok = retinue_activity:activity_start(),
        ?assertEqual(false, erlang:read_timer(OldTimer)),
        Pid ! {timeout, OldTimer, idle},
        ?assertMatch(#{idle_timer := undefined}, sys:get_state(Pid)),
        assert_no_idle(Pid, 0),
        ok = retinue_activity:activity_end(),
        #{idle_timer := NewTimer} = sys:get_state(Pid),
        ?assertNotEqual(OldTimer, NewTimer),
        Pid ! {timeout, OldTimer, idle},
        ?assertMatch(#{idle_timer := NewTimer}, sys:get_state(Pid)),
        assert_no_idle(Pid, 0),
        ok = retinue_activity:set_idle_policy(persistent),
        Pid ! {timeout, NewTimer, idle},
        ?assertMatch(#{idle_timer := undefined}, sys:get_state(Pid)),
        assert_no_idle(Pid, 0)
    end).

idle_reads_and_unmatched_releases_do_not_reset_timer_test() ->
    with_activity(fun(Pid) ->
        ok = retinue_activity:set_idle_policy({idle, 2000}),
        #{idle_timer := Timer} = sys:get_state(Pid),
        _ = retinue_activity:status(),
        ok = retinue_activity:activity_end(),
        Pid ! {'DOWN', make_ref(), process, self(), normal},
        ?assertMatch(#{idle_timer := Timer}, sys:get_state(Pid))
    end).

code_change_preserves_holds_and_policy_test() ->
    with_activity(fun(Pid) ->
        ok = retinue_activity:activity_start(),
        ok = retinue_activity:set_idle_policy({idle, 50}),
        State = sys:get_state(Pid),
        ?assertEqual({ok, State}, retinue_activity:code_change(old, State, hot_update))
    end).

start_owner(Count) ->
    Parent = self(),
    Owner = spawn(fun() ->
        [ok = retinue_activity:activity_start() || _ <- lists:seq(1, Count)],
        Parent ! {activity_held, self()},
        receive stop -> ok end
    end),
    receive {activity_held, Owner} -> Owner
    after 1000 -> exit(Owner, kill), error(activity_not_held)
    end.

await_count(Expected, Attempts) ->
    case maps:get(external_activity, retinue_activity:status()) of
        Expected -> ok;
        _ when Attempts > 0 -> timer:sleep(10), await_count(Expected, Attempts - 1);
        Actual -> ?assertEqual(Expected, Actual)
    end.

await_idle(Pid) ->
    receive {wfcli_daemon_idle, Pid} -> ok
    after 1000 -> error(idle_timeout_missing)
    end.

assert_no_idle(Pid, Timeout) ->
    receive {wfcli_daemon_idle, Pid} -> error(unexpected_idle_timeout)
    after Timeout -> ok
    end.

with_activity(Fun) ->
    with_activity(#{}, false, Fun).

with_activity(Overrides, Environment, Fun) ->
    Defaults = #{daemon_idle_shutdown => false, daemon_idle_timeout_ms => 200,
                 daemon_idle_notify_pid => self()},
    Saved = [{Key, application:get_env(retinue_daemon, Key)} || Key <- maps:keys(Defaults)],
    SavedEnvironment = os:getenv("RETINUE_DAEMON_IDLE_POLICY"),
    try
        maps:foreach(fun(Key, Value) -> application:set_env(retinue_daemon, Key, Value) end,
                     maps:merge(Defaults, Overrides)),
        set_environment(Environment),
        {ok, Pid} = retinue_activity:start_link(),
        try Fun(Pid)
        after gen_server:stop(Pid)
        end
    after
        lists:foreach(fun
            ({Key, undefined}) -> application:unset_env(retinue_daemon, Key);
            ({Key, {ok, Value}}) -> application:set_env(retinue_daemon, Key, Value)
        end, Saved),
        set_environment(SavedEnvironment)
    end.

set_environment(false) -> os:unsetenv("RETINUE_DAEMON_IDLE_POLICY");
set_environment(Value) -> os:putenv("RETINUE_DAEMON_IDLE_POLICY", Value).

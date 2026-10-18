%%%-------------------------------------------------------------------
%% EUnit coverage for daemon supervisor recovery.
%%%-------------------------------------------------------------------
-module(retinue_supervision_eunit).

-include_lib("eunit/include/eunit.hrl").

registered_processes_are_declared_test() ->
    case application:load(retinue_daemon) of
        ok -> ok;
        {error, {already_loaded, retinue_daemon}} -> ok
    end,
    {ok, Registered} = application:get_key(retinue_daemon, registered),
    ?assertEqual(lists:sort([retinue_sup | declared_descendants(retinue_sup)]),
                 lists:sort(Registered)).

game_worker_failure_is_isolated_test_() ->
    [{atom_to_list(Worker), {timeout, 15, fun() ->
        with_daemon(fun() ->
            Before = tree_snapshot(retinue_sup),
            exit(maps:get(Worker, Before), kill),
            assert_restarted([Worker], Before)
        end)
    end}} || Worker <- [wf_query_service, retinue_poe2_service, retinue_checks]].

game_supervisor_failure_is_isolated_test_() ->
    [{atom_to_list(Supervisor), {timeout, 15, fun() ->
        with_daemon(fun() ->
            Before = tree_snapshot(retinue_sup),
            Restarted = maps:keys(tree_snapshot(Supervisor)),
            exit(maps:get(Supervisor, Before), kill),
            assert_restarted(Restarted, Before)
        end)
    end}} || Supervisor <- [wf_sup, retinue_poe2_sup]].

game_supervisor_kill_waits_for_departing_child_test_() ->
    {timeout, 15, fun() ->
        with_daemon(fun() ->
            Before = tree_snapshot(retinue_sup),
            Restarted = maps:keys(tree_snapshot(wf_sup)),
            Child = maps:get(wf_worldstate_service, Before),
            Parent = maps:get(retinue_games_sup, Before),
            with_blocked_child(Child, fun(Release) ->
                exit(maps:get(wf_sup, Before), kill),
                await_monitor(Child, Parent, 300),
                ?assertEqual(undefined, whereis(wf_sup)),
                ?assertEqual(maps:get(retinue_poe2_service, Before),
                             whereis(retinue_poe2_service)),
                Release(),
                assert_restarted(Restarted, Before)
            end)
        end)
    end}.

departing_child_wait_is_bounded_and_non_destructive_test_() ->
    {timeout, 15, fun() ->
        with_daemon(fun() ->
            Before = tree_snapshot(retinue_sup),
            Restarted = maps:keys(tree_snapshot(wf_sup)),
            Child = maps:get(wf_worldstate_service, Before),
            Parent = maps:get(retinue_games_sup, Before),
            Supervisor = maps:get(wf_sup, Before),
            with_blocked_child(Child, fun(Release) ->
                ok = sys:suspend(Parent),
                try
                    Ref = monitor(process, Supervisor),
                    exit(Supervisor, kill),
                    receive {'DOWN', Ref, process, Supervisor, killed} -> ok
                    after 1000 -> error(supervisor_not_killed)
                    end,
                    ?assertEqual({error, {child_shutdown_timeout,
                                          wf_worldstate_service, Child}},
                                 wf_sup:start_link()),
                    ?assert(is_process_alive(Child)),
                    ?assertEqual(Child, whereis(wf_worldstate_service)),
                    {monitored_by, Monitors} = process_info(Child, monitored_by),
                    ?assertNot(lists:member(self(), Monitors))
                after
                    Release(),
                    ok = sys:resume(Parent)
                end,
                assert_restarted(Restarted, Before)
            end)
        end)
    end}.

foreign_registered_child_is_not_stopped_test() ->
    with_daemon(fun() ->
        ok = supervisor:terminate_child(retinue_games_sup, retinue_poe2_sup),
        {Foreign, Monitor} = spawn_monitor(fun() -> receive stop -> ok end end),
        true = register(retinue_poe2_service, Foreign),
        try
            ?assertMatch({error, {shutdown, {failed_to_start_child, retinue_poe2_service,
                                           {already_started, Foreign}}}},
                         supervisor:restart_child(retinue_games_sup, retinue_poe2_sup)),
            ?assertEqual(Foreign, whereis(retinue_poe2_service)),
            ?assert(is_process_alive(Foreign))
        after
            Foreign ! stop,
            receive {'DOWN', Monitor, process, Foreign, normal} -> ok
            after 1000 -> error(foreign_child_not_stopped)
            end,
            {ok, _} = supervisor:restart_child(retinue_games_sup, retinue_poe2_sup)
        end
    end).

duplicate_subtree_start_leaves_live_children_alone_test() ->
    with_daemon(fun() ->
        Before = tree_snapshot(retinue_sup),
        lists:foreach(fun(Module) ->
            ?assertEqual({error, {already_started, maps:get(Module, Before)}},
                         Module:start_link())
        end, [retinue_games_sup, wf_sup, retinue_poe2_sup]),
        ?assertEqual(Before, tree_snapshot(retinue_sup))
    end).

games_supervisor_kill_preserves_activity_test_() ->
    {timeout, 15, fun() ->
        with_daemon(fun() ->
            Before = tree_snapshot(retinue_sup),
            Restarted = maps:keys(tree_snapshot(retinue_games_sup)) ++
                        [retinue_local_api, retinue_daemon, retinue_checks],
            exit(maps:get(retinue_games_sup, Before), kill),
            assert_restarted(Restarted, Before)
        end)
    end}.

local_api_failure_restarts_facade_and_check_ledger_test_() ->
    {timeout, 15, fun() ->
        with_daemon(fun() ->
            Before = tree_snapshot(retinue_sup),
            exit(maps:get(retinue_local_api, Before), kill),
            assert_restarted([retinue_local_api, retinue_daemon, retinue_checks], Before)
        end)
    end}.

activity_failure_resets_dependents_and_holds_test_() ->
    [{Label, {timeout, 15, fun() ->
        with_daemon(fun() -> activity_failure_resets_dependents_and_holds(Restart) end)
    end}} || {Label, Restart} <-
        [{"crash", fun(Pid) -> exit(Pid, kill) end},
         {"hot update", fun(_Pid) ->
             ok = retinue_hot_update:restart_supervised_child(retinue_activity)
         end}]].

activity_failure_resets_dependents_and_holds(Restart) ->
    Test = self(),
    SavedFetch = application:get_env(retinue_daemon, daemon_worldstate_fetch_fun),
    {ok, PlayerCache} = application:get_env(retinue_daemon, player_cache),
    Cache = filename:join(filename:dirname(PlayerCache), "supervision-worldstate.json"),
    application:set_env(retinue_daemon, daemon_worldstate_fetch_fun,
                        fun() ->
                            Test ! {blocked_fetch, self()},
                            receive finish -> {error, test_complete} end
                        end),
    try
        Request = #{source => worldstate, mode => list, type_filter => alert,
                    opts => #{cache => Cache, refresh => true}},
        {ok, _Ref} = wf_worldstate_service:submit(self(), Request),
        Worker = await_fetch(),
        WorkerMonitor = monitor(process, Worker),
        {ok, _Watch} = wf_worldstate_service:subscribe(
                        self(), Request#{specs => [#{label => "alerts", type_filter => alert}]}),
        ?assertMatch(#{external_activity := 1}, retinue_activity:status()),
        Before = tree_snapshot(retinue_sup),
        Restart(maps:get(retinue_activity, Before)),
        assert_restarted(maps:keys(maps:remove(retinue_sup, Before)), Before),
        receive {'DOWN', WorkerMonitor, process, Worker, _} -> ok
        after 1000 -> error(orphaned_worldstate_fetch)
        end,
        ?assertMatch(#{one_shots := 0, watches := 0, fetching := 0},
                     wf_worldstate_service:status()),
        ?assertMatch(#{external_activity := 0}, retinue_activity:status()),
        NextRequest = Request#{opts := #{cache => Cache ++ ".next", refresh => true}},
        {ok, Ref} = wf_worldstate_service:submit(self(), NextRequest),
        NewWorker = await_fetch(),
        ?assertNotEqual(Worker, NewWorker),
        ?assertMatch(#{external_activity := 1}, retinue_activity:status()),
        NewWorker ! finish,
        receive {retinue_daemon, Ref, {error, test_complete}} -> ok
        after 1000 -> error(worldstate_reply_missing)
        end,
        ?assertMatch(#{one_shots := 0, fetching := 0}, wf_worldstate_service:status()),
        ?assertMatch(#{external_activity := 0}, retinue_activity:status())
    after
        restore_env(daemon_worldstate_fetch_fun, SavedFetch)
    end.

resolution_issue_request_returns_data_directly_test() ->
    with_daemon(fun() ->
        ?assert(is_list(gen_server:call(retinue_daemon, {game, warframe, resolution_issues})))
    end).

declared_descendants(Supervisor) ->
    lists:append([case Spec of
        #{id := Name, type := supervisor, modules := [Module]} ->
            [Name | declared_descendants(Module)];
        #{id := Name, type := worker} -> [Name]
    end || Spec <- Supervisor:child_specs()]).

tree_snapshot(Supervisor) ->
    Pid = whereis(Supervisor),
    ?assert(is_pid(Pid)),
    lists:foldl(fun
        ({Name, Child, supervisor, _}, Acc) when is_pid(Child) ->
            ?assertEqual(Child, whereis(Name)),
            maps:merge(Acc, tree_snapshot(Name));
        ({Name, Child, worker, _}, Acc) ->
            ?assert(is_pid(Child)),
            ?assertEqual(Child, whereis(Name)),
            Acc#{Name => Child}
    end, #{Supervisor => Pid}, supervisor:which_children(Supervisor)).

assert_restarted(Restarted, Before) ->
    lists:foreach(fun(Name) ->
        _ = await_replacement(Name, maps:get(Name, Before), 300)
    end, Restarted),
    After = tree_snapshot(retinue_sup),
    ?assertEqual(lists:sort(maps:keys(Before)), lists:sort(maps:keys(After))),
    maps:foreach(fun(Name, Old) ->
        New = maps:get(Name, After),
        ?assert(is_process_alive(New)),
        case lists:member(Name, Restarted) of
            true ->
                ?assertNotEqual({Name, Old}, {Name, New}),
                ?assertNot(is_process_alive(Old));
            false -> ?assertEqual({Name, Old}, {Name, New})
        end
    end, Before).

with_daemon(Fun) ->
    Started = whereis(retinue_sup) =:= undefined,
    case Started of true -> ok = retinue_test_daemon:start(); false -> ok end,
    try
        Fun()
    after
        case Started of true -> retinue_test_daemon:stop(); false -> ok end
    end.

with_blocked_child(Child, Fun) ->
    Test = self(),
    Gate = make_ref(),
    {Helper, Monitor} = spawn_monitor(fun() ->
        try sys:replace_state(Child, fun(State) ->
            Test ! {Gate, blocked},
            receive {Gate, release} -> State end
        end, infinity)
        catch exit:_ -> ok
        end
    end),
    Release = fun() -> Child ! {Gate, release} end,
    try
        receive {Gate, blocked} -> ok
        after 1000 -> error(child_not_blocked)
        end,
        Fun(Release)
    after
        Release(),
        receive {'DOWN', Monitor, process, Helper, _} -> ok
        after 1000 -> error(blocked_child_helper_alive)
        end
    end.

await_fetch() ->
    receive {blocked_fetch, Worker} -> Worker
    after 1000 -> error(worldstate_fetch_not_started)
    end.

restore_env(Key, undefined) -> application:unset_env(retinue_daemon, Key);
restore_env(Key, {ok, Value}) -> application:set_env(retinue_daemon, Key, Value).

await_monitor(_Child, _Parent, 0) -> error(child_shutdown_not_monitored);
await_monitor(Child, Parent, Attempts) ->
    ?assert(is_process_alive(Parent)),
    case process_info(Child, monitored_by) of
        {monitored_by, Monitors} ->
            case lists:member(Parent, Monitors) of
                true -> ok;
                false -> timer:sleep(10), await_monitor(Child, Parent, Attempts - 1)
            end;
        undefined -> error(departing_child_died_before_release)
    end.

await_replacement(Name, _Old, 0) -> error({not_restarted, Name});
await_replacement(Name, Old, Attempts) ->
    case whereis(Name) of
        Pid when is_pid(Pid), Pid =/= Old -> Pid;
        _ -> timer:sleep(10), await_replacement(Name, Old, Attempts - 1)
    end.

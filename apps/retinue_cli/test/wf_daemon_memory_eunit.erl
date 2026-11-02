-module(wf_daemon_memory_eunit).

-include_lib("eunit/include/eunit.hrl").

current_build_migration_does_not_load_store_test() ->
    with_root(fun(Root) ->
        with_env([{build_store_file, filename:join(Root, "missing.term")},
                  {build_cache_file, filename:join(Root, "missing-cache.term")}], fun() ->
            lists:foreach(fun(Error) ->
                Store = (wf_build_store:empty())#{goals => #{saved => user_data}},
                State = #{store => Store, store_load_error => Error, dirty => true},
                {{ok, Updated}, Calls} = traced_calls({wf_build_store, load, 0}, fun() ->
                    wf_build_service:code_change(old, State, hot_update)
                end),
                ?assertEqual([], Calls),
                ?assertEqual(Store, maps:get(store, Updated)),
                ?assertEqual(Error, maps:get(store_load_error, Updated)),
                ?assertEqual(true, maps:get(dirty, Updated))
            end, [undefined, recorded_load_failure])
        end)
    end).

legacy_build_migration_keeps_validation_and_saved_groups_test() ->
    with_root(fun(Root) ->
        Path = filename:join(Root, "builds.term"),
        with_env([{build_store_file, Path}, {build_cache_file, Path ++ ".cache"}], fun() ->
            Store = (wf_build_store:empty())#{goals => #{saved => user_data}},
            {{ok, Updated}, Calls} = traced_calls({wf_build_store, load, 0}, fun() ->
                wf_build_service:code_change(old, #{store => Store, dirty => true}, hot_update)
            end),
            ?assertEqual(1, length(Calls)),
            ?assertEqual(undefined, maps:get(store_load_error, Updated)),
            {ok, Saved} = file:read_file(Path),
            ?assertEqual(#{saved => user_data}, maps:get(goals, binary_to_term(Saved))),
            ok = file:write_file(Path, <<"not an external term">>),
            {ok, Failed} = wf_build_service:code_change(old, #{store => Store}, hot_update),
            ?assertMatch({invalid_build_store, _}, maps:get(store_load_error, Failed)),
            ?assertEqual(Store, maps:get(store, Failed))
        end)
    end).

market_migration_only_loads_missing_snapshot_test() ->
    with_root(fun(Root) ->
        Path = filename:join(Root, "market.term"),
        Cached = (wf_market_cache:empty())#{items => [cached_item]},
        ok = file:write_file(Path, term_to_binary(#{version => 4, snapshot => Cached})),
        Current = #{items => [current_item]},
        {{ok, Kept}, NoCalls} = traced_calls({wf_market_cache, load, 1}, fun() ->
            wf_market_service:code_change(old, #{cache_path => Path, snapshot => Current,
                                                 cache_error => retained_error}, hot_update)
        end),
        ?assertEqual([], NoCalls),
        ?assertEqual(Current, maps:get(snapshot, Kept)),
        ?assertEqual(retained_error, maps:get(cache_error, Kept)),
        {{ok, Loaded}, Calls} = traced_calls({wf_market_cache, load, 1}, fun() ->
            wf_market_service:code_change(old, #{cache_path => Path}, hot_update)
        end),
        ?assertEqual(1, length(Calls)),
        ?assertEqual([cached_item], maps:get(items, maps:get(snapshot, Loaded)))
    end).

build_reply_cache_is_bounded_and_expires_without_requests_test() ->
    Store = (wf_build_store:empty())#{goals => #{saved => user_data}},
    {ok, Initial} = wf_build_service:code_change(old,
        #{store => Store, store_load_error => undefined}, hot_update),
    Past = erlang:monotonic_time(millisecond) - 1,
    Seed = Initial#{cache => #{expired => #{expires => Past, data => old}}},
    Full = lists:foldl(fun retain_build_reply/2, Seed, lists:seq(1, 140)),
    Timer = maps:get(cache_timer, Full),
    try
        ?assertEqual(128, map_size(maps:get(cache, Full))),
        ?assertNot(maps:is_key(expired, maps:get(cache, Full))),
        ?assertEqual(lists:seq(13, 140), lists:sort(
            [maps:get(item, maps:get(data, Entry)) || Entry <- maps:values(maps:get(cache, Full))])),
        ?assertEqual(Store, maps:get(store, Full)),
        ?assert(is_integer(erlang:read_timer(Timer))),
        _ = erlang:cancel_timer(Timer),
        Expired = maps:map(fun(_Key, Entry) -> Entry#{expires => Past} end,
                           maps:get(cache, Full)),
        {noreply, Empty} = wf_build_service:handle_info(
            {timeout, Timer, prune_reply_cache}, Full#{cache => Expired}),
        ?assertEqual(#{}, maps:get(cache, Empty)),
        ?assertEqual(undefined, maps:get(cache_timer, Empty)),
        ?assertEqual(Store, maps:get(store, Empty)),
        ?assertEqual({noreply, Empty}, wf_build_service:handle_info(
            {timeout, Timer, prune_reply_cache}, Empty))
    after
        _ = erlang:cancel_timer(Timer)
    end.

build_reply_cache_timer_fires_while_idle_test() ->
    {ok, Initial} = wf_build_service:code_change(old,
        #{store_load_error => undefined}, hot_update),
    Expires = erlang:monotonic_time(millisecond) + 500,
    State = retain_build_reply(1, Initial#{cache => #{soon => #{expires => Expires, data => old}}}),
    Timer = maps:get(cache_timer, State),
    try
        Message = receive {timeout, Timer, prune_reply_cache} = Expiry -> Expiry
                  after 2000 -> error(idle_cache_timer_missing) end,
        {noreply, Pruned} = wf_build_service:handle_info(Message, State),
        try
            ?assertNot(maps:is_key(soon, maps:get(cache, Pruned))),
            ?assertEqual(1, map_size(maps:get(cache, Pruned)))
        after erlang:cancel_timer(maps:get(cache_timer, Pruned)) end
    after erlang:cancel_timer(Timer) end.

build_reply_hit_prunes_expired_entries_and_migration_cancels_timer_test() ->
    {ok, Initial} = wf_build_service:code_change(old,
        #{store_load_error => undefined}, hot_update),
    Request = #{source => overframe, action => list, offset => 0},
    Key = {overframe, wf_overframe_source:module_info(md5), Request},
    Now = erlang:monotonic_time(millisecond),
    State = Initial#{cache => #{Key => #{expires => Now + 60000, data => cached},
                               expired => #{expires => Now - 1, data => old}}},
    {reply, {ok, Ref}, Pruned} = wf_build_service:handle_call(
        {submit, self(), Request}, {self(), make_ref()}, State),
    Timer = maps:get(cache_timer, Pruned),
    try
        receive {retinue_build, Ref, {ok, cached}} -> ok
        after 1000 -> error(cached_reply_missing) end,
        ?assertEqual(1, map_size(maps:get(cache, Pruned))),
        {ok, Migrated} = wf_build_service:code_change(old, Pruned, hot_update),
        ?assertEqual(false, erlang:read_timer(Timer)),
        ?assertEqual(#{}, maps:get(cache, Migrated)),
        ?assertEqual(undefined, maps:get(cache_timer, Migrated))
    after
        _ = erlang:cancel_timer(Timer)
    end.

audit_coalesces_refreshes_without_applying_obsolete_results_test() ->
    with_resolution(fun(Pid) ->
        First = next_audit(),
        State = sys:get_state(Pid),
        Subscription = maps:get(subscription, State),
        PlayerState = sys:get_state(wf_player_service),
        ?assertMatch(#{view := metadata},
                     maps:get(Subscription, maps:get(subscribers, PlayerState))),
        OldWorker = maps:get(audit_worker, State),
        ok = wf_resolution_issues:reconcile(<<"player_metadata">>, [issue(<<"old">>)]),
        ok = wf_resolution_issues:reconcile(<<"assets">>, [issue(<<"asset">>)]),
        lists:foreach(fun(N) ->
            Pid ! {wf_player, Subscription, <<"test">>, #{revision => N}}
        end, lists:seq(1, 100)),
        A = refresh_caller(),
        B = refresh_caller(),
        await(fun() -> map_size(maps:get(audit_waiters, sys:get_state(Pid))) =:= 2 end),
        ?assertEqual(2, maps:get(count, wf_resolution_issues:status())),
        First ! {audit_reply, {ok, [issue(<<"obsolete">>)]}},
        Second = next_audit(),
        ?assertNot(is_process_alive(First)),
        ?assertEqual([<<"old">>], player_issue_ids()),
        Pid ! {audit_result, maps:get(token, OldWorker), {ok, [issue(<<"late">>)]}},
        Pid ! {audit_timeout, maps:get(token, OldWorker)},
        Second ! {audit_reply, {ok, [issue(<<"current">>)]}},
        ?assertEqual(ok, caller_reply(A)),
        ?assertEqual(ok, caller_reply(B)),
        ?assertEqual([<<"current">>], player_issue_ids()),
        ?assertEqual(2, length(wf_resolution_issues:list())),
        ?assertEqual(#{}, maps:get(audit_waiters, sys:get_state(Pid))),
        receive {audit_started, _} -> error(redundant_audit)
        after 20 -> ok end
    end).

audit_failure_preserves_registry_and_recovers_test() ->
    with_resolution(fun(Pid) ->
        finish_initial_audit(Pid),
        Caller = refresh_caller(),
        Worker = next_audit(),
        Worker ! {audit_reply, {error, known_failure}},
        ?assertEqual({error, known_failure}, caller_reply(Caller)),
        ?assertEqual([<<"old">>], player_issue_ids()),
        ?assertEqual(known_failure, maps:get(refresh_error, wf_resolution_issues:status())),
        NextCaller = refresh_caller(),
        NextWorker = next_audit(),
        NextWorker ! {audit_reply, {ok, []}},
        ?assertEqual(ok, caller_reply(NextCaller)),
        ?assertEqual([], player_issue_ids()),
        ?assertEqual(undefined, maps:get(refresh_error, wf_resolution_issues:status()))
    end).

audit_timeout_and_crash_keep_last_good_state_test_() ->
    [?_test(with_resolution(fun(Pid) ->
        finish_initial_audit(Pid),
        Caller = refresh_caller(),
        Worker = next_audit(),
        Current = maps:get(audit_worker, sys:get_state(Pid)),
        Expected = case Mode of
            timeout ->
                Pid ! {audit_timeout, maps:get(token, Current)},
                {error, resolution_audit_timeout};
            crash ->
                exit(Worker, kill),
                {error, {resolution_audit_worker_down, killed}}
        end,
        ?assertEqual(Expected, caller_reply(Caller)),
        ?assertNot(is_process_alive(Worker)),
        ?assertEqual(false, erlang:read_timer(maps:get(timer, Current))),
        ?assertEqual([<<"old">>], player_issue_ids())
    end)) || Mode <- [timeout, crash]].

audit_waiters_are_removed_when_callers_exit_test() ->
    with_resolution(fun(Pid) ->
        finish_initial_audit(Pid),
        {Client, Monitor} = refresh_caller(),
        Worker = next_audit(),
        exit(Client, kill),
        receive {'DOWN', Monitor, process, Client, killed} -> ok
        after 1000 -> error(caller_alive) end,
        await(fun() -> map_size(maps:get(audit_waiters, sys:get_state(Pid))) =:= 0 end),
        Worker ! {audit_reply, {ok, []}},
        await(fun() -> maps:get(audit_worker, sys:get_state(Pid)) =:= undefined end)
    end).

audit_migration_retires_old_worker_and_keeps_waiters_test() ->
    with_resolution(fun(Pid) ->
        finish_initial_audit(Pid),
        Caller = refresh_caller(),
        Worker = next_audit(),
        Current = maps:get(audit_worker, sys:get_state(Pid)),
        ok = sys:suspend(Pid),
        try ok = sys:change_code(Pid, wf_resolution_issues, old, hot_update)
        after ok = sys:resume(Pid) end,
        Replacement = next_audit(),
        await(fun() -> not is_process_alive(Worker) end),
        ?assertEqual(false, erlang:read_timer(maps:get(timer, Current))),
        Pid ! {audit_result, maps:get(token, Current), {ok, [issue(<<"obsolete">>)]}},
        Replacement ! {audit_reply, {ok, []}},
        ?assertEqual(ok, caller_reply(Caller)),
        ?assertEqual([], player_issue_ids())
    end).

audit_waiter_deadline_bounds_continuous_refreshes_test() ->
    with_resolution(fun(Pid) ->
        finish_initial_audit(Pid),
        Caller = refresh_caller(),
        Worker = next_audit(),
        [{Monitor, #{timer := Timer}}] = maps:to_list(maps:get(audit_waiters, sys:get_state(Pid))),
        Pid ! {audit_waiter_timeout, Monitor},
        ?assertEqual({error, resolution_audit_timeout}, caller_reply(Caller)),
        ?assertEqual(false, erlang:read_timer(Timer)),
        ?assertEqual(#{}, maps:get(audit_waiters, sys:get_state(Pid))),
        ?assertEqual([<<"old">>], player_issue_ids()),
        Worker ! {audit_reply, {ok, []}},
        await(fun() -> maps:get(audit_worker, sys:get_state(Pid)) =:= undefined end)
    end).

legacy_audit_subscription_migrates_to_metadata_test() ->
    with_resolution(fun(Pid) ->
        finish_initial_audit(Pid),
        State = sys:get_state(Pid),
        ok = wf_player_service:unsubscribe(maps:get(subscription, State)),
        {ok, FullRef, _Snapshot} = wf_player_service:subscribe(Pid, full),
        _ = sys:replace_state(Pid, fun(Current) ->
            maps:without([subscription_view, audit_worker, audit_pending, audit_waiters],
                         Current#{subscription => FullRef})
        end),
        ok = sys:suspend(Pid),
        try ok = sys:change_code(Pid, wf_resolution_issues, old, hot_update)
        after ok = sys:resume(Pid) end,
        Worker = next_audit(),
        Current = sys:get_state(Pid),
        Subscribers = maps:get(subscribers, sys:get_state(wf_player_service)),
        ?assertNot(maps:is_key(FullRef, Subscribers)),
        ?assertMatch(#{view := metadata}, maps:get(maps:get(subscription, Current), Subscribers)),
        ?assertEqual([<<"old">>], player_issue_ids()),
        Worker ! {audit_reply, {ok, []}},
        await(fun() -> maps:get(audit_worker, sys:get_state(Pid)) =:= undefined end)
    end).

audit_shutdown_stops_worker_and_timer_test() ->
    with_resolution(fun(Pid) ->
        Worker = next_audit(),
        Current = maps:get(audit_worker, sys:get_state(Pid)),
        ok = gen_server:stop(Pid),
        await(fun() -> not is_process_alive(Worker) end),
        ?assertEqual(false, erlang:read_timer(maps:get(timer, Current)))
    end).

periodic_source_check_is_queued_and_coalesced_without_owner_io_test() ->
    {{noreply, Queued}, Calls} = traced_calls({file, read_file, 1}, fun() ->
        wf_source_manager:handle_info(refresh_stale_sources, source_state())
    end),
    ?assertEqual([], Calls),
    [Job] = queue:to_list(maps:get(queue, Queued)),
    ?assertMatch(#{kind := periodic, max_age_seconds := 0, reply := none}, Job),
    Coalesced = lists:foldl(fun(_, State) ->
        {noreply, Next} = wf_source_manager:handle_info(refresh_stale_sources, State), Next
    end, Queued, lists:seq(1, 100)),
    ?assertEqual(1, queue:len(maps:get(queue, Coalesced))).

source_worker_timeout_waits_for_down_before_releasing_slot_test() ->
    with_source_worker(fun(State, Worker, Tag) ->
        Current = maps:get(current, State),
        Token = maps:get(token, Current),
        {noreply, Coalesced} = wf_source_manager:handle_info(refresh_stale_sources, State),
        ?assertEqual(0, queue:len(maps:get(queue, Coalesced))),
        {noreply, TimingOut} = wf_source_manager:handle_info({source_job_timeout, Token}, State),
        ?assertMatch(#{timed_out := true}, maps:get(current, TimingOut)),
        ?assertEqual({noreply, TimingOut}, wf_source_manager:handle_info(
            {source_job_result, Token, {ok, late}}, TimingOut)),
        Down = worker_down(Current),
        {noreply, Finished} = wf_source_manager:handle_info(Down, TimingOut),
        ?assertEqual(undefined, maps:get(current, Finished)),
        ?assertNot(is_process_alive(Worker)),
        ?assertEqual(false, erlang:read_timer(maps:get(timer, Current))),
        receive {Tag, {error, source_job_timeout}} -> ok
        after 1000 -> error(timeout_reply_missing) end,
        ?assertEqual({noreply, Finished}, wf_source_manager:handle_info(
            {source_job_result, Token, {ok, late}}, Finished))
    end).

source_worker_crash_preserves_error_shape_test() ->
    with_source_worker(fun(State, Worker, Tag) ->
        Current = maps:get(current, State),
        exit(Worker, kill),
        {noreply, _Finished} = wf_source_manager:handle_info(worker_down(Current), State),
        receive {Tag, {error, {source_worker_down, killed}}} -> ok
        after 1000 -> error(crash_reply_missing) end
    end).

source_worker_success_and_migration_preserve_result_test() ->
    with_source_worker(fun(State, Worker, Tag) ->
        Current = maps:get(current, State),
        {ok, Migrated} = wf_source_manager:code_change(old, State, hot_update),
        ?assertEqual(Current, maps:get(current, Migrated)),
        Worker ! finish_source,
        Token = maps:get(token, Current),
        Result = receive {source_job_result, Token, Reply} -> Reply
                 after 1000 -> error(source_result_missing) end,
        {noreply, Finished} = wf_source_manager:handle_info(
            {source_job_result, Token, Result}, Migrated),
        receive {Tag, {ok, #{success := true}}} -> ok
        after 1000 -> error(success_reply_missing) end,
        ?assertEqual(undefined, maps:get(current, Finished)),
        ?assertEqual(false, erlang:read_timer(maps:get(timer, Current)))
    end).

source_shutdown_stops_worker_and_timer_test() ->
    with_source_worker(fun(State, Worker, _Tag) ->
        Current = maps:get(current, State),
        ok = wf_source_manager:terminate(normal, State),
        await(fun() -> not is_process_alive(Worker) end),
        ?assertEqual(false, erlang:read_timer(maps:get(timer, Current)))
    end).

retain_build_reply(N, State) ->
    Request = #{source => overframe, action => list, offset => N},
    Key = {request, N},
    Token = make_ref(),
    Monitor = erlang:monitor(process, self()),
    Running = State#{workers => #{Key => #{token => Token, monitor => Monitor}},
                     worker_monitors => #{Monitor => Key},
                     pending => #{Key => #{request => Request, waiters => #{}}}},
    {noreply, Result} = wf_build_service:handle_info(
        {build_result, Key, Token, {ok, #{item => N}}}, Running),
    Result.

with_resolution(Fun) ->
    Parent = self(),
    with_root(fun(Root) ->
        with_env([{player_cache, filename:join(Root, "player.term")},
                  {resolution_issues_file, filename:join(Root, "issues.json")},
                  {resolution_audit_timeout_ms, 60000},
                  {resolution_audit_fun, fun() ->
                      Parent ! {audit_started, self()},
                      receive {audit_reply, Reply} -> Reply
                      after 4000 -> error(test_audit_not_released) end
                  end}], fun() ->
            {ok, Player} = wf_player_service:start_link(),
            try
                {ok, Pid} = wf_resolution_issues:start_link(),
                try Fun(Pid)
                after stop(Pid) end
            after stop(Player) end
        end)
    end).

finish_initial_audit(Pid) ->
    Worker = next_audit(),
    Worker ! {audit_reply, {ok, [issue(<<"old">>)]}},
    await(fun() -> maps:get(audit_worker, sys:get_state(Pid)) =:= undefined end).

next_audit() ->
    receive {audit_started, Pid} -> Pid
    after 1000 -> error(audit_not_started) end.

refresh_caller() ->
    Parent = self(),
    spawn_monitor(fun() -> Parent ! {refresh_reply, self(), wf_resolution_issues:refresh()} end).

caller_reply({Pid, Monitor}) ->
    Reply = receive {refresh_reply, Pid, Result} -> Result
            after 1000 -> error(refresh_reply_missing) end,
    receive {'DOWN', Monitor, process, Pid, normal} -> Reply
    after 1000 -> error(caller_did_not_exit) end.

issue(Identity) -> #{<<"kind">> => <<"metadata">>, <<"identity">> => Identity}.

player_issue_ids() ->
    lists:sort([maps:get(<<"identity">>, I) || I <- wf_resolution_issues:list(),
                 maps:get(<<"scope">>, I) =:= <<"player_metadata">>]).

source_state() ->
    #{queue => queue:new(), current => undefined, client_monitors => #{},
      refresh_timer => undefined, refresh_interval_ms => 0, max_age_seconds => 0}.

with_source_worker(Fun) ->
    Parent = self(),
    with_env([{source_job_timeout_ms, 60000}, {source_update_fun, fun(_Action) ->
        Parent ! {source_started, self()},
        receive finish_source -> ok after 4000 -> error(test_source_not_released) end
    end}], fun() ->
        Tag = make_ref(),
        Job = #{kind => refresh, request => #{selections => [nodes]},
                reply => {call, {self(), Tag}}},
        {noreply, State} = wf_source_manager:handle_info(process_queue,
            (source_state())#{queue => queue:from_list([Job])}),
        Worker = receive {source_started, Pid} -> Pid
                 after 1000 -> error(source_not_started) end,
        try Fun(State, Worker, Tag)
        after wf_source_manager:terminate(normal, State) end
    end).

worker_down(#{worker_monitor := Monitor, pid := Pid}) ->
    receive {'DOWN', Monitor, process, Pid, _} = Message -> Message
    after 1000 -> error(worker_did_not_exit) end.

traced_calls(MFA = {Module, _, _}, Fun) ->
    {module, Module} = code:ensure_loaded(Module),
    Parent = self(), Tag = make_ref(),
    Worker = spawn(fun() ->
        receive run -> Parent ! {Tag, Fun()} end,
        receive finish -> ok end
    end),
    Session = trace:session_create(?MODULE, self(), []),
    try
        1 = trace:process(Session, Worker, true, [call]),
        1 = trace:function(Session, MFA, true, [local]),
        Worker ! run,
        Result = receive {Tag, Value} -> Value after 1000 -> error(traced_call_timeout) end,
        Delivered = trace:delivered(Session, Worker),
        receive {trace_delivered, Worker, Delivered} -> ok
        after 1000 -> error(trace_delivery_timeout) end,
        {Result, trace_messages(Worker, [])}
    after
        trace:session_destroy(Session),
        exit(Worker, kill)
    end.

trace_messages(Worker, Acc) ->
    receive {trace, Worker, call, Call} -> trace_messages(Worker, [Call | Acc])
    after 0 -> lists:reverse(Acc) end.

with_env(Values, Fun) ->
    Saved = [{Key, application:get_env(retinue_daemon, Key)} || {Key, _} <- Values],
    lists:foreach(fun({Key, Value}) -> application:set_env(retinue_daemon, Key, Value) end, Values),
    try Fun()
    after lists:foreach(fun({Key, {ok, Value}}) -> application:set_env(retinue_daemon, Key, Value);
                           ({Key, undefined}) -> application:unset_env(retinue_daemon, Key)
                       end, Saved)
    end.

with_root(Fun) ->
    Root = filename:join("/tmp", "retinue-memory-test-" ++
                        integer_to_list(erlang:unique_integer([positive]))),
    ok = filelib:ensure_dir(filename:join(Root, "placeholder")),
    try Fun(Root)
    after file:del_dir_r(Root) end.

stop(Pid) ->
    case is_process_alive(Pid) of
        true -> gen_server:stop(Pid);
        false -> ok
    end.

await(Predicate) -> await(Predicate, 100).
await(_Predicate, 0) -> error(condition_timeout);
await(Predicate, Remaining) ->
    case Predicate() of
        true -> ok;
        false -> timer:sleep(5), await(Predicate, Remaining - 1)
    end.

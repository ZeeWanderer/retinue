%%%-------------------------------------------------------------------
%% EUnit tests for persistent export query store.
%%%-------------------------------------------------------------------
-module(wf_exports_store_eunit).

-include_lib("eunit/include/eunit.hrl").

queued_query_reuses_parsed_dataset_test() ->
    Started = setup_stores(),
    try
        Request = catalog_request("mods",
          ["--exports-dir", fixture_exports(), "--limit", "5"]),
        Result1 = submit_and_wait(Request),
        Results1 = maps:get(results, Result1),
        ?assert(maps:get(shown, Results1) > 0),
        ?assert(length(maps:get(slice, Results1)) > 0),
        ?assertEqual(1, maps:get(cached_datasets, wf_exports_store:status())),
        Result2 = submit_and_wait(Request),
        ?assertEqual(maps:get(results, Result1), maps:get(results, Result2)),
        ?assertEqual(1, maps:get(cached_datasets, wf_exports_store:status()))
    after
        cleanup_stores(Started)
    end.

queued_catalogs_cache_built_entities_test() ->
    Started = setup_stores(),
    try
        ItemRequest = catalog_request("items",
          ["--exports-dir", fixture_exports(),
           "--file", "ExportFlavour_en.json", "visible"]),
        ItemResult = submit_and_wait(ItemRequest),
        ?assertEqual(1, maps:get(total, maps:get(results, ItemResult))),
        EnemyRequest = catalog_request("enemies",
          ["--knowledge-dir", fixture_knowledge(), "lancer"]),
        EnemyResult1 = submit_and_wait(EnemyRequest),
        EnemyResult2 = submit_and_wait(EnemyRequest),
        ?assertEqual(maps:get(results, EnemyResult1), maps:get(results, EnemyResult2)),
        Status = wf_exports_store:status(),
        ?assertEqual(2, maps:get(cached_catalogs, Status)),
        ?assertEqual(2, maps:get(cached_datasets, Status))
    after
        cleanup_stores(Started)
    end.

code_change_discards_obsolete_cache_shapes_test() ->
    CurrentKey = {4, entities, mods, undefined, false},
    OldKey = {entities, mods, undefined, false},
    State = #{cache => #{CurrentKey => current, OldKey => old}},
    {ok, Updated} = wf_exports_store:code_change(undefined, State, undefined),
    ?assertEqual(#{CurrentKey => current}, maps:get(cache, Updated)).

concurrent_cache_updates_do_not_restore_stale_entries_test() ->
    Started = setup_stores(),
    TestPid = self(),
    Seed = fun(_Request, State) ->
        {{ok, seeded}, State#{cache => #{shared => old}}}
    end,
    application:set_env(retinue_daemon, daemon_catalog_execute_fun, Seed),
    try
        ?assertEqual({ok, seeded}, submit_and_wait_reply(#{kind => seed})),
        Execute = fun(Request, State) ->
            Kind = maps:get(kind, Request),
            TestPid ! {catalog_worker_started, Kind, self()},
            receive continue -> ok end,
            Cache = maps:get(cache, State),
            case Kind of
                update_shared -> {{ok, updated}, State#{cache => Cache#{shared => new}}};
                add_other -> {{ok, added}, State#{cache => Cache#{other => value}}};
                inspect -> {{ok, Cache}, State}
            end
        end,
        application:set_env(retinue_daemon, daemon_catalog_execute_fun, Execute),
        {ok, SharedRef} = wf_exports_store:submit(self(), #{kind => update_shared}),
        {ok, OtherRef} = wf_exports_store:submit(self(), #{kind => add_other}),
        SharedWorker = receive
            {catalog_worker_started, update_shared, Pid1} -> Pid1
        after 1000 -> error(shared_worker_not_started)
        end,
        OtherWorker = receive
            {catalog_worker_started, add_other, Pid2} -> Pid2
        after 1000 -> error(other_worker_not_started)
        end,
        SharedWorker ! continue,
        ?assertEqual({ok, updated}, await_query(SharedRef)),
        OtherWorker ! continue,
        ?assertEqual({ok, added}, await_query(OtherRef)),
        application:set_env(
          retinue_daemon, daemon_catalog_execute_fun,
          fun(_Request, State) -> {{ok, maps:get(cache, State)}, State} end),
        {ok, Cache} = submit_and_wait_reply(#{kind => inspect}),
        ?assertEqual(new, maps:get(shared, Cache)),
        ?assertEqual(value, maps:get(other, Cache))
    after
        application:unset_env(retinue_daemon, daemon_catalog_execute_fun),
        cleanup_stores(Started)
    end.

malformed_typed_query_does_not_crash_store_test() ->
    Started = setup_stores(),
    try
        Request = #{source => exports, command => "mods", query => malformed,
                    cwd => filename:absname(".")},
        {error, {catalog_query_failed, error, {badmap, malformed}}} = submit_and_wait_reply(Request),
        ?assert(is_map(wf_exports_store:status()))
    after
        cleanup_stores(Started)
    end.

identical_concurrent_queries_share_catalog_worker_test() ->
    Started = setup_stores(),
    QueryStarted = start_query_service(),
    TestPid = self(),
    ExecuteFun = fun(_Request, State) ->
        TestPid ! {catalog_worker_started, self()},
        receive continue -> {{ok, ignored}, State} end
    end,
    application:set_env(retinue_daemon, daemon_catalog_execute_fun, ExecuteFun),
    try
        Request = #{query_tokens => ["dataset=mods", "test"]},
        {ok, Ref1} = wf_query_service:submit(self(), Request),
        {ok, Ref2} = wf_query_service:submit(self(), Request),
        Worker1 = receive
            {catalog_worker_started, Pid1} -> Pid1
        after 1000 ->
            error(first_catalog_worker_not_started)
        end,
        receive
            {catalog_worker_started, _Pid2} -> error(duplicate_catalog_worker_started)
        after 100 -> ok
        end,
        Worker1 ! continue,
        _ = await_query(Ref1),
        _ = await_query(Ref2),
        wait_until_idle(wf_query_service, 100),
        wait_until_idle(wf_exports_store, 100)
    after
        application:unset_env(retinue_daemon, daemon_catalog_execute_fun),
        cleanup_query_service(QueryStarted),
        cleanup_stores(Started)
    end.

canceling_one_duplicate_keeps_shared_catalog_worker_test() ->
    Started = setup_stores(),
    TestPid = self(),
    ExecuteFun = fun(_Request, State) ->
        TestPid ! {shared_catalog_started, self()},
        receive continue -> {{ok, shared}, State} end
    end,
    application:set_env(retinue_daemon, daemon_catalog_execute_fun, ExecuteFun),
    Client1 = spawn(fun() -> receive stop -> ok end end),
    Client2 = spawn(fun() ->
        receive Reply -> TestPid ! {second_catalog_client, Reply} end
    end),
    try
        Request = #{job => shared},
        {ok, _Ref1} = wf_exports_store:submit(Client1, Request),
        Worker = receive
            {shared_catalog_started, Pid} -> Pid
        after 1000 -> error(shared_catalog_worker_not_started)
        end,
        {ok, Ref2} = wf_exports_store:submit(Client2, Request),
        WorkerMonitor = erlang:monitor(process, Worker),
        exit(Client1, kill),
        receive
            {'DOWN', WorkerMonitor, process, Worker, _Reason} ->
                error(shared_catalog_worker_was_canceled)
        after 100 -> ok
        end,
        Worker ! continue,
        receive
            {second_catalog_client, {retinue_daemon, Ref2, {ok, shared}}} -> ok
        after 1000 -> error(second_catalog_client_timeout)
        end,
        erlang:demonitor(WorkerMonitor, [flush])
    after
        case is_process_alive(Client1) of true -> exit(Client1, kill); false -> ok end,
        case is_process_alive(Client2) of true -> exit(Client2, kill); false -> ok end,
        application:unset_env(retinue_daemon, daemon_catalog_execute_fun),
        cleanup_stores(Started)
    end.

catalog_worker_limit_queues_excess_work_test() ->
    application:set_env(retinue_daemon, catalog_workers, 2),
    Started = setup_stores(),
    TestPid = self(),
    ExecuteFun = fun(Request, State) ->
        TestPid ! {bounded_catalog_started, maps:get(job, Request), self()},
        receive continue -> {{ok, done}, State} end
    end,
    application:set_env(retinue_daemon, daemon_catalog_execute_fun, ExecuteFun),
    try
        {ok, Ref1} = wf_exports_store:submit(self(), #{job => 1}),
        {ok, Ref2} = wf_exports_store:submit(self(), #{job => 2}),
        {ok, Ref3} = wf_exports_store:submit(self(), #{job => 3}),
        {Job1, Worker1} = receive
            {bounded_catalog_started, FirstJob, FirstPid} -> {FirstJob, FirstPid}
        after 1000 -> error(first_bounded_catalog_not_started)
        end,
        {_Job2, Worker2} = receive
            {bounded_catalog_started, SecondJob, SecondPid} -> {SecondJob, SecondPid}
        after 1000 -> error(second_bounded_catalog_not_started)
        end,
        ?assertMatch(#{active := 2, queued := 1}, wf_exports_store:status()),
        receive {bounded_catalog_started, _, _} -> error(catalog_limit_exceeded)
        after 100 -> ok
        end,
        Worker1 ! continue,
        _ = await_query(case Job1 of 1 -> Ref1; 2 -> Ref2 end),
        {Job3, Worker3} = receive
            {bounded_catalog_started, ThirdJob, ThirdPid} -> {ThirdJob, ThirdPid}
        after 1000 -> error(queued_catalog_not_started)
        end,
        ?assertEqual(3, Job3),
        Worker2 ! continue,
        Worker3 ! continue,
        _ = await_query(case Job1 of 1 -> Ref2; 2 -> Ref1 end),
        _ = await_query(Ref3)
    after
        application:unset_env(retinue_daemon, daemon_catalog_execute_fun),
        application:unset_env(retinue_daemon, catalog_workers),
        cleanup_stores(Started)
    end.

query_client_exit_cancels_running_catalog_work_test() ->
    Started = setup_stores(),
    QueryStarted = start_query_service(),
    TestPid = self(),
    ExecuteFun = fun(_Request, State) ->
        TestPid ! {catalog_worker_started, self()},
        receive continue -> {{ok, ignored}, State} end
    end,
    application:set_env(retinue_daemon, daemon_catalog_execute_fun, ExecuteFun),
    Client = spawn(fun() -> receive stop -> ok end end),
    try
        {ok, _Ref} = wf_query_service:submit(
                       Client, #{query_tokens => ["dataset=mods", "test"]}),
        Worker = receive
            {catalog_worker_started, Pid} -> Pid
        after 1000 ->
            ?assert(false)
        end,
        WorkerMonitor = erlang:monitor(process, Worker),
        exit(Client, kill),
        receive
            {'DOWN', WorkerMonitor, process, Worker, killed} -> ok
        after 1000 ->
            ?assert(false)
        end,
        wait_until_idle(wf_query_service, 100),
        wait_until_idle(wf_exports_store, 100)
    after
        case is_process_alive(Client) of true -> exit(Client, kill); false -> ok end,
        application:unset_env(retinue_daemon, daemon_catalog_execute_fun),
        cleanup_query_service(QueryStarted),
        cleanup_stores(Started)
    end.

submit_and_wait(Request) ->
    {ok, Result} = submit_and_wait_reply(Request),
    Result.

submit_and_wait_reply(Request) ->
    {ok, Ref} = wf_exports_store:submit(self(), Request),
    receive
        {retinue_daemon, Ref, Reply} -> Reply
    after 5000 ->
        ?assert(false)
    end.

await_query(Ref) ->
    receive
        {retinue_daemon, Ref, Reply} -> Reply
    after 5000 ->
        error({query_reply_timeout, Ref})
    end.

catalog_request(Command, Args) when Command =:= "mods"; Command =:= "items" ->
    Query = wf_test_cli:catalog(Command, Args),
    #{source => exports, command => Command, query => Query, cwd => filename:absname(".")};
catalog_request(Command, Args) ->
    Query = wf_test_cli:catalog(Command, Args),
    #{source => exports, command => Command, query => Query, cwd => filename:absname(".")}.

fixture_exports() ->
    filename:join(["apps", "retinue_cli", "test", "fixtures", "exports"]).

fixture_knowledge() ->
    filename:join(["apps", "retinue_cli", "test", "fixtures", "knowledge"]).

setup_stores() ->
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    ActivityStarted = case whereis(retinue_activity) of
        undefined -> {ok, _} = retinue_activity:start_link(), true;
        _ -> false
    end,
    WorldstateStarted = case whereis(wf_worldstate_service) of
        undefined -> {ok, _} = wf_worldstate_service:start_link(), true;
        _ -> false
    end,
    ExportStarted = case whereis(wf_exports_store) of
        undefined -> {ok, _} = wf_exports_store:start_link(), true;
        _ -> false
    end,
    SourceStarted = case whereis(wf_source_manager) of
        undefined -> {ok, _} = wf_source_manager:start_link(), true;
        _ -> false
    end,
    {WorldstateStarted, ExportStarted, SourceStarted, ActivityStarted}.

cleanup_stores({WorldstateStarted, ExportStarted, SourceStarted, ActivityStarted}) ->
    case SourceStarted of
        true -> gen_server:stop(wf_source_manager);
        false -> ok
    end,
    case ExportStarted of
        true -> gen_server:stop(wf_exports_store);
        false -> ok
    end,
    case WorldstateStarted of
        true -> gen_server:stop(wf_worldstate_service);
        false -> ok
    end,
    case ActivityStarted of
        true -> gen_server:stop(retinue_activity);
        false -> ok
    end.

start_query_service() ->
    case whereis(wf_query_service) of
        undefined -> {ok, _Pid} = wf_query_service:start_link(), true;
        _Pid -> false
    end.

cleanup_query_service(true) -> gen_server:stop(wf_query_service);
cleanup_query_service(false) -> ok.

wait_until_idle(_Module, 0) ->
    ?assert(false);
wait_until_idle(Module, Attempts) ->
    case Module:status() of
        #{processing := false, queued := 0} -> ok;
        _ ->
            timer:sleep(10),
            wait_until_idle(Module, Attempts - 1)
    end.

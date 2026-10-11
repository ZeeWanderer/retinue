%%%-------------------------------------------------------------------
%% EUnit tests for persistent worldstate service.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_service_eunit).

-include_lib("eunit/include/eunit.hrl").

queued_one_shot_test() ->
    Started = setup_service(),
    try
        Request = (base_request())#{type_filter => fissure, query => undefined},
        {ok, Ref} = wfcli_worldstate_service:submit(self(), Request),
        receive
            {retinue_daemon, Ref, {ok, Result}} ->
                ?assertEqual(entries, maps:get(kind, Result)),
                ?assert(length(maps:get(entries, Result)) > 0)
        after 5000 ->
            ?assert(false)
        end,
        assert_activity(0)
    after
        cleanup_service(Started)
    end.

invalid_query_is_rejected_by_daemon_test() ->
    Started = setup_service(),
    try
        Request = (base_request())#{type_filter => alert, query => "foo OR"},
        ?assertMatch({error, {query_errors, [_ | _]}},
                     wfcli_worldstate_service:submit(self(), Request)),
        assert_activity(0)
    after
        cleanup_service(Started)
    end.

unknown_query_field_is_rejected_by_daemon_test() ->
    Started = setup_service(),
    try
        Request = (base_request())#{type_filter => alert, query => "no_such_field=Corpus"},
        ?assertMatch({error, {query_errors, [_ | _]}},
                     wfcli_worldstate_service:submit(self(), Request))
    after
        cleanup_service(Started)
    end.

invalid_watch_query_is_rejected_by_daemon_test() ->
    Started = setup_service(),
    try
        Request = (base_request())#{
            specs => [#{label => "alerts", type_filter => alert, query => "foo OR"}],
            interval => 60},
        ?assertMatch({error, {query_errors, [_ | _]}},
                     wfcli_worldstate_service:subscribe(self(), Request)),
        assert_activity(0)
    after
        cleanup_service(Started)
    end.

memory_reuse_reports_delivery_and_origin_test() ->
    Started = setup_service(),
    try
        Request = (base_request())#{type_filter => alert, query => undefined},
        First = submit_result(Request),
        ?assertEqual(cached, maps:get(source, First)),
        ?assertEqual(cached, maps:get(snapshot_origin, First)),
        Second = submit_result(Request),
        ?assertEqual(memory, maps:get(source, Second)),
        ?assertEqual(cached, maps:get(snapshot_origin, Second)),
        ?assert(maps:get(snapshot_age_ms, Second) >= 0)
    after
        cleanup_service(Started)
    end.

local_and_utc_use_separate_projection_caches_test() ->
    Started = setup_service(),
    try
        Base = base_request(),
        Opts = (maps:get(opts, Base))#{raw => false, utc => false},
        Request = Base#{opts := Opts, type_filter => fissure, query => undefined},
        Local = submit_result(Request),
        Utc = submit_result(Request#{opts := Opts#{utc := true}}),
        [First | _] = maps:get(entries, Utc),
        Expiry = maps:get(expiry, maps:get(row_map, First)),
        ?assert(lists:suffix("Z", Expiry)),
        [LocalFirst | _] = maps:get(entries, Local),
        ?assertNot(lists:suffix("Z", maps:get(expiry, maps:get(row_map, LocalFirst)))),
        ?assertEqual(maps:get(entries, Local), maps:get(entries, submit_result(Request)))
    after cleanup_service(Started) end.

archimedea_context_change_reindexes_cached_snapshot_test() ->
    ensure_service_stopped(),
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:set_env(
      retinue_daemon, archimedea_context_fun,
      fun() -> persistent_term:get({?MODULE, archimedea_context}) end),
    persistent_term:put({?MODULE, archimedea_context}, loadout_context(one, "Owned One")),
    Activity = setup_activity(),
    {ok, _Pid} = wfcli_worldstate_service:start_link(),
    try
        Request = (base_request())#{
                    opts := #{cache => fixture("worldstate_sample.json"),
                              ttl => 999999999, resolve_items => false,
                              raw => false, search_raw => false},
                    type_filter => archimedea, query => undefined},
        First = archimedea_loadouts(submit_result(Request), "Deep"),
        ?assert(string:find(First, "Owned One") =/= nomatch),
        persistent_term:put({?MODULE, archimedea_context},
                            loadout_context(two, "Owned Two")),
        Second = archimedea_loadouts(submit_result(Request), "Deep"),
        ?assert(string:find(Second, "Owned Two") =/= nomatch),
        ?assertEqual(nomatch, string:find(Second, "Owned One"))
    after
        gen_server:stop(wfcli_worldstate_service),
        cleanup_activity(Activity),
        application:unset_env(retinue_daemon, archimedea_context_fun),
        persistent_term:erase({?MODULE, archimedea_context})
    end.

one_cycle_watch_subscription_test() ->
    Started = setup_service(),
    try
        Request = (base_request())#{
            specs => [#{label => "fissures", type_filter => fissure, query => undefined}],
            interval => 60,
            once => true,
            always => false
        },
        {ok, Ref} = wfcli_worldstate_service:subscribe(self(), Request),
        receive
            {retinue_daemon, Ref, {ok, Update}} ->
                ?assertEqual(watch, maps:get(kind, Update)),
                [Spec] = maps:get(specs, Update),
                ?assertEqual("fissures", maps:get(label, Spec)),
                ?assert(length(maps:get(entries, Spec)) > 0)
        after 5000 ->
            ?assert(false)
        end,
        assert_activity(0)
    after
        cleanup_service(Started)
    end.

concurrent_one_shots_share_fetch_test() ->
    ensure_service_stopped(),
    TestPid = self(),
    FetchFun = fun() ->
        TestPid ! {fetch_started, self()},
        receive continue -> file:read_file(fixture("worldstate_sample.json")) end
    end,
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:set_env(retinue_daemon, daemon_worldstate_fetch_fun, FetchFun),
    Activity = setup_activity(),
    {ok, _Pid} = wfcli_worldstate_service:start_link(),
    Cache = filename:join("/tmp", "wfcli-service-singleflight.json"),
    _ = file:delete(Cache),
    try
        Request = #{source => worldstate,
                    opts => #{cache => Cache, ttl => 60, refresh => true,
                              resolve_items => false, raw => true, search_raw => true},
                    type_filter => alert, query => undefined, mode => list},
        {ok, Ref1} = wfcli_worldstate_service:submit(self(), Request),
        Worker = receive {fetch_started, FetchPid} -> FetchPid after 1000 -> ?assert(false) end,
        {ok, Ref2} = wfcli_worldstate_service:submit(self(), Request),
        assert_activity(1),
        Worker ! continue,
        receive {retinue_daemon, Ref1, {ok, _}} -> ok after 5000 -> ?assert(false) end,
        receive {retinue_daemon, Ref2, {ok, _}} -> ok after 5000 -> ?assert(false) end,
        receive {fetch_started, _Other} -> ?assert(false) after 100 -> ok end,
        assert_activity(0)
    after
        gen_server:stop(wfcli_worldstate_service),
        cleanup_activity(Activity),
        application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
        _ = file:delete(Cache)
    end.

fetch_worker_crash_is_reported_without_killing_service_test() ->
    ensure_service_stopped(),
    Cache = filename:join(
              "/tmp", "wfcli-fetch-crash-" ++
                      integer_to_list(erlang:unique_integer([positive])) ++ ".json"),
    TestPid = self(),
    FetchFun = fun() ->
        TestPid ! {fetch_worker_started, self()},
        receive stop -> {error, stopped} end
    end,
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:set_env(retinue_daemon, daemon_worldstate_fetch_fun, FetchFun),
    Activity = setup_activity(),
    {ok, Pid} = wfcli_worldstate_service:start_link(),
    try
        Request = #{source => worldstate,
                    opts => #{cache => Cache, ttl => 60, refresh => true,
                              resolve_items => false, raw => true, search_raw => true},
                    type_filter => alert, query => undefined, mode => list},
        {ok, Ref} = wfcli_worldstate_service:submit(self(), Request),
        Worker = receive {fetch_worker_started, FetchPid} -> FetchPid
                 after 1000 -> ?assert(false)
                 end,
        exit(Worker, kill),
        receive
            {retinue_daemon, Ref, {error, {fetch_worker_down, killed}}} -> ok
        after 1000 ->
            ?assert(false)
        end,
        ?assert(is_process_alive(Pid)),
        assert_activity(0)
    after
        gen_server:stop(Pid),
        cleanup_activity(Activity),
        application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
        _ = file:delete(Cache),
        _ = file:delete(Cache ++ ".lock")
    end.

fetch_worker_stops_with_owner_test_() ->
    [fun() -> fetch_worker_stops_with_owner(Reason) end || Reason <- [normal, kill]].

fetch_worker_stops_with_owner(Reason) ->
    ensure_service_stopped(),
    Test = self(),
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:set_env(retinue_daemon, daemon_worldstate_fetch_fun,
                        fun() ->
                            Test ! {blocked_fetch, self()},
                            receive finish -> {error, unused} end
                        end),
    Activity = setup_activity(),
    {ok, Pid} = wfcli_worldstate_service:start_link(),
    unlink(Pid),
    Cache = filename:join("/tmp", "wfcli-fetch-owner-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    try
        {ok, _} = wfcli_worldstate_service:submit(
                    self(), #{source => worldstate,
                              opts => #{cache => Cache, refresh => true},
                              type_filter => alert, mode => list}),
        Worker = receive {blocked_fetch, W} -> W
                 after 1000 -> error(fetch_not_started)
                 end,
        assert_activity(1),
        Ref = monitor(process, Worker),
        case Reason of
            normal -> gen_server:stop(Pid);
            kill -> exit(Pid, kill)
        end,
        receive {'DOWN', Ref, process, Worker, _} -> ok
        after 1000 -> error(orphaned_fetch_worker)
        end,
        await_external_activity(0, 100)
    after
        case is_process_alive(Pid) of true -> gen_server:stop(Pid); false -> ok end,
        cleanup_activity(Activity),
        application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
        _ = file:delete(Cache),
        _ = file:delete(Cache ++ ".lock")
    end.

teshin_one_shot_is_calculated_without_worldstate_test() ->
    Started = setup_service(),
    try
        Request = #{source => teshin, opts => #{}, type_filter => teshin,
                    inventory => teshin, query => undefined, mode => list},
        Result = submit_result(Request),
        ?assertEqual(inventory, maps:get(kind, Result)),
        ?assertEqual(teshin, maps:get(inventory, Result)),
        ?assertEqual(calculated, maps:get(source, Result)),
        ?assertEqual(20, length(maps:get(entries, Result)))
    after
        cleanup_service(Started)
    end.

refreshing_watch_waits_for_fetch_test() ->
    ensure_service_stopped(),
    TestPid = self(),
    FetchFun = fun() ->
        TestPid ! {fetch_started, self()},
        receive continue -> file:read_file(fixture("worldstate_sample.json")) end
    end,
    Cache = filename:join(
              "/tmp", "wfcli-watch-refresh-" ++
                      integer_to_list(erlang:unique_integer([positive])) ++ ".json"),
    {ok, FixtureBin} = file:read_file(fixture("worldstate_sample.json")),
    ok = file:write_file(Cache, FixtureBin),
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
    Activity = setup_activity(),
    {ok, _Pid} = wfcli_worldstate_service:start_link(),
    try
        Opts = #{cache => Cache, ttl => 999999999,
                 resolve_items => false, raw => true, search_raw => true},
        Request = #{source => worldstate, opts => Opts,
                    type_filter => alert, query => undefined, mode => list},
        ?assertEqual(cached, maps:get(source, submit_result(Request))),
        application:set_env(retinue_daemon, daemon_worldstate_fetch_fun, FetchFun),
        Watch = Request#{opts := Opts#{refresh => true},
                         specs => [#{label => "alerts", type_filter => alert,
                                     query => undefined}],
                         interval => 60, once => true, always => false},
        {ok, Ref} = wfcli_worldstate_service:subscribe(self(), Watch),
        Worker = receive {fetch_started, FetchPid} -> FetchPid
                 after 1000 -> ?assert(false)
                 end,
        receive
            {retinue_daemon, Ref, _Early} -> ?assert(false)
        after 50 -> ok
        end,
        Worker ! continue,
        receive
            {retinue_daemon, Ref, {ok, Update}} ->
                ?assertEqual(fetched, maps:get(source, Update)),
                ?assertEqual(fetched, maps:get(snapshot_origin, Update))
        after 5000 ->
            ?assert(false)
        end
    after
        gen_server:stop(wfcli_worldstate_service),
        cleanup_activity(Activity),
        application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
        _ = file:delete(Cache),
        _ = file:delete(Cache ++ ".lock")
    end.

watch_holds_activity_until_unsubscribe_test() ->
    Started = setup_service(),
    ActivityPid = whereis(retinue_activity),
    try
        {ok, Ref1} = wfcli_worldstate_service:subscribe(self(), watch_request()),
        receive {retinue_daemon, Ref1, {ok, _}} -> ok after 1000 -> ?assert(false) end,
        {ok, Ref2} = wfcli_worldstate_service:subscribe(self(), watch_request()),
        receive {retinue_daemon, Ref2, {ok, _}} -> ok after 1000 -> ?assert(false) end,
        assert_activity(1),
        ok = retinue_activity:set_idle_policy({idle, 20}),
        assert_no_idle(ActivityPid, 60),
        _ = submit_result((base_request())#{type_filter => alert, query => undefined}),
        ok = wfcli_worldstate_service:unsubscribe(Ref1),
        assert_activity(1),
        ok = wfcli_worldstate_service:unsubscribe(Ref2),
        assert_activity(0),
        receive {wfcli_daemon_idle, ActivityPid} -> ok after 1000 -> ?assert(false) end
    after
        cleanup_service(Started)
    end.

watch_owner_down_releases_activity_test() ->
    Started = setup_service(),
    Client = spawn(fun() -> receive stop -> ok end end),
    ActivityPid = whereis(retinue_activity),
    try
        {ok, _Ref} = wfcli_worldstate_service:subscribe(Client, watch_request()),
        assert_activity(1),
        ok = retinue_activity:set_idle_policy({idle, 20}),
        exit(Client, kill),
        receive {wfcli_daemon_idle, ActivityPid} -> ok after 1000 -> ?assert(false) end,
        ?assertMatch(#{watches := 0, fetching := 0}, wfcli_worldstate_service:status()),
        assert_activity(0)
    after
        exit(Client, kill),
        cleanup_service(Started)
    end.

unsubscribed_fetch_keeps_activity_until_finished_test() ->
    Started = setup_service(),
    Test = self(),
    Cache = filename:join("/tmp", "wfcli-held-fetch-" ++
                         integer_to_list(erlang:unique_integer([positive])) ++ ".json"),
    application:set_env(retinue_daemon, daemon_worldstate_fetch_fun,
                        fun() ->
                            Test ! {blocked_fetch, self()},
                            receive finish -> {error, canceled} end
                        end),
    try
        {ok, Ref} = wfcli_worldstate_service:submit(
                      self(), #{source => worldstate, mode => list, type_filter => alert,
                                opts => #{cache => Cache, refresh => true}}),
        Worker = receive {blocked_fetch, W} -> W after 1000 -> error(fetch_not_started) end,
        ok = wfcli_worldstate_service:unsubscribe(Ref),
        ?assertMatch(#{one_shots := 0, fetching := 1}, wfcli_worldstate_service:status()),
        assert_activity(1),
        ActivityPid = whereis(retinue_activity),
        ok = retinue_activity:set_idle_policy({idle, 20}),
        assert_no_idle(ActivityPid, 60),
        Worker ! finish,
        receive {wfcli_daemon_idle, ActivityPid} -> ok after 1000 -> ?assert(false) end,
        assert_activity(0),
        receive {retinue_daemon, Ref, _} -> ?assert(false) after 0 -> ok end
    after
        cleanup_service(Started),
        application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
        _ = file:delete(Cache),
        _ = file:delete(Cache ++ ".lock")
    end.

external_hold_is_independent_of_worldstate_test() ->
    Started = setup_service(),
    try
        ok = retinue_activity:activity_start(),
        {ok, Ref} = wfcli_worldstate_service:subscribe(self(), watch_request()),
        receive {retinue_daemon, Ref, {ok, _}} -> ok after 1000 -> ?assert(false) end,
        assert_activity(2),
        ok = wfcli_worldstate_service:unsubscribe(Ref),
        assert_activity(1),
        ok = retinue_activity:activity_end(),
        assert_activity(0)
    after
        cleanup_service(Started)
    end.

code_change_preserves_worldstate_hold_test() ->
    Started = setup_service(),
    try
        {ok, Ref} = wfcli_worldstate_service:subscribe(self(), watch_request()),
        receive {retinue_daemon, Ref, {ok, _}} -> ok after 1000 -> ?assert(false) end,
        Pid = whereis(wfcli_worldstate_service),
        ok = sys:suspend(Pid),
        try
            ok = sys:change_code(Pid, wfcli_worldstate_service, old, hot_update)
        after
            ok = sys:resume(Pid)
        end,
        assert_activity(1),
        ok = wfcli_worldstate_service:unsubscribe(Ref),
        assert_activity(0)
    after
        cleanup_service(Started)
    end.

watch_request() ->
    (base_request())#{specs => [#{label => "alerts", type_filter => alert, query => undefined}],
                      interval => 60, once => false, always => false}.

base_request() ->
    #{source => worldstate,
      opts => #{cache => fixture("worldstate_sample.json"),
                ttl => 999999999,
                resolve_items => false,
                raw => true,
                search_raw => true},
      mode => list}.

submit_result(Request) ->
    {ok, Ref} = wfcli_worldstate_service:submit(self(), Request),
    receive
        {retinue_daemon, Ref, {ok, Result}} -> Result
    after 5000 ->
        ?assert(false)
    end.

loadout_context(Key, Name) ->
    Pool = #{owned => [<<"owned">>], catalog => [<<"a">>, <<"b">>, <<"c">>]},
    #{status => ready, key => Key, account_seed => 7,
      names => #{<<"owned">> => Name, <<"a">> => "A",
                 <<"b">> => "B", <<"c">> => "C"},
      pools => #{suits => Pool, primaries => Pool,
                 secondaries => Pool, melees => Pool}}.

archimedea_loadouts(Result, Kind) ->
    [Entry] = [Candidate || Candidate <- maps:get(entries, Result),
                           maps:get(archimedea, maps:get(row_map, Candidate)) =:= Kind],
    maps:get(loadouts, maps:get(row_map, Entry)).

await_external_activity(Expected, Attempts) ->
    case maps:get(external_activity, retinue_activity:status()) of
        Expected -> ok;
        _ when Attempts > 0 ->
            timer:sleep(10),
            await_external_activity(Expected, Attempts - 1);
        Actual ->
            ?assertEqual(Expected, Actual)
    end.

assert_activity(Expected) ->
    _ = wfcli_worldstate_service:status(),
    ?assertMatch(#{external_activity := Expected}, retinue_activity:status()).

assert_no_idle(Pid, Timeout) ->
    receive {wfcli_daemon_idle, Pid} -> error(unexpected_idle_timeout)
    after Timeout -> ok
    end.

fixture(Name) ->
    filename:join(["apps", "retinue_cli", "test", "fixtures", Name]).

setup_service() ->
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    application:unset_env(retinue_daemon, daemon_idle_notify_pid),
    application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun),
    Activity = setup_activity(),
    case whereis(wfcli_worldstate_service) of
        undefined ->
            {ok, _Pid} = wfcli_worldstate_service:start_link(),
            {started, Activity};
        _Pid -> {already_running, Activity}
    end.

cleanup_service({started, Activity}) ->
    gen_server:stop(wfcli_worldstate_service),
    cleanup_activity(Activity);
cleanup_service({already_running, Activity}) ->
    cleanup_activity(Activity).

setup_activity() ->
    case whereis(retinue_activity) of
        undefined ->
            SavedNotify = application:get_env(retinue_daemon, daemon_idle_notify_pid),
            application:set_env(retinue_daemon, daemon_idle_notify_pid, self()),
            {ok, Pid} = retinue_activity:start_link(),
            {started, Pid, SavedNotify};
        _Pid -> already_running
    end.

cleanup_activity({started, Pid, SavedNotify}) ->
    gen_server:stop(Pid),
    case SavedNotify of
        undefined -> application:unset_env(retinue_daemon, daemon_idle_notify_pid);
        {ok, Value} -> application:set_env(retinue_daemon, daemon_idle_notify_pid, Value)
    end;
cleanup_activity(already_running) ->
    ok.

ensure_service_stopped() ->
    case whereis(wfcli_worldstate_service) of
        undefined -> ok;
        _Pid -> gen_server:stop(wfcli_worldstate_service)
    end.

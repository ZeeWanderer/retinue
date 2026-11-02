-module(retinue_incident_log_eunit).
-include_lib("eunit/include/eunit.hrl").

persistent_bounded_warning_log_test() ->
    with_log(fun(Path) ->
        ?assertEqual(ok, retinue_incident_log:install()),
        {ok, #{level := warning, config := Config}} = logger:get_handler_config(retinue_incidents),
        ?assertEqual(1024 * 1024, maps:get(max_no_bytes, Config)),
        ?assertEqual(2, maps:get(max_no_files, Config)),
        logger:notice("wf_notice_not_an_incident"),
        logger:warning(#{event => wf_test_incident, source => wfcd, reason => timeout}),
        ok = logger_std_h:filesync(retinue_incidents),
        {ok, Body} = file:read_file(Path),
        ?assertNotEqual(nomatch, binary:match(Body, <<"wf_test_incident">>)),
        ?assertEqual(nomatch, binary:match(Body, <<"wf_notice_not_an_incident">>))
    end).

manual_refresh_failure_is_persisted_test() ->
    with_log(fun(Path) ->
        application:set_env(retinue_daemon, source_refresh_interval_ms, 0),
        application:set_env(retinue_daemon, source_update_fun, fun(_) -> {error, fixture_failure} end),
        {ok, Pid} = wf_source_manager:start_link(),
        try
            {ok, Ref} = wf_source_manager:submit(self(), #{action => refresh, selections => [wfcd]}),
            receive
                {retinue_daemon, Ref, {ok, #{success := false}}} -> ok
            after 1000 -> ?assert(false)
            end,
            ok = logger_std_h:filesync(retinue_incidents),
            {ok, Body} = file:read_file(Path),
            ?assertNotEqual(nomatch, binary:match(Body, <<"knowledge_refresh_failed">>)),
            ?assertNotEqual(nomatch, binary:match(Body, <<"fixture_failure">>)),
            ?assertEqual(1, length(binary:matches(Body, <<"knowledge_refresh_failed">>)))
        after
            gen_server:stop(Pid),
            application:unset_env(retinue_daemon, source_refresh_interval_ms),
            application:unset_env(retinue_daemon, source_update_fun)
        end
    end).

background_refresh_failure_is_persisted_once_test() ->
    with_log(fun(Path) ->
        application:set_env(retinue_daemon, source_refresh_interval_ms, 0),
        application:set_env(retinue_daemon, source_max_age_seconds, 0),
        application:set_env(retinue_daemon, source_update_fun, fun(wfcd) -> {error, fixture_failure}; (_) -> ok end),
        {ok, Pid} = wf_source_manager:start_link(),
        try
            Pid ! refresh_stale_sources,
            {ok, Ref} = wf_source_manager:submit(self(), #{action => refresh, selections => []}),
            receive {retinue_daemon, Ref, {ok, #{success := true}}} -> ok after 1000 -> ?assert(false) end,
            ok = logger_std_h:filesync(retinue_incidents),
            {ok, Body} = file:read_file(Path),
            ?assertEqual(1, length(binary:matches(Body, <<"knowledge_refresh_failed">>))),
            ?assertNotEqual(nomatch, binary:match(Body, <<"fixture_failure">>))
        after
            gen_server:stop(Pid),
            application:unset_env(retinue_daemon, source_refresh_interval_ms),
            application:unset_env(retinue_daemon, source_max_age_seconds),
            application:unset_env(retinue_daemon, source_update_fun)
        end
    end).

mirror_failure_is_persisted_test() ->
    with_log(fun(Path) ->
        application:set_env(retinue_daemon, wfcd_http_fun,
            fun("https://registry.npmjs.org/@wfcd/items/latest", _) ->
                    {ok, 200, <<"{\"name\":\"@wfcd/items\",\"version\":\"1.2.3\"}">>};
               (_, _) -> {ok, 503, <<>>}
            end),
        try
            ?assertMatch({error, {wfcd_sources_failed, _, _}}, wf_wfcd:fetch()),
            ok = logger_std_h:filesync(retinue_incidents),
            {ok, Body} = file:read_file(Path),
            ?assertEqual(2, length(binary:matches(Body, <<"wfcd_mirror_failed">>))),
            ?assertNotEqual(nomatch, binary:match(Body, <<"unpkg">>)),
            ?assertNotEqual(nomatch, binary:match(Body, <<"jsdelivr">>))
        after application:unset_env(retinue_daemon, wfcd_http_fun)
        end
    end).

with_log(Test) ->
    {ok, Activity} = retinue_activity:start_link(),
    PreviousHandler = logger:get_handler_config(retinue_incidents),
    _ = logger:remove_handler(retinue_incidents),
    Root = filename:join("/tmp", "wfcli-incident-test-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    Previous = os:getenv("XDG_STATE_HOME"),
    os:putenv("XDG_STATE_HOME", Root),
    try
        ok = retinue_incident_log:install(),
        Test(retinue_incident_log:path())
    after
        logger:remove_handler(retinue_incidents),
        gen_server:stop(Activity),
        case Previous of false -> os:unsetenv("XDG_STATE_HOME"); _ -> os:putenv("XDG_STATE_HOME", Previous) end,
        file:del_dir_r(Root),
        case PreviousHandler of
            {ok, #{module := Module} = Config} ->
                logger:add_handler(retinue_incidents, Module, maps:without([id, module], Config));
            _ -> ok
        end
    end.

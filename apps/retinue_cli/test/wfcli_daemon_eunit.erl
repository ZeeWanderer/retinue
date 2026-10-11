%%%-------------------------------------------------------------------
%% EUnit tests for daemon process API.
%%%-------------------------------------------------------------------
-module(wfcli_daemon_eunit).

-include_lib("eunit/include/eunit.hrl").

status_request_returns_running_map_test() ->
    Started = setup_daemon(),
    try
        Status = retinue_daemon:request(status),
        ?assertMatch(#{status := running,
                       node := _,
                       pid := _,
                       uptime_ms := _,
                       version := _,
                       otp_release := _,
                       installation := _,
                       install_root := _,
                       protocols := #{erlang_distribution_rpc := _,
                                      native_socket_api := _},
                       flavor := _,
                       build := _}, Status),
        ?assert(is_pid(maps:get(pid, Status))),
        ?assert(is_integer(maps:get(uptime_ms, Status))),
        ?assertEqual(retinue_build:installation(), maps:get(installation, Status)),
        ?assertEqual(retinue_build:install_root(), maps:get(install_root, Status)),
        Protocols = maps:get(protocols, Status),
        ?assertEqual(retinue_protocol:contract(),
                     maps:get(erlang_distribution_rpc, Protocols)),
        ?assertEqual(retinue_local_protocol:contract(),
                     maps:get(native_socket_api, Protocols)),
        ?assertEqual(retinue_local_protocol:contract(<<"poe2">>),
                     maps:get(poe2_socket_api, Protocols)),
        Paths = maps:get(paths, Status),
        ?assert(lists:keymember(assets, 1, Paths)),
        case maps:get(assets, maps:get(warframe, maps:get(games, Status))) of
            #{cache_root := AssetRoot} ->
                ?assertEqual({assets, AssetRoot}, lists:keyfind(assets, 1, Paths));
            unavailable ->
                ?assertEqual({assets, wfcli_paths:cache_file("assets")},
                             lists:keyfind(assets, 1, Paths))
        end
    after
        cleanup_daemon(Started)
    end.

unknown_request_is_data_error_test() ->
    Started = setup_daemon(),
    try
        ?assertEqual({error, {unknown_request, bogus}}, retinue_daemon:request(bogus))
    after
        cleanup_daemon(Started)
    end.

release_update_requires_name_test() ->
    Started = setup_daemon(),
    try
        ?assertEqual({error, release_name_required}, retinue_daemon:request({update_release, []}))
    after
        cleanup_daemon(Started)
    end.

protocol_handshake_test() ->
    Started = setup_daemon(),
    try
        Reply = retinue_daemon:request({hello, retinue_protocol:contract()}),
        ?assertEqual(true, maps:get(compatible, Reply)),
        ?assertEqual(retinue_protocol:handshake_version(), maps:get(handshake, Reply)),
        ?assertEqual(retinue_protocol:interfaces(), maps:get(interfaces, Reply)),
        ?assertEqual(retinue_build:version(), maps:get(version, Reply)),
        ?assertEqual(retinue_build:flavor(), maps:get(flavor, Reply)),
        ?assertEqual(64, byte_size(maps:get(build, Reply)))
    after
        cleanup_daemon(Started)
    end.

legacy_protocol_handshake_is_rejected_test() ->
    Started = setup_daemon(),
    try
        Reply = retinue_daemon:request({hello, 13}),
        ?assertEqual(false, maps:get(compatible, Reply)),
        ?assert(is_pid(whereis(retinue_daemon)))
    after
        cleanup_daemon(Started)
    end.

typed_hot_update_request_test() ->
    Started = setup_daemon(),
    try
        {ok, Bundles} = retinue_hot_update:read_applications([retinue_core, wfcore, retinue_daemon]),
        ?assertMatch({ok, #{loaded := _, unchanged := _, migrated := _}},
                     retinue_daemon:request({hot_update, Bundles})),
        ?assert(is_pid(whereis(retinue_daemon)))
    after
        cleanup_daemon(Started)
    end.

concurrent_hot_updates_are_rejected_test() ->
    Started = setup_daemon(),
    Test = self(),
    application:set_env(
      retinue_daemon, daemon_hot_update_fun,
      fun(_Bundles) ->
          Test ! {hot_update_started, self()},
          receive continue ->
              {ok, #{loaded => [], unchanged => [], migrated => []}}
          end
      end),
    try
        Caller = spawn(fun() -> Test ! {hot_update_reply,
                                       retinue_daemon:request({hot_update, []})} end),
        Worker = receive {hot_update_started, Pid} -> Pid after 1000 -> timeout end,
        ?assert(is_pid(Worker)),
        ?assertEqual({error, update_in_progress},
                     retinue_daemon:request({hot_update, []})),
        Worker ! continue,
        receive
            {hot_update_reply, Reply} ->
                ?assertMatch({ok, #{loaded := [], unchanged := [], migrated := []}}, Reply)
        after 1000 ->
            exit(Caller, kill),
            error(hot_update_reply_timeout)
        end
    after
        application:unset_env(retinue_daemon, daemon_hot_update_fun),
        cleanup_daemon(Started)
    end.

artifact_worker_down_clears_update_state_test() ->
    Monitor = make_ref(),
    State = #{started_at => 0, artifact_id => <<"old">>,
              artifact_update => #{artifact_id => <<"new">>, monitor => Monitor,
                                   pid => self()}},
    {noreply, Updated} =
        retinue_daemon:handle_info({'DOWN', Monitor, process, self(), test_crash}, State),
    ?assertEqual(false, maps:get(artifact_update, Updated)).

legacy_artifact_update_state_is_cleared_test() ->
    State = #{started_at => 0, artifact_id => <<"old">>, artifact_update => true},
    Path = filename:join("/tmp", "wfcli-hot-log-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    Previous = logger:get_handler_config(retinue_incidents),
    application:set_env(retinue_daemon, incident_log_file, Path),
    try
        {ok, Updated} = retinue_daemon:code_change(undefined, State, undefined),
        ?assertEqual(false, maps:get(artifact_update, Updated)),
        ?assertMatch({ok, _}, logger:get_handler_config(retinue_incidents))
    after
        application:unset_env(retinue_daemon, incident_log_file),
        case Previous of {error, _} -> logger:remove_handler(retinue_incidents); _ -> ok end,
        file:delete(Path)
    end.

setup_daemon() ->
    case whereis(retinue_daemon) of
        undefined ->
            {ok, _Pid} = retinue_daemon:start_link(),
            started;
        _Pid ->
            already_running
    end.

cleanup_daemon(started) ->
    gen_server:stop(retinue_daemon);
cleanup_daemon(already_running) ->
    ok.

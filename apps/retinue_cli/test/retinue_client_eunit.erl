%%%-------------------------------------------------------------------
%% EUnit tests for daemon control helpers that do not start distribution.
%%%-------------------------------------------------------------------
-module(retinue_client_eunit).

-include_lib("eunit/include/eunit.hrl").

default_daemon_node_uses_retinue_name_test() ->
    Node = atom_to_list(retinue_client:daemon_node()),
    ?assertMatch("retinue_daemon@" ++ _, Node).

compiled_version_matches_application_version_test() ->
    _ = application:load(wfcore),
    {ok, Version} = application:get_key(wfcore, vsn),
    ?assertEqual(Version, retinue_build:version()).

release_script_candidates_include_rebar_outputs_test() ->
    Candidates = retinue_client:release_script_candidates(),
    ?assert(lists:any(fun(Path) ->
                          string:find(Path, "_build/default/rel/retinue_daemon/bin/retinue_daemon") =/= nomatch
                      end, Candidates)),
    ?assert(lists:any(fun(Path) ->
                          string:find(
                            Path,
                            "_build/prod/rel/retinue_daemon/bin/retinue_daemon") =/= nomatch
                      end, Candidates)).

build_flavor_follows_staged_path_test() ->
    ?assertEqual(dev, retinue_build:flavor_from_path("/repo/dev/bin/retinue")),
    ?assertEqual(prod, retinue_build:flavor_from_path("/repo/prod/bin/retinue")),
    lists:foreach(fun(Name) ->
        ?assertEqual(dev, retinue_build:flavor_from_path("/repo/" ++ Name))
    end, ["retinued", "wfclid", "poe2clid", "arcclid"]).

brew_update_root_uses_stable_opt_prefix_test() ->
    ?assertEqual("/home/linuxbrew/.linuxbrew/opt/retinue",
                 retinue_build:brew_update_root(
                   "/home/linuxbrew/.linuxbrew/Cellar/retinue/0.1.0")).

homebrew_install_follows_resolved_cellar_path_test() ->
    ?assert(retinue_build:homebrew_install_from_path(
              "/home/linuxbrew/.linuxbrew/Cellar/retinue/0.1.1/libexec/bin/retinue")),
    ?assert(retinue_build:homebrew_install_from_path(
              "/home/linuxbrew/.linuxbrew/opt/retinue/bin/retinue")),
    ?assertNot(retinue_build:homebrew_install_from_path("/repo/prod/bin/retinue")),
    ?assertNot(retinue_build:homebrew_install_from_path(
                 "/home/linuxbrew/.linuxbrew/Cellar/other/0.1.1/bin/retinue")),
    ?assertNot(retinue_build:homebrew_install_from_path(
                 "/home/linuxbrew/.linuxbrew/Cellar/wfcli/0.2.1/bin/wfcli")),
    ?assertNot(retinue_build:homebrew_install_from_path(
                 "/home/linuxbrew/.linuxbrew/opt/wfcli/bin/wfcli")).

release_environment_keeps_logs_out_of_release_test() ->
    Env = retinue_client:release_environment(),
    ?assertEqual(
       retinue_paths:file(state, "daemon-log"),
       proplists:get_value("RUNNER_LOG_DIR", Env)),
    ?assertEqual(retinue_paths:file(state, "erl_crash.dump"),
                 proplists:get_value("ERL_CRASH_DUMP", Env)),
    ?assertEqual(atom_to_list(retinue_build:flavor()),
                 proplists:get_value("RETINUE_BUILD_FLAVOR", Env)),
    ?assertEqual(false, proplists:get_value("LD_PRELOAD", Env)),
    ?assertEqual(false, proplists:get_value("LD_LIBRARY_PATH", Env)),
    ?assertEqual(false, proplists:get_value("STEAM_RUNTIME", Env)),
    ?assertEqual(false, proplists:get_value("STEAM_RUNTIME_LIBRARY_PATH", Env)).

homebrew_stop_keeps_login_registration_test() ->
    ?assertEqual(["services", "kill", "retinue"],
                 retinue_homebrew_service:stop_args()).

daemon_cli_known_commands_test() ->
    Known = maps:keys(maps:get(commands, retinue_daemon_cli:command())),
    ?assert(lists:member("status", Known)),
    ?assert(lists:member("start", Known)),
    ?assert(lists:member("stop", Known)),
    ?assert(lists:member("restart", Known)),
    ?assert(lists:member("autostart", Known)),
    ?assert(lists:member("update", Known)).

daemon_readiness_requires_registered_server_test() ->
    ?assertEqual(
       retry,
       retinue_client:readiness_result(
         pong, {error, {daemon_call_failed, noproc}})),
    ?assertEqual(retry, retinue_client:readiness_result(pang, not_called)),
    ?assertEqual(ready, retinue_client:readiness_result(pong, {ok, #{status => running}})).

autostart_unit_runs_supervised_persistent_release_test() ->
    Unit = retinue_autostart:unit_file(
             "/tmp/retinue daemon/bin/retinue-daemon", "/opt/erlang/bin:/usr/bin"),
    ?assertNotEqual(nomatch, binary:match(Unit, <<"Type=simple">>)),
    ?assertNotEqual(nomatch,
                    binary:match(Unit, <<"RETINUE_DAEMON_IDLE_POLICY=persistent">>)),
    ?assertNotEqual(nomatch,
                    binary:match(Unit, <<"ExecStart=\"/tmp/retinue daemon/bin/retinue-daemon\" foreground">>)),
    ?assertNotEqual(nomatch,
                    binary:match(Unit, <<"Environment=\"PATH=/opt/erlang/bin:/usr/bin\"">>)),
    ?assertNotEqual(nomatch, binary:match(Unit, <<"WorkingDirectory=">>)),
    ?assertEqual(nomatch, binary:match(Unit, <<"WorkingDirectory=\"">>)),
    ?assertNotEqual(nomatch, binary:match(Unit, <<"ERL_CRASH_DUMP=">>)),
    ?assertNotEqual(nomatch, binary:match(Unit, <<"Restart=on-failure">>)).

contract_mismatch_error_names_required_interfaces_test() ->
    Error = retinue_client:format_error(
              {daemon_contract_mismatch,
               [#{kind => interface, interface => market,
                  required => 1, available => 2}]}),
    ?assert(string:find(Error, "interfaces") =/= nomatch).

matching_build_handshake_is_compatible_test() ->
    {ok, Build} = current_build_identity(),
    Contract = retinue_protocol:contract(),
    Reply = (retinue_protocol:negotiate(Contract))#{
        build => Build, flavor => retinue_build:flavor()},
    ?assertEqual(ok, retinue_client:handshake_compatibility(Reply, Contract)).

legacy_handshake_requires_daemon_update_test() ->
    {ok, Build} = current_build_identity(),
    Contract = retinue_protocol:contract(),
    Reply = retinue_protocol:negotiate(Contract),
    ?assertEqual({error, {daemon_build_mismatch, Build, undefined}},
                 retinue_client:handshake_compatibility(Reply, Contract)).

different_build_handshake_requires_daemon_update_test() ->
    {ok, Build} = current_build_identity(),
    Other = <<"different">>,
    Contract = retinue_protocol:contract(),
    Reply = (retinue_protocol:negotiate(Contract))#{build => Other},
    ?assertEqual({error, {daemon_build_mismatch, Build, Other}},
                 retinue_client:handshake_compatibility(Reply, Contract)).

different_flavor_handshake_requires_daemon_restart_test() ->
    OtherFlavor = case retinue_build:flavor() of dev -> prod; prod -> dev end,
    Contract = retinue_protocol:contract(),
    Reply = (retinue_protocol:negotiate(Contract))#{
        flavor => OtherFlavor, build => <<"irrelevant">>},
    ?assertEqual({error, {daemon_flavor_mismatch, retinue_build:flavor(), OtherFlavor}},
                 retinue_client:handshake_compatibility(Reply, Contract)).

interface_mismatch_requires_daemon_update_test() ->
    Contract = retinue_protocol:contract(),
    Offered = Contract#{interfaces => (retinue_protocol:interfaces())#{market => 2},
                        compatible => true},
    ?assertMatch({error, {daemon_contract_mismatch,
                          [#{kind := interface, interface := market}]}},
                 retinue_client:handshake_compatibility(Offered, Contract)).

malformed_offered_interfaces_are_rejected_test() ->
    Contract = retinue_protocol:contract(),
    Offered = Contract#{interfaces => invalid, compatible => true},
    ?assertMatch({error, {daemon_contract_mismatch,
                          [#{kind := interfaces, available := invalid}]}},
                 retinue_client:handshake_compatibility(Offered, Contract)).

only_older_daemon_contracts_trigger_recovery_test() ->
    ?assert(retinue_daemon_client:recoverable_contract_mismatch(
              [#{kind => interface, interface => market,
                 required => 2, available => 1}])),
    ?assert(retinue_daemon_client:recoverable_contract_mismatch(
              [#{kind => interface, interface => market,
                 required => 1, available => undefined}])),
    ?assertNot(retinue_daemon_client:recoverable_contract_mismatch(
                 [#{kind => interface, interface => market,
                    required => 1, available => 2}])),
    ?assertNot(retinue_daemon_client:recoverable_contract_mismatch(
                 [#{kind => interfaces, required => valid,
                    available => invalid}])).

legacy_integer_handshake_requires_daemon_update_test() ->
    Contract = retinue_protocol:contract(),
    Legacy = #{compatible => true, protocol => 13, build => <<"legacy">>},
    ?assertMatch({error, {daemon_contract_mismatch, [_ | _]}},
                 retinue_client:handshake_compatibility(Legacy, Contract)).

query_errors_are_readable_test() ->
    ?assertEqual("bad syntax; unknown field",
                 retinue_client:format_error(
                   {query_errors, ["bad syntax", "unknown field"]})).

timed_out_launcher_process_is_terminated_test() ->
    case os:find_executable("sleep") of
        false -> ok;
        Sleep ->
            Port = open_port(
                     {spawn_executable, Sleep},
                     [binary, exit_status, {args, ["60"]}]),
            ok = retinue_client:terminate_port_process(Port),
            ?assertEqual(undefined, erlang:port_info(Port))
    end.

current_build_identity() ->
    {ok, Bundles} = retinue_hot_update:read_applications([retinue_core, wfcore, retinue_daemon]),
    retinue_hot_update:build_identity(Bundles).

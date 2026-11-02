%%%-------------------------------------------------------------------
%% Local supervised daemon harness for CLI integration suites.
%%%-------------------------------------------------------------------
-module(retinue_test_daemon).

-export([start/0, start/1, stop/0]).

start() ->
    TestRoot = filename:join(
                 "/tmp", "wfcli-test-" ++ os:getpid() ++ "-" ++
                     integer_to_list(erlang:unique_integer([positive]))),
    start(TestRoot).

start(TestRoot) ->
    {ok, CallerCwd} = file:get_cwd(),
    application:set_env(retinue_daemon, player_cache, filename:join(TestRoot, "player.term")),
    application:set_env(retinue_daemon, game_metadata_cache,
                        filename:join(TestRoot, "game-metadata.term")),
    application:set_env(retinue_daemon, resolution_issues_file,
                        filename:join(TestRoot, "resolution-issues.json")),
    application:set_env(retinue_daemon, local_socket, filename:join(TestRoot, "retinue_daemon.sock")),
    application:set_env(retinue_daemon, notification_settings_file,
                        filename:join(TestRoot, "notifications.json")),
    application:set_env(retinue_daemon, market_account_file,
                        filename:join(TestRoot, "market-token")),
    application:set_env(retinue_daemon, market_presence_file,
                        filename:join(TestRoot, "market-presence.json")),
    application:set_env(retinue_daemon, build_store_file, filename:join(TestRoot, "builds.term")),
    application:set_env(retinue_daemon, build_cache_file, filename:join(TestRoot, "builds.cache")),
    application:set_env(retinue_daemon, poe2_session_file, filename:join(TestRoot, "poe2-session.json")),
    application:set_env(retinue_daemon, incident_log_file, filename:join(TestRoot, "retinue_daemon.log")),
    application:set_env(retinue_daemon, checks_file, filename:join(TestRoot, "checks.json")),
    persistent_term:put({?MODULE, test_root}, TestRoot),
    application:set_env(retinue_cli, test_local_daemon, true),
    application:set_env(retinue_daemon, daemon_enabled, true),
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    try
        case application:ensure_all_started(retinue_daemon) of
            {ok, _} -> ok;
            {error, Reason} -> {error, Reason}
        end
    after
        _ = file:set_cwd(CallerCwd)
    end.

stop() ->
    application:unset_env(retinue_cli, test_local_daemon),
    _ = application:stop(retinue_daemon),
    cleanup_test_root(persistent_term:get({?MODULE, test_root}, undefined)),
    persistent_term:erase({?MODULE, test_root}),
    application:unset_env(retinue_daemon, player_cache),
    application:unset_env(retinue_daemon, game_metadata_cache),
    application:unset_env(retinue_daemon, resolution_issues_file),
    application:unset_env(retinue_daemon, local_socket),
    application:unset_env(retinue_daemon, notification_settings_file),
    application:unset_env(retinue_daemon, market_account_file),
    application:unset_env(retinue_daemon, market_presence_file),
    application:unset_env(retinue_daemon, build_store_file),
    application:unset_env(retinue_daemon, build_cache_file),
    application:unset_env(retinue_daemon, poe2_session_file),
    application:unset_env(retinue_daemon, incident_log_file),
    application:unset_env(retinue_daemon, checks_file),
    ok.

cleanup_test_root(undefined) -> ok;
cleanup_test_root(TestRoot) ->
    _ = file:delete(filename:join(TestRoot, "player.term.tmp")),
    _ = file:delete(filename:join(TestRoot, "player.term")),
    _ = file:delete(filename:join(TestRoot, "game-metadata.term.tmp")),
    _ = file:delete(filename:join(TestRoot, "game-metadata.term")),
    _ = file:delete(filename:join(TestRoot, "resolution-issues.json.tmp")),
    _ = file:delete(filename:join(TestRoot, "resolution-issues.json")),
    _ = file:delete(filename:join(TestRoot, "notifications.json.tmp")),
    _ = file:delete(filename:join(TestRoot, "notifications.json")),
    _ = file:delete(filename:join(TestRoot, "market-token.tmp")),
    _ = file:delete(filename:join(TestRoot, "market-token")),
    _ = file:delete(filename:join(TestRoot, "market-presence.json.tmp")),
    _ = file:delete(filename:join(TestRoot, "market-presence.json")),
    _ = file:delete(filename:join(TestRoot, "retinue_daemon.sock")),
    _ = file:del_dir_r(TestRoot),
    ok.

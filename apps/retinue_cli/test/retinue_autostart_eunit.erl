-module(retinue_autostart_eunit).
-include_lib("eunit/include/eunit.hrl").

late_paths_test_() ->
    {timeout, 10, fun late_paths/0}.

late_paths() ->
    Root = filename:join("/tmp", "retinue-startup-" ++ os:getpid() ++ "-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    Script = filename:join(Root, "daemon with $ and %"),
    State = filename:join(Root, "state link"),
    Target = filename:join(Root, "late state"),
    ok = filelib:ensure_dir(Script),
    try
        ok = file:make_symlink(Target, State),
        ok = file:make_dir(Target),
        wait_then_ready(Script, State, fun() ->
            ok = file:write_file(Script, <<"#!/bin/sh\nexit 0\n">>),
            file:change_mode(Script, 8#700)
        end),
        ok = file:change_mode(Script, 8#600),
        wait_then_ready(Script, State, fun() -> file:change_mode(Script, 8#700) end),
        ok = file:del_dir(Target),
        wait_then_ready(Script, State, fun() -> file:make_dir(Target) end)
    after
        file:del_dir_r(Root)
    end.

wait_then_ready(Script, State, Ready) ->
    Port = open_port({spawn_executable, os:find_executable("timeout")},
                     [binary, exit_status, stderr_to_stdout,
                      {args, ["6", "/bin/sh", "-ec", retinue_autostart:wait_for_paths(),
                              "retinue-wait-paths", Script, State]}]),
    assert_waiting(Port),
    ok = Ready(),
    receive
        {Port, {exit_status, Status}} -> ?assertEqual(0, Status)
    after 7000 -> error(startup_wait_did_not_finish)
    end.

assert_waiting(Port) ->
    receive
        {Port, {exit_status, Status}} -> error({startup_wait_exited_early, Status})
    after 100 -> ok
    end.

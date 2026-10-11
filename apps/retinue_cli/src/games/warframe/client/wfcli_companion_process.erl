-module(wfcli_companion_process).
-export([start/0, stop/0, unit_active/0]).
-ifdef(TEST).
-export([start_arguments/3]).
-endif.
-define(UNIT, "wfcompanion.service").

-doc "Start detached companion in transient user service.".
-spec start() -> {ok, started | already_running, binary()} | {error, term()}.
start() ->
    case unit_active() of
        true -> {ok, already_running, <<>>};
        false ->
            case {os:find_executable("systemd-run"), retinue_native_program:binary(wfcompanion)} of
                {false, _} -> {error, systemd_run_not_found};
                {_, {error, _Reason} = Error} -> Error;
                {SystemdRun, {ok, Companion}} ->
                    _ = reset_failed(),
                    Environment = session_environment(),
                    Args = start_arguments(Companion, wfcli_command(), Environment),
                    case retinue_native_program:run_command(SystemdRun, Args, []) of
                        {ok, Output} -> {ok, started, Output};
                        {error, Reason} -> {error, {companion_start_failed, Reason}}
                    end
            end
    end.

-doc "Stop detached companion transient user service.".
-spec stop() -> {ok, binary()} | {error, term()}.
stop() ->
    case os:find_executable("systemctl") of
        false -> {error, systemctl_not_found};
        Systemctl -> retinue_native_program:run_command(Systemctl, ["--user", "stop", ?UNIT], [])
    end.

-doc "Return whether wfcli-managed transient companion service is active.".
-spec unit_active() -> boolean().
unit_active() ->
    case os:find_executable("systemctl") of
        false -> false;
        Systemctl ->
            case retinue_native_program:run_command_raw(Systemctl, ["--user", "is-active", "--quiet", ?UNIT], []) of
                {0, _Output} -> true;
                {error, _Reason} -> false;
                {_Status, _Output} -> false
            end
    end.

start_arguments(Companion, Wfcli, Environment) ->
    ["--user",
     "--unit=" ++ filename:rootname(?UNIT),
     "--collect",
     "--service-type=exec",
     "--property=KillMode=mixed"]
    ++ ["--setenv=" ++ Name ++ "=" ++ Value || {Name, Value} <- Environment]
    ++ ["--setenv=RETINUE_COMMAND=" ++ Wfcli,
        "--",
        Companion].

session_environment() ->
    Names = ["PATH", "DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "XDG_CURRENT_DESKTOP",
             "DBUS_SESSION_BUS_ADDRESS"],
    [{Name, Value} || Name <- Names,
                      Value <- [os:getenv(Name)],
                      Value =/= false,
                      Value =/= ""].

reset_failed() ->
    case os:find_executable("systemctl") of
        false -> ok;
        Systemctl ->
            _ = retinue_native_program:run_command_raw(Systemctl, ["--user", "reset-failed", ?UNIT], []),
            ok
    end.

wfcli_command() -> filename:join([retinue_build:install_root(), "bin", "retinue"]).

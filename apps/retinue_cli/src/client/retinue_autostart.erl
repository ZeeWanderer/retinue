%%%-------------------------------------------------------------------
%% Optional systemd user-service installation for retinue_daemon.
%%%-------------------------------------------------------------------
-module(retinue_autostart).

-export([enable/0, disable/0, status/0, installed/0, active/0, start/0, stop/0]).

-ifdef(TEST).
-export([unit_file/1, unit_file/2, unit_path/0, unit_name/0, active_result/1,
         wait_for_paths/0]).
-endif.

-define(COMMAND_TIMEOUT, 30000).

-type status() :: #{manager := systemd | homebrew,
                    path := file:filename_all(), installed := boolean(),
                    enabled := boolean(), active := boolean()}.

-doc "Install, enable, and start systemd user service for persistent retinue_daemon.".
-spec enable() -> {ok, status()} | {error, term()}.
enable() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:enable();
        false -> enable_systemd()
    end.

enable_systemd() ->
    case {systemctl(), release_script()} of
        {{error, _Reason} = Error, _} -> Error;
        {_, {error, _Reason} = Error} -> Error;
        {{ok, Systemctl}, {ok, Script}} ->
            Path = unit_path(),
            ok = ensure_runtime_dirs(),
            case write_unit(Path, unit_file(Script)) of
                ok ->
                    with_commands(
                      Systemctl,
                      [["--user", "daemon-reload"],
                       ["--user", "enable", unit_name()]],
                      fun() -> start_service(Systemctl) end);
                {error, _Reason} = Error -> Error
            end
    end.

-doc "Disable login startup while leaving any running daemon untouched.".
-spec disable() -> {ok, status()} | {error, term()}.
disable() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:disable();
        false -> disable_systemd()
    end.

disable_systemd() ->
    case systemctl() of
        {error, _Reason} = Error -> Error;
        {ok, Systemctl} ->
            case enabled(Systemctl) of
                false -> status_result(Systemctl);
                true ->
                    case run_command(Systemctl, ["--user", "disable", unit_name()]) of
                        {ok, _Output} -> status_result(Systemctl);
                        {error, _Reason} = Error -> Error
                    end
            end
    end.

-doc "Return installed, enabled, and active state of retinue_daemon user service.".
-spec status() -> {ok, status()} | {error, term()}.
status() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:status();
        false -> status_systemd()
    end.

status_systemd() ->
    case systemctl() of
        {ok, Systemctl} -> status_result(Systemctl);
        {error, _Reason} = Error -> Error
    end.

-doc "Return whether wfcli systemd user unit is installed.".
-spec installed() -> boolean().
installed() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:installed();
        false -> installed_systemd()
    end.

installed_systemd() -> filelib:is_regular(unit_path()).

-doc "Return whether installed retinue_daemon user service is active.".
-spec active() -> {ok, boolean()} | {error, term()}.
active() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:active();
        false -> active_systemd()
    end.

active_systemd() ->
    case systemctl() of
        {ok, Systemctl} -> probe_active(Systemctl);
        {error, _Reason} = Error -> Error
    end.

-doc "Start installed retinue_daemon user service without changing login enablement.".
-spec start() -> {ok, status()} | {error, term()}.
start() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:start();
        false -> service_command("start")
    end.

-doc "Stop installed retinue_daemon user service without changing login enablement.".
-spec stop() -> {ok, status()} | {error, term()}.
stop() ->
    case retinue_build:homebrew_install() of
        true -> retinue_homebrew_service:stop();
        false -> service_command("stop")
    end.

service_command(Command) ->
    case systemctl() of
        {error, _Reason} = Error -> Error;
        {ok, Systemctl} ->
            case prepare_service_command(Systemctl, Command) of
                ok ->
                    case run_command(Systemctl, ["--user", Command, unit_name()]) of
                        {ok, _Output} -> status_result(Systemctl);
                        {error, _Reason} = Error -> Error
                    end;
                {error, _Reason} = Error -> Error
            end
    end.

prepare_service_command(Systemctl, "start") ->
    case refresh_unit(Systemctl) of
        ok ->
            reset_failed(Systemctl);
        {error, _Reason} = Error -> Error
    end;
prepare_service_command(_Systemctl, _Command) -> ok.

start_service(Systemctl) ->
    ok = reset_failed(Systemctl),
    case probe_active(Systemctl) of
        {ok, true} -> verify_daemon_started(Systemctl);
        {ok, false} ->
            case retinue_client:stop() of
                {ok, stopped, _Node} ->
                    case run_command(Systemctl, ["--user", "start", unit_name()]) of
                        {ok, _Output} -> verify_daemon_started(Systemctl);
                        {error, Reason} -> {error, {autostart_start_failed, Reason}}
                    end;
                {error, Reason} -> {error, {daemon_stop_failed, Reason}}
            end;
        {error, _} = Error -> Error
    end.

reset_failed(Systemctl) ->
    _ = run_command_raw(Systemctl, ["--user", "reset-failed", unit_name()]),
    ok.

verify_daemon_started(Systemctl) ->
    case retinue_client:start(persistent) of
        {ok, _Status, _Node} -> status_result(Systemctl);
        {error, Reason} -> {error, {autostart_daemon_not_ready, Reason}}
    end.

status_map(Systemctl, Active) ->
    Path = unit_path(),
    #{manager => systemd,
      path => Path,
      installed => installed_systemd(),
      enabled => enabled(Systemctl),
      active => Active}.

status_result(Systemctl) ->
    case probe_active(Systemctl) of
        {ok, Active} -> {ok, status_map(Systemctl, Active)};
        {error, _} = Error -> Error
    end.

enabled(Systemctl) ->
    command_succeeds(Systemctl, ["--user", "is-enabled", "--quiet", unit_name()]).

probe_active(Systemctl) ->
    active_result(run_command_raw(Systemctl, ["--user", "is-active", unit_name()])).

active_result({0, _}) -> {ok, true};
active_result({Code, Output}) when Code =:= 3; Code =:= 4 ->
    case string:trim(Output) of
        <<"activating">> -> {ok, true};
        <<"inactive">> -> {ok, false};
        <<"failed">> -> {ok, false};
        <<"unknown">> -> {ok, false};
        _ -> {error, {service_status_failed, Code, Output}}
    end;
active_result({error, _} = Error) -> Error;
active_result({Code, Output}) -> {error, {service_status_failed, Code, Output}}.

command_succeeds(Executable, Args) ->
    case run_command_raw(Executable, Args) of
        {0, _Output} -> true;
        _ -> false
    end.

with_commands(_Systemctl, [], Next) -> Next();
with_commands(Systemctl, [Args | Rest], Next) ->
    case run_command(Systemctl, Args) of
        {ok, _Output} -> with_commands(Systemctl, Rest, Next);
        {error, _Reason} = Error -> Error
    end.

refresh_unit(Systemctl) ->
    case release_script() of
        {ok, Script} ->
            ok = ensure_runtime_dirs(),
            Path = unit_path(),
            Content = unit_file(Script),
            case file:read_file(Path) of
                {ok, Content} -> ok;
                _ ->
                    case write_unit(Path, Content) of
                        ok ->
                            case run_command(Systemctl, ["--user", "daemon-reload"]) of
                                {ok, _Output} -> ok;
                                {error, _Reason} = Error -> Error
                            end;
                        {error, _Reason} = Error -> Error
                    end
            end;
        {error, _Reason} = Error -> Error
    end.

write_unit(Path, Content) ->
    ok = filelib:ensure_dir(Path),
    Temp = Path ++ ".tmp." ++ os:getpid(),
    case file:write_file(Temp, Content) of
        ok ->
            case file:rename(Temp, Path) of
                ok -> ok;
                {error, Reason} ->
                    _ = file:delete(Temp),
                    {error, {unit_install_failed, Path, Reason}}
            end;
        {error, Reason} -> {error, {unit_write_failed, Temp, Reason}}
    end.

unit_path() ->
    ConfigHome = retinue_paths:base(config),
    filename:join([ConfigHome, "systemd", "user", unit_name()]).

unit_name() ->
    case retinue_build:flavor() of
        dev -> "retinue-daemon-dev.service";
        prod -> "retinue-daemon.service"
    end.

unit_file(Script) ->
    unit_file(Script, service_path()).

unit_file(Script, Path) ->
    Flavor = atom_to_list(retinue_build:flavor()),
    InstallRoot = retinue_build:install_root(),
    UpdateRoot = retinue_build:update_root(),
    CrashDump = retinue_paths:file(state, "erl_crash.dump"),
    StateRoot = retinue_paths:root(state),
    iolist_to_binary([
        "[Unit]\n",
        "Description=Retinue daemon (", Flavor, ")\n\n",
        "[Service]\n",
        "Type=simple\n",
        "Environment=RETINUE_DAEMON_IDLE_POLICY=persistent\n",
        "Environment=", systemd_quote("RETINUE_BUILD_FLAVOR=" ++ Flavor), "\n",
        "Environment=", systemd_quote("RETINUE_INSTALL_ROOT=" ++ InstallRoot), "\n",
        "Environment=", systemd_quote("RETINUE_UPDATE_ROOT=" ++ UpdateRoot), "\n",
        "Environment=", systemd_quote("ERL_CRASH_DUMP=" ++ CrashDump), "\n",
        "Environment=LD_PRELOAD=\n",
        "Environment=LD_LIBRARY_PATH=\n",
        "Environment=", systemd_quote("PATH=" ++ Path), "\n",
        "WorkingDirectory=-", systemd_path(StateRoot), "\n",
        "ExecStartPre=:/bin/sh -ec ", systemd_quote(wait_for_paths()),
        " retinue-wait-paths ", systemd_quote(filename:absname(Script)),
        " ", systemd_quote(StateRoot), "\n",
        "TimeoutStartSec=90\n",
        "ExecStart=", systemd_quote(filename:absname(Script)), " foreground\n",
        "Restart=on-failure\n",
        "RestartSec=2\n",
        "KillMode=mixed\n\n",
        "[Install]\n",
        "WantedBy=default.target\n"
    ]).

wait_for_paths() ->
    %% User services cannot order themselves after mounts in the system manager.
    "until [ -x \"$1\" ] && [ -d \"$2\" ]; do /bin/sleep 1; done".

ensure_runtime_dirs() ->
    ok = retinue_paths:ensure_private(retinue_paths:root(state)),
    retinue_paths:ensure_private(retinue_paths:file(state, "daemon-log")).

service_path() ->
    RuntimeBin = filename:join(code:root_dir(), "bin"),
    ErlBin = case os:find_executable("erl") of
        false -> [];
        Erl -> [filename:dirname(Erl)]
    end,
    string:join(ErlBin ++ [RuntimeBin, "/usr/local/bin", "/usr/bin", "/bin",
                           "/usr/local/sbin", "/usr/sbin"], ":").

systemd_quote(Value) -> [$", escape_systemd(Value), $"].

systemd_path([]) -> [];
systemd_path([$\s | Rest]) -> "\\x20" ++ systemd_path(Rest);
systemd_path([$\t | Rest]) -> "\\x09" ++ systemd_path(Rest);
systemd_path([$% | Rest]) -> "%%" ++ systemd_path(Rest);
systemd_path([$\\ | Rest]) -> "\\x5c" ++ systemd_path(Rest);
systemd_path([Char | Rest]) -> [Char | systemd_path(Rest)].

escape_systemd([]) -> [];
escape_systemd([$% | Rest]) -> "%%" ++ escape_systemd(Rest);
escape_systemd([$\\ | Rest]) -> "\\\\" ++ escape_systemd(Rest);
escape_systemd([$" | Rest]) -> "\\\"" ++ escape_systemd(Rest);
escape_systemd([Char | Rest]) -> [Char | escape_systemd(Rest)].

release_script() ->
    case [Path || Path <- retinue_client:release_script_candidates(), filelib:is_regular(Path)] of
        [Path | _] -> {ok, Path};
        [] -> {error, {daemon_release_not_found, retinue_client:release_script_candidates()}}
    end.

systemctl() ->
    case os:find_executable("systemctl") of
        false -> {error, systemctl_not_found};
        Path -> {ok, Path}
    end.

run_command(Executable, Args) ->
    case run_command_raw(Executable, Args) of
        {0, Output} -> {ok, Output};
        {error, _Reason} = Error -> Error;
        {Status, Output} -> {error, {systemctl_failed, Status, Output}}
    end.

run_command_raw(Executable, Args) ->
    try
        Port = open_port(
                 {spawn_executable, Executable},
                 [binary, exit_status, stderr_to_stdout, {args, Args},
                  {env, retinue_client:release_environment()}]),
        collect_port(Port, [])
    catch
        Class:Reason -> {error, {command_start_failed, Executable, Class, Reason}}
    end.

collect_port(Port, Acc) ->
    receive
        {Port, {data, Data}} -> collect_port(Port, [Data | Acc]);
        {Port, {exit_status, Status}} ->
            {Status, iolist_to_binary(lists:reverse(Acc))}
    after ?COMMAND_TIMEOUT ->
        erlang:port_close(Port),
        {error, command_timeout}
    end.

%%%-------------------------------------------------------------------
%% Homebrew service ownership for packaged retinue_daemon.
%%%-------------------------------------------------------------------
-module(retinue_homebrew_service).

-export([enable/0, disable/0, status/0, installed/0, active/0, start/0, stop/0]).

-ifdef(TEST).
-export([stop_args/0]).
-endif.

-define(COMMAND_TIMEOUT, 30000).
-define(FORMULA, "retinue").
-define(UNIT, "retinue-daemon.service").

-type status() :: #{manager := homebrew, path := file:filename_all(), installed := boolean(),
                    enabled := boolean(), active := boolean()}.

-doc "Register and start retinue_daemon through Homebrew services.".
-spec enable() -> {ok, status()} | {error, term()}.
enable() ->
    case retinue_client:stop() of
        {ok, stopped, _Node} ->
            start();
        {error, _Reason} = Error -> Error
    end.

-doc "Unregister Homebrew autostart while leaving retinue_daemon running.".
-spec disable() -> {ok, status()} | {error, term()}.
disable() ->
    case installed() of
        false -> status();
        true ->
            case brew_command(["services", "stop", ?FORMULA]) of
                {ok, _Output} ->
                    case retinue_client:start(persistent) of
                        {ok, _State, _Node} -> status();
                        {error, _Reason} = Error -> Error
                    end;
                {error, _Reason} = Error -> Error
            end
    end.

-doc "Return Homebrew service registration and runtime state.".
-spec status() -> {ok, status()} | {error, term()}.
status() ->
    case systemctl() of
        {ok, Systemctl} -> {ok, status_map(Systemctl)};
        {error, _Reason} = Error -> Error
    end.

-doc "Return whether Homebrew's retinue_daemon service is registered.".
-spec installed() -> boolean().
installed() -> filelib:is_file(unit_path()).

-doc "Return whether Homebrew's retinue_daemon service is active.".
-spec active() -> {ok, boolean()} | {error, term()}.
active() ->
    case systemctl() of
        {ok, Systemctl} -> {ok, command_succeeds(Systemctl,
                                                  ["--user", "is-active", "--quiet", ?UNIT])};
        {error, _Reason} = Error -> Error
    end.

-doc "Start registered retinue_daemon through Homebrew services.".
-spec start() -> {ok, status()} | {error, term()}.
start() ->
    Action = case installed() andalso not homebrew_owned() of
        true -> "restart";
        false -> "start"
    end,
    case brew_command(["services", Action, ?FORMULA]) of
        {ok, _Output} -> status();
        {error, _Reason} = Error -> Error
    end.

-doc "Stop retinue_daemon while preserving Homebrew login registration.".
-spec stop() -> {ok, status()} | {error, term()}.
stop() ->
    case brew_command(stop_args()) of
        {ok, _Output} -> status();
        {error, _Reason} = Error -> Error
    end.

stop_args() -> ["services", "kill", ?FORMULA].

status_map(Systemctl) ->
    #{manager => homebrew,
      path => unit_path(),
      installed => installed(),
      enabled => command_succeeds(Systemctl,
                                  ["--user", "is-enabled", "--quiet", ?UNIT]),
      active => command_succeeds(Systemctl,
                                 ["--user", "is-active", "--quiet", ?UNIT])}.

unit_path() ->
    ConfigHome = retinue_paths:base(config),
    filename:join([ConfigHome, "systemd", "user", ?UNIT]).

homebrew_owned() ->
    Installed = unit_path(),
    Packaged = filename:join(retinue_build:update_root(), ?UNIT),
    case {file:read_file(Installed), file:read_file(Packaged)} of
        {{ok, Content}, {ok, Content}} -> true;
        _ -> false
    end.

brew_command(Args) ->
    case os:find_executable("brew") of
        false -> {error, brew_not_found};
        Brew -> run_command(Brew, Args)
    end.

systemctl() ->
    case os:find_executable("systemctl") of
        false -> {error, systemctl_not_found};
        Systemctl -> {ok, Systemctl}
    end.

command_succeeds(Executable, Args) ->
    case run_command_raw(Executable, Args) of
        {0, _Output} -> true;
        _ -> false
    end.

run_command(Executable, Args) ->
    case run_command_raw(Executable, Args) of
        {0, Output} -> {ok, Output};
        {error, _Reason} = Error -> Error;
        {Status, Output} -> {error, {brew_services_failed, Status, Output}}
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

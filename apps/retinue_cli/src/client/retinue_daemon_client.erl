%%%-------------------------------------------------------------------
%% Thin client for controlling the persistent retinue_daemon node.
%%%-------------------------------------------------------------------
-module(retinue_daemon_client).

-include_lib("kernel/include/file.hrl").

-export([
    status/0,
    start/0,
    start/1,
    stop/0,
    restart/0,
    restart/1,
    update/1,
    hot_update/1,
    hot_update_beam_dir/0,
    ensure_running/0,
    format_error/1,
    daemon_node/0,
    release_script_candidates/0,
    release_environment/0
]).

-ifdef(TEST).
-export([handshake_compatibility/2, recoverable_contract_mismatch/1,
         readiness_result/2, terminate_port_process/1]).
-endif.

-define(START_RETRIES, 300).
-define(START_SLEEP_MS, 100).
-define(STOP_RETRIES, 50).
-define(STOP_SLEEP_MS, 100).
-define(REQUEST_TIMEOUT_MS, 120000).

-type control_result() ::
    {ok, started | already_running | stopped | restarted, node()} |
    {error, term()}.
-type idle_policy() :: persistent | idle | {idle, pos_integer()}.

-doc "Return daemon status without auto-starting it.".
-spec status() -> {running, node(), map()} | {stopped, node()} | {error, term()}.
status() ->
    with_distribution(
      fun(Node) ->
          case ping(Node) of
              pong ->
                  case daemon_call(Node, status) of
                      {ok, #{status := running} = Status} -> {running, Node, Status};
                      {ok, Other} -> {error, {unexpected_status, Other}};
                      {error, _Reason} = Error -> Error
                  end;
              pang ->
                  {stopped, Node}
          end
      end).

-doc "Start retinue_daemon release if not already reachable.".
-spec start() -> control_result().
start() ->
    with_distribution(
      fun(Node) ->
          case retinue_autostart:installed() of
              true -> start_managed_daemon(Node);
              false -> start_release_daemon(Node)
          end
      end).

-doc "Start or reuse retinue_daemon, then apply explicit runtime idle policy.".
-spec start(idle_policy()) -> control_result().
start(Policy) ->
    case ensure_running() of
        {ok, Status, Node} ->
            case configure_idle_policy(Node, Policy) of
                ok -> {ok, Status, Node};
                {error, Reason} -> {error, Reason}
            end;
        Error -> Error
    end.

-doc "Stop reachable retinue_daemon release; stopped is idempotent.".
-spec stop() -> control_result().
stop() ->
    with_distribution(
      fun(Node) ->
          case retinue_autostart:installed() of
              false -> stop_release_daemon(Node);
              true ->
                  case retinue_autostart:active() of
                      {ok, true} -> stop_managed_daemon(Node);
                      {ok, false} -> stop_release_daemon(Node);
                      {error, Reason} -> {error, {autostart_status_failed, Reason}}
                  end
          end
      end).

-doc "Stop then start daemon. Used by control CLI, not by normal data commands.".
-spec restart() -> control_result().
restart() ->
    restart(persistent).

-doc "Stop then start daemon and apply explicit runtime idle policy.".
-spec restart(idle_policy()) -> control_result().
restart(Policy) ->
    case stop() of
        {ok, stopped, _Node} ->
            case start(Policy) of
                {ok, _Status, Node1} -> {ok, restarted, Node1};
                Error -> Error
            end;
        Error ->
            Error
    end.

-doc "Apply an OTP release upgrade by release package name, starting daemon first when needed.".
-spec update(string()) -> {ok, term()} | {error, term()}.
update(ReleaseName) ->
    retinue_client_transport:call({update_release, ReleaseName}).

-doc "Hot-load changed BEAMs from a local build directory into the running daemon.".
-spec hot_update(auto | file:filename_all()) -> {ok, map()} | {error, term()}.
hot_update(auto) ->
    case hot_update_override_requested() of
        true -> hot_update_from_directory();
        false -> hot_update_from_embedded()
    end;
hot_update(Dir) ->
    hot_update_bundles_from_dirs(sibling_ebin_dirs(filename:absname(Dir))).

hot_update_from_embedded() ->
    case retinue_hot_update:read_applications([retinue_core, wfcore, retinue_daemon]) of
        {ok, Bundles} -> hot_update_bundles(Bundles);
        {error, EmbeddedReason} ->
            case hot_update_from_directory() of
                {error, DirectoryReason} ->
                    {error, {hot_update_sources_unavailable,
                             EmbeddedReason, DirectoryReason}};
                Result -> Result
            end
    end.

hot_update_from_directory() ->
    case hot_update_beam_dir() of
        {ok, Dir} -> hot_update_bundles_from_dirs(sibling_ebin_dirs(Dir));
        {error, _Reason} = Error -> Error
    end.

hot_update_override_requested() ->
    case os:getenv("RETINUE_HOT_BEAM_DIR") of
        false -> false;
        undefined -> false;
        "" -> false;
        _Path -> true
    end.

-doc "Find a local build ebin directory used by `daemon update` auto-discovery.".
-spec hot_update_beam_dir() -> {ok, file:filename_all()} | {error, term()}.
hot_update_beam_dir() ->
    Env = case os:getenv("RETINUE_HOT_BEAM_DIR") of
        false -> [];
        undefined -> [];
        "" -> [];
        Path -> [Path]
    end,
    Staged = case retinue_build:hot_ebin_dirs() of
        {ok, StagedDirs} -> lists:reverse(StagedDirs);
        {error, _Reason} -> []
    end,
    Candidates = unique_paths(Env ++ Staged ++ [
        filename:absname("_build/default/lib/retinue_daemon/ebin"),
        filename:absname("_build/prod/lib/retinue_daemon/ebin")
    ]),
    case [Path || Path <- Candidates, filelib:is_dir(Path)] of
        [CandidateDir | _] -> {ok, CandidateDir};
        [] -> {error, {hot_update_beam_dir_not_found, Candidates}}
    end.

-doc "Ensure daemon node is reachable, starting release script if necessary.".
-spec ensure_running() -> {ok, started | already_running, node()} | {error, term()}.
ensure_running() ->
    case start() of
        {ok, Status, Node} ->
            case ensure_compatible(Node) of
                ok -> {ok, Status, Node};
                {error, {daemon_contract_mismatch, Details} = Mismatch} ->
                    case recoverable_contract_mismatch(Details) of
                        true -> recover_compatibility(Status, Node, Mismatch);
                        false -> {error, Mismatch}
                    end;
                {error, {daemon_build_mismatch, _Client, _Server} = Mismatch} ->
                    recover_compatibility(Status, Node, Mismatch);
                {error, {daemon_flavor_mismatch, _Client, _Server} = Mismatch} ->
                    restart_incompatible_daemon(Node, Mismatch, undefined);
                Error -> Error
            end;
        Error -> Error
    end.

ensure_compatible(Node) ->
    ClientContract = retinue_protocol:contract(),
    case daemon_call(Node, {hello, ClientContract}) of
        {ok, Reply} when is_map(Reply) ->
            handshake_compatibility(Reply, ClientContract);
        {ok, Other} -> {error, {invalid_handshake, Other}};
        {error, _Reason} = Error -> Error
    end.

recover_compatibility(Status, Node, Mismatch) ->
    case hot_update(auto) of
        {ok, _} ->
            case ensure_compatible(Node) of
                ok -> {ok, Status, Node};
                {error, StillIncompatible} ->
                    restart_incompatible_daemon(Node, StillIncompatible, undefined)
            end;
        {error, Reason} ->
            case ensure_compatible(Node) of
                ok -> {ok, Status, Node};
                {error, _StillIncompatible} ->
                    restart_incompatible_daemon(Node, Mismatch, Reason)
            end
    end.

restart_incompatible_daemon(Node, Mismatch, UpdateReason) ->
    case stop_release_daemon(Node) of
        {ok, stopped, Node} ->
            case start() of
                {ok, Status, Node} ->
                    case ensure_compatible(Node) of
                        ok -> {ok, Status, Node};
                        {error, Reason} ->
                            {error, {restart_incompatible, Mismatch, UpdateReason, Reason}}
                    end;
                {error, Reason} ->
                    {error, {restart_incompatible, Mismatch, UpdateReason, Reason}}
            end;
        {error, Reason} ->
            {error, {restart_incompatible, Mismatch, UpdateReason, Reason}}
    end.

handshake_compatibility(Reply, ClientContract) when is_map(Reply),
                                                     is_map(ClientContract) ->
    case {retinue_protocol:compatibility(ClientContract, Reply),
          maps:get(compatible, Reply, false)} of
        {ok, true} ->
            ClientFlavor = retinue_build:flavor(),
            case maps:get(flavor, Reply, undefined) of
                ServerFlavor when ServerFlavor =/= undefined,
                                  ServerFlavor =/= ClientFlavor ->
                    {error, {daemon_flavor_mismatch, ClientFlavor, ServerFlavor}};
                _ ->
                    compare_build_identity(Reply)
            end;
        {{error, Mismatches}, _} ->
            {error, {daemon_contract_mismatch, Mismatches}};
        {ok, false} ->
            {error, {daemon_contract_mismatch,
                     maps:get(mismatches, Reply, [daemon_rejected_contract])}}
    end;
handshake_compatibility(Reply, _ClientContract) ->
    {error, {invalid_handshake, Reply}}.

recoverable_contract_mismatch(Mismatches) when is_list(Mismatches),
                                               Mismatches =/= [] ->
    lists:all(fun mismatch_marks_daemon_outdated/1, Mismatches);
recoverable_contract_mismatch(_Mismatches) ->
    false.

mismatch_marks_daemon_outdated(#{required := Required, available := Available})
  when is_integer(Required), Required > 0,
       is_integer(Available), Available > 0 ->
    Required > Available;
mismatch_marks_daemon_outdated(#{required := Required, available := undefined})
  when is_integer(Required), Required > 0 ->
    true;
mismatch_marks_daemon_outdated(_Mismatch) ->
    false.

client_build_identity() ->
    retinue_hot_update:current_build_identity().

compare_build_identity(Reply) ->
    case client_build_identity() of
        {ok, ClientBuild} ->
            ServerBuild = maps:get(build, Reply, undefined),
            case ServerBuild of
                ClientBuild -> ok;
                _ -> {error, {daemon_build_mismatch, ClientBuild, ServerBuild}}
            end;
        {error, Reason} ->
            {error, {client_build_identity_unavailable, Reason}}
    end.

configure_idle_policy(Node, Policy) ->
    case ensure_compatible(Node) of
        ok ->
            case daemon_call(Node, {set_idle_policy, Policy}) of
                {ok, ok} -> ok;
                {ok, {error, Reason}} -> {error, Reason};
                {ok, Other} -> {error, {unexpected_idle_policy_reply, Other}};
                {error, _Reason} = Error -> Error
            end;
        Error -> Error
    end.

hot_update_bundles(Bundles) ->
    %% Explicit updates must bridge protocol changes; compatibility is checked after loading.
    case start() of
        {ok, _Status, Node} ->
            case daemon_call(Node, {hot_update, Bundles}) of
                {ok, {ok, Result}} when is_map(Result) ->
                    case ensure_compatible(Node) of
                        ok -> {ok, Result};
                        {error, _Reason} = Error -> Error
                    end;
                {ok, {error, Reason}} -> {error, Reason};
                {ok, Other} -> {error, {unexpected_hot_update_reply, Other}};
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

unique_paths(Paths) ->
    lists:reverse(
      lists:foldl(
        fun(Path, Acc) ->
            Absolute = filename:absname(Path),
            case lists:member(Absolute, Acc) of
                true -> Acc;
                false -> [Absolute | Acc]
            end
        end,
        [],
        Paths)).

hot_update_bundles_from_dirs(Dirs) ->
    case retinue_hot_update:read_directories(Dirs) of
        {ok, Bundles} -> hot_update_bundles(Bundles);
        {error, _Reason} = Error -> Error
    end.

sibling_ebin_dirs(Dir) ->
    case filename:basename(Dir) of
        "ebin" ->
            LibDir = filename:dirname(filename:dirname(Dir)),
            Dirs = [filename:join([LibDir, "retinue_core", "ebin"]),
                    filename:join([LibDir, "wfcore", "ebin"]),
                    filename:join([LibDir, "retinue_daemon", "ebin"])],
            case [Path || Path <- Dirs, filelib:is_dir(Path)] of
                [] -> [Dir];
                Existing -> Existing
            end;
        _ -> [Dir]
    end.

-doc "Format daemon/protocol failures for CLI error messages.".
-spec format_error(term()) -> string().
format_error({daemon_contract_mismatch, Mismatches}) ->
    lists:flatten(io_lib:format(
      "client requires daemon interfaces not offered by the running build: ~p",
      [Mismatches]));
format_error({daemon_build_mismatch, Client, Server}) ->
    lists:flatten(io_lib:format(
      "client daemon build ~s differs from running daemon build ~s",
      [format_build(Client), format_build(Server)]));
format_error({restart_incompatible, Mismatch, UpdateReason, RestartReason}) ->
    lists:flatten(io_lib:format(
      "automatic daemon recovery failed (~p, update: ~p): ~p",
      [Mismatch, UpdateReason, RestartReason]));
format_error({daemon_flavor_mismatch, Client, Server}) ->
    lists:flatten(io_lib:format(
      "client flavor ~p differs from running daemon flavor ~p", [Client, Server]));
format_error({source_unavailable, Kind, Id, Path, Reason, custom}) ->
    lists:flatten(io_lib:format("custom ~p source ~s is unavailable at ~s: ~p",
                                [Kind, Id, Path, Reason]));
format_error({source_unavailable, Kind, Id, Path, Reason, managed}) ->
    lists:flatten(io_lib:format("managed ~p source ~s could not be prepared at ~s: ~p",
                                [Kind, Id, Path, Reason]));
format_error({query_errors, Errors}) when is_list(Errors) ->
    string:join([retinue_text:to_list(Error) || Error <- Errors], "; ");
format_error(Reason) -> lists:flatten(io_lib:format("~p", [Reason])).

format_build(undefined) -> "unknown";
format_build(Build) when is_binary(Build) -> binary_to_list(Build);
format_build(Build) -> lists:flatten(io_lib:format("~p", [Build])).

-doc "Return fixed loopback daemon node name.".
-spec daemon_node() -> node().
daemon_node() ->
    'retinue_daemon@localhost'.

-doc "Candidate release-control scripts used by `wfcli daemon start`.".
-spec release_script_candidates() -> [file:filename_all()].
release_script_candidates() ->
    Env =
        case os:getenv("RETINUE_DAEMON_SCRIPT") of
            false -> [];
            undefined -> [];
            "" -> [];
            Path -> [Path]
        end,
    unique_paths(Env ++ [
        filename:join([retinue_build:update_root(), "bin", "retinue-daemon"]),
        filename:join([retinue_build:install_root(), "bin", "retinue-daemon"]),
        filename:absname("_build/default/rel/retinue_daemon/bin/retinue_daemon"),
        filename:absname("_build/prod/rel/retinue_daemon/bin/retinue_daemon")
    ] ++ path_candidate()).

with_distribution(Fun) ->
    Node = daemon_node(),
    case ensure_node_started(Node) of
        ok -> Fun(Node);
        {error, Reason} -> {error, Reason}
    end.

ensure_node_started(_DaemonNode) ->
    ensure_epmd_started(),
    ok = application:set_env(kernel, inet_dist_use_interface, {127, 0, 0, 1}),
    StartResult =
        case node() of
            nonode@nohost ->
                net_kernel:start(undefined, #{
                    name_domain => shortnames,
                    dist_listen => false,
                    hidden => true
                });
            _ ->
                {ok, self()}
        end,
    case StartResult of
        {ok, _Pid} ->
            ok;
        {error, {already_started, _Pid}} ->
            ok;
        {error, Reason} ->
            {error, {net_kernel_start_failed, Reason}}
    end.

ensure_epmd_started() ->
    case os:find_executable("epmd") of
        false -> ok;
        Epmd ->
            _ = run_epmd(Epmd),
            ok
    end.

run_epmd(Epmd) ->
    try
        Port = open_port({spawn_executable, Epmd},
                         [binary, exit_status, stderr_to_stdout, {args, ["-daemon"]}]),
        collect_epmd(Port, [])
    catch
        Class:Reason ->
            {error, {epmd_start_failed, Class, Reason}}
    end.

collect_epmd(Port, Acc) ->
    receive
        {Port, {data, Data}} ->
            collect_epmd(Port, [Data | Acc]);
        {Port, {exit_status, 0}} ->
            ok;
        {Port, {exit_status, Status}} ->
            {error, {epmd_failed, Status, iolist_to_binary(lists:reverse(Acc))}}
    after 2000 ->
        port_close(Port),
        {error, epmd_timeout}
    end.

ping(Node) ->
    case net_kernel:connect_node(Node) of
        true -> pong;
        false -> pang;
        ignored -> pang
    end.

start_managed_daemon(Node) ->
    case retinue_autostart:active() of
        {ok, true} ->
            case retinue_autostart:start() of
                {ok, _ServiceStatus} ->
                    case wait_for_daemon(Node, ?START_RETRIES) of
                        {ok, started, Node} -> {ok, already_running, Node};
                        {error, _Reason} = Error -> Error
                    end;
                {error, Reason} -> {error, {autostart_start_failed, Reason}}
            end;
        {ok, false} ->
            case stop_release_daemon(Node) of
                {ok, stopped, Node} -> start_systemd_daemon(Node, started);
                {error, _Reason} = Error -> Error
            end;
        {error, Reason} -> {error, {autostart_status_failed, Reason}}
    end.

start_systemd_daemon(Node, Status) ->
    case retinue_autostart:start() of
        {ok, _ServiceStatus} ->
            case wait_for_daemon(Node, ?START_RETRIES) of
                {ok, started, Node} -> {ok, Status, Node};
                {error, _Reason} = Error -> Error
            end;
        {error, Reason} -> {error, {autostart_start_failed, Reason}}
    end.

start_release_daemon(Node) ->
    case ping(Node) of
        pong ->
            case wait_for_daemon(Node, ?START_RETRIES) of
                {ok, started, Node} -> {ok, already_running, Node};
                {error, _Reason} = Error -> Error
            end;
        pang -> start_release_daemon_script(Node)
    end.

start_release_daemon_script(Node) ->
    case find_release_script() of
        {ok, Script} ->
            case run_release_script(Script, ["daemon"]) of
                {ok, _Output} ->
                    wait_for_daemon(Node, ?START_RETRIES);
                {error, Reason} ->
                    case wait_for_daemon(Node, ?START_RETRIES) of
                        {ok, started, Node} -> {ok, already_running, Node};
                        {error, _} -> {error, Reason}
                    end
            end;
        {error, Reason} ->
            {error, Reason}
    end.

stop_managed_daemon(Node) ->
    case ping(Node) of
        pong -> _ = daemon_call(Node, stop);
        pang -> ok
    end,
    case retinue_autostart:stop() of
        {ok, _ServiceStatus} -> wait_for_pang(Node, ?STOP_RETRIES);
        {error, Reason} -> {error, {autostart_stop_failed, Reason}}
    end.

stop_release_daemon(Node) ->
    case ping(Node) of
        pang ->
            {ok, stopped, Node};
        pong ->
            case daemon_call(Node, stop) of
                {ok, ok} ->
                    wait_for_pang(Node, ?STOP_RETRIES);
                {ok, Other} ->
                    {error, {unexpected_stop_reply, Other}};
                {error, _Reason} = Error -> Error
            end
    end.

daemon_call(Node, Request) ->
    try gen_server:call({retinue_daemon, Node}, Request, ?REQUEST_TIMEOUT_MS) of
        Reply -> {ok, Reply}
    catch
        exit:{nodedown, _Node} -> {error, {daemon_call_failed, noconnection}};
        exit:{{nodedown, _Node}, _Call} -> {error, {daemon_call_failed, noconnection}};
        exit:{noproc, _Call} -> {error, {daemon_call_failed, noproc}};
        exit:Reason -> {error, {daemon_call_failed, Reason}}
    end.

find_release_script() ->
    case [Path || Path <- release_script_candidates(), is_regular_file(Path)] of
        [Path | _] -> {ok, Path};
        [] -> {error, {daemon_release_not_found, release_script_candidates()}}
    end.

is_regular_file(Path) ->
    case file:read_file_info(Path) of
        {ok, #file_info{type = regular}} -> true;
        _ -> false
    end.

path_candidate() ->
    case os:find_executable("retinue-daemon") of
        false -> [];
        Path -> [Path]
    end.

run_release_script(Script, Args) ->
    ok = ensure_runtime_dirs(),
    try
        Port = open_port({spawn_executable, Script},
                         [binary, exit_status, stderr_to_stdout,
                          {args, Args}, {env, release_environment()}]),
        collect_port(Port, [])
    catch
        Class:Reason ->
            {error, {start_failed, Script, Class, Reason}}
    end.

-doc "Return clean overrides for native daemon and control-process children.".
-spec release_environment() -> [{string(), string() | false}].
release_environment() ->
    Flavor = atom_to_list(retinue_build:flavor()),
    [{"RUNNER_LOG_DIR", retinue_paths:file(state, "daemon-log")},
     {"ERL_CRASH_DUMP", retinue_paths:file(state, "erl_crash.dump")},
     {"RETINUE_BUILD_FLAVOR", Flavor},
     {"RETINUE_INSTALL_ROOT", retinue_build:install_root()},
     {"RETINUE_UPDATE_ROOT", retinue_build:update_root()},
     {"LD_PRELOAD", false},
     {"LD_LIBRARY_PATH", false},
     {"STEAM_RUNTIME", false},
     {"STEAM_RUNTIME_LIBRARY_PATH", false}].

ensure_runtime_dirs() ->
    ok = retinue_paths:ensure_private(retinue_paths:root(state)),
    retinue_paths:ensure_private(retinue_paths:file(state, "daemon-log")).

collect_port(Port, Acc) ->
    receive
        {Port, {data, Data}} ->
            collect_port(Port, [Data | Acc]);
        {Port, {exit_status, 0}} ->
            {ok, iolist_to_binary(lists:reverse(Acc))};
        {Port, {exit_status, Status}} ->
            {error, {daemon_script_failed, Status, iolist_to_binary(lists:reverse(Acc))}}
    after 30000 ->
        terminate_port_process(Port),
        {error, daemon_script_timeout}
    end.

terminate_port_process(Port) ->
    case erlang:port_info(Port, os_pid) of
        {os_pid, OsPid} ->
            send_term_signal(OsPid),
            wait_for_port_exit(Port);
        undefined -> ok
    end.

send_term_signal(OsPid) ->
    case os:find_executable("kill") of
        false -> ok;
        Kill ->
            try
                SignalPort = open_port(
                  {spawn_executable, Kill},
                  [binary, exit_status, stderr_to_stdout,
                   {args, ["-TERM", integer_to_list(OsPid)]}]),
                collect_signal_port(SignalPort)
            catch
                _:_ -> ok
            end
    end.

collect_signal_port(Port) ->
    receive
        {Port, {data, _Data}} -> collect_signal_port(Port);
        {Port, {exit_status, _Status}} -> ok
    after 1000 ->
        close_port(Port),
        ok
    end.

wait_for_port_exit(Port) ->
    receive
        {Port, {data, _Data}} -> wait_for_port_exit(Port);
        {Port, {exit_status, _Status}} -> ok
    after 1000 ->
        close_port(Port),
        ok
    end.

close_port(Port) ->
    try port_close(Port) of
        true -> ok
    catch
        error:badarg -> ok
    end.

wait_for_daemon(Node, 0) ->
    {error, {daemon_not_ready, Node}};
wait_for_daemon(Node, Retries) ->
    Ping = ping(Node),
    Call = case Ping of
        pong -> daemon_call(Node, {hello, retinue_protocol:contract()});
        pang -> not_called
    end,
    case readiness_result(Ping, Call) of
        ready ->
            {ok, started, Node};
        retry ->
            timer:sleep(?START_SLEEP_MS),
            wait_for_daemon(Node, Retries - 1)
    end.

readiness_result(pong, {ok, _Reply}) -> ready;
readiness_result(_Ping, _Call) -> retry.

wait_for_pang(Node, 0) ->
    {error, {daemon_still_running, Node}};
wait_for_pang(Node, Retries) ->
    case ping(Node) of
        pang -> {ok, stopped, Node};
        pong ->
            timer:sleep(?STOP_SLEEP_MS),
            wait_for_pang(Node, Retries - 1)
    end.

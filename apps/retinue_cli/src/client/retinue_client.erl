%%%-------------------------------------------------------------------
%% Stable public facade for daemon lifecycle and request transport.
%%%-------------------------------------------------------------------
-module(retinue_client).

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
    call/1,
    call/2,
    one_shot/1,
    one_shot/2,
    subscribe/1,
    subscribe/2,
    next/2,
    unsubscribe/1,
    ensure_running/0,
    format_error/1,
    daemon_node/0,
    release_script_candidates/0,
    release_environment/0
]).

-ifdef(TEST).
-export([handshake_compatibility/2, readiness_result/2, terminate_port_process/1]).
-endif.

-type control_result() ::
    {ok, started | already_running | stopped | restarted, node()} | {error, term()}.
-type idle_policy() :: persistent | idle | {idle, pos_integer()}.

-doc "Return daemon status without auto-starting it.".
-spec status() -> {running, node(), map()} | {stopped, node()} | {error, term()}.
-ifdef(TEST).
status() ->
    case application:get_env(retinue_cli, test_local_daemon, false) of
        true -> {running, node(), retinue_daemon:request(status)};
        false -> retinue_daemon_client:status()
    end.
-else.
status() -> retinue_daemon_client:status().
-endif.

-doc "Start retinue_daemon using its configured persistent policy.".
-spec start() -> control_result().
start() -> retinue_daemon_client:start().

-doc "Start retinue_daemon and apply one idle policy.".
-spec start(idle_policy()) -> control_result().
start(Policy) -> retinue_daemon_client:start(Policy).

-doc "Stop retinue_daemon if running.".
-spec stop() -> control_result().
stop() -> retinue_daemon_client:stop().

-doc "Restart retinue_daemon using its persistent policy.".
-spec restart() -> control_result().
restart() -> retinue_daemon_client:restart().

-doc "Restart retinue_daemon and apply one idle policy.".
-spec restart(idle_policy()) -> control_result().
restart(Policy) -> retinue_daemon_client:restart(Policy).

-doc "Apply an OTP release upgrade by release package name.".
-spec update(string()) -> {ok, term()} | {error, term()}.
update(ReleaseName) -> retinue_daemon_client:update(ReleaseName).

-doc "Hot-load daemon BEAMs from embedded code or one build directory.".
-spec hot_update(auto | file:filename_all()) -> {ok, map()} | {error, term()}.
hot_update(Source) -> retinue_daemon_client:hot_update(Source).

-doc "Find the local daemon BEAM directory used for development hot updates.".
-spec hot_update_beam_dir() -> {ok, file:filename_all()} | {error, term()}.
hot_update_beam_dir() -> retinue_daemon_client:hot_update_beam_dir().

-doc "Call daemon request API, starting it when absent.".
-spec call(term()) -> {ok, term()} | {error, term()}.
call(Request) -> retinue_client_transport:call(Request).
call(Game, Request) -> call({game, Game, Request}).

-doc "Submit one request and wait for its asynchronous response.".
-spec one_shot(map()) -> {ok, term()} | {error, term()}.
one_shot(Request) -> retinue_client_transport:one_shot(Request).
one_shot(Game, Request) -> one_shot(Request#{game => Game}).

-doc "Register a persistent daemon request stream.".
-spec subscribe(map()) -> {ok, map()} | {error, term()}.
subscribe(Request) -> retinue_client_transport:subscribe(Request).
subscribe(Game, Request) -> subscribe(Request#{game => Game}).

-doc "Wait for the next response from a request stream.".
-spec next(map(), timeout()) -> {ok, term()} | {error, term()}.
next(Handle, Timeout) -> retinue_client_transport:next(Handle, Timeout).

-doc "Cancel a daemon request stream.".
-spec unsubscribe(map()) -> ok | {error, term()}.
unsubscribe(Handle) -> retinue_client_transport:unsubscribe(Handle).

-doc "Return a compatible daemon, starting or updating it when needed.".
-spec ensure_running() -> {ok, started | already_running, node()} | {error, term()}.
ensure_running() -> retinue_daemon_client:ensure_running().

-doc "Format daemon and protocol failures for terminal output.".
-spec format_error(term()) -> string().
format_error(Reason) -> retinue_daemon_client:format_error(Reason).

-doc "Return configured distributed daemon node name.".
-spec daemon_node() -> node().
daemon_node() -> retinue_daemon_client:daemon_node().

-doc "Return daemon release-script discovery candidates.".
-spec release_script_candidates() -> [file:filename_all()].
release_script_candidates() -> retinue_daemon_client:release_script_candidates().

-doc "Return environment passed to a spawned daemon release.".
-spec release_environment() -> [{string(), string() | false}].
release_environment() -> retinue_daemon_client:release_environment().

-ifdef(TEST).
handshake_compatibility(Reply, Contract) ->
    retinue_daemon_client:handshake_compatibility(Reply, Contract).

readiness_result(Ping, Call) -> retinue_daemon_client:readiness_result(Ping, Call).

terminate_port_process(Port) -> retinue_daemon_client:terminate_port_process(Port).
-endif.

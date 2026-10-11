%%%-------------------------------------------------------------------
%% Persistent wfcli service process.
%%%-------------------------------------------------------------------
-module(retinue_daemon).

-behaviour(gen_server).

-export([start_link/0, request/1, submit/2, subscribe/2, unsubscribe/1,
         handshake_version/0]).

-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(CALL_TIMEOUT_MS, 120000).
-define(ARTIFACT_CHECK_MS, 5000).

-type request() ::
    status |
    stop |
    player_snapshot |
    player_clear |
    resolution_issues |
    notification_settings |
    {notification_settings, map()} |
    {companion_command, map()} |
    {hello, term()} |
    {set_idle_policy, persistent | idle | {idle, pos_integer()}} |
    {hot_update, [map()]} |
    {update_release, string()}.
-type status() :: #{
    status := running,
    node := node(),
    pid := pid(),
    uptime_ms := non_neg_integer(),
    version := string() | undefined,
    otp_release := string(),
    installation := homebrew | local,
    install_root := file:filename_all(),
    protocols := #{erlang_distribution_rpc := map(),
                   native_socket_api := map()},
    build := binary() | undefined
}.
-type reply() ::
    status() |
    ok |
    {ok, term()} |
    {ok, term(), term()} |
    {error, term()}.
-type state() :: #{
    started_at := integer(),
    artifact_id := binary() | undefined,
    artifact_update := false | #{artifact_id := binary(), monitor := reference(), pid := pid()},
    manual_update := false | #{token := reference(), monitor := reference(), pid := pid(),
                               from := gen_server:from()}
}.

-doc "Start persistent service registered as `retinue_daemon`; supervisor owns this in release mode.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Synchronous daemon API; keep requests data-only so remote CLI calls stay upgrade-safe.".
-spec request(request()) -> reply().
request(Request) ->
    gen_server:call(?SERVER, Request, ?CALL_TIMEOUT_MS).

-doc "Queue one immediate request and return its asynchronous reply reference.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}, ?CALL_TIMEOUT_MS).

-doc "Register one persistent subscription and return its update reference.".
-spec subscribe(pid(), map()) -> {ok, reference()} | {error, term()}.
subscribe(Client, Request) ->
    gen_server:call(?SERVER, {subscribe, Client, Request}, ?CALL_TIMEOUT_MS).

-doc "Cancel a queued request or persistent subscription.".
-spec unsubscribe(reference()) -> ok | {error, term()}.
unsubscribe(Ref) ->
    gen_server:call(?SERVER, {unsubscribe, Ref}, ?CALL_TIMEOUT_MS).

-doc "Erlang distribution handshake-envelope version.".
-spec handshake_version() -> pos_integer().
handshake_version() -> retinue_protocol:handshake_version().

-doc "Initialize daemon state. Future watch registrations should live under this process tree.".
-spec init([]) -> {ok, state()}.
init([]) ->
    schedule_artifact_check(),
    {ok, #{started_at => erlang:monotonic_time(millisecond),
           artifact_id => current_artifact_id(), artifact_update => false,
           manual_update => false}}.

-spec handle_call(term(), gen_server:from(), state()) ->
    {reply, term(), state()} | {noreply, state()}.
handle_call(status, _From, State) ->
    {reply, status(State), State};
handle_call({hello, ClientContract}, _From, State) ->
    Reply0 = (retinue_protocol:negotiate(ClientContract))#{
               version => retinue_build:version(),
               flavor => retinue_build:flavor(),
               node => node()},
    Reply = with_build_identity(Reply0),
    {reply, Reply, State};
handle_call({submit, Client, Request}, _From, State) ->
    Reply = safe_submit(Client, Request),
    {reply, Reply, State};
handle_call({subscribe, Client, #{game := Game} = Request}, _From, State) ->
    {reply, retinue_games:subscribe(Game, Client, maps:remove(game, Request)), State};
handle_call({unsubscribe, {Game, Ref}}, _From, State) ->
    {reply, retinue_games:unsubscribe(Game, Ref), State};
handle_call({set_idle_policy, Policy}, _From, State) ->
    {reply, retinue_activity:set_idle_policy(Policy), State};
handle_call({game, Game, Request}, _From, State) ->
    {reply, retinue_games:request(Game, Request), State};
handle_call({hot_update, Bundles}, From, State) when is_list(Bundles) ->
    case update_in_progress(State) of
        true -> {reply, {error, update_in_progress}, State};
        false -> {noreply, start_manual_update(Bundles, From, State)}
    end;
handle_call(stop, _From, State) ->
    _ = spawn(fun stop_node_after_reply/0),
    {reply, ok, State};
handle_call({update_release, ReleaseName}, _From, State) when is_list(ReleaseName) ->
    case update_in_progress(State) of
        true -> {reply, {error, update_in_progress}, State};
        false -> {reply, update_release(ReleaseName), State}
    end;
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast({diagnostics_consume, Client, Ref}, State) ->
    retinue_local_api:diagnostics_consume(Client, Ref),
    {noreply, State};
handle_cast(_Msg, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info(artifact_check, State) ->
    schedule_artifact_check(),
    {noreply, check_artifact_update(State)};
handle_info({artifact_update, ArtifactId, Result},
            State = #{artifact_update := #{artifact_id := ArtifactId, monitor := Monitor}}) ->
    erlang:demonitor(Monitor, [flush]),
    case Result of
        {ok, _Info} -> {noreply, State#{artifact_id => ArtifactId, artifact_update => false}};
        {error, Reason} ->
            logger:warning("retinue_daemon artifact update failed: ~p", [Reason]),
            {noreply, State#{artifact_update => false}}
    end;
handle_info({artifact_update, _ArtifactId, _Result}, State) ->
    {noreply, State};
handle_info({manual_update, Token, Result},
            State = #{manual_update := #{token := Token, monitor := Monitor,
                                         from := From}}) ->
    erlang:demonitor(Monitor, [flush]),
    gen_server:reply(From, Result),
    {noreply, State#{manual_update => false}};
handle_info({manual_update, _Token, _Result}, State) ->
    {noreply, State};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{artifact_update := #{monitor := Monitor}}) ->
    logger:warning("retinue_daemon artifact updater stopped: ~p", [Reason]),
    {noreply, State#{artifact_update => false}};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{manual_update := #{monitor := Monitor, from := From}}) ->
    gen_server:reply(From, {error, {hot_update_worker_down, Reason}}),
    {noreply, State#{manual_update => false}};
handle_info(_Msg, State) ->
    {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, _State) ->
    ok.

-doc "OTP hot-upgrade hook. Real state migration belongs here once appup/relup exists.".
-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    retinue_incident_log:install(),
    Update = case maps:get(artifact_update, State, false) of
        #{artifact_id := _, monitor := _, pid := _} = Active -> Active;
        _ -> false
    end,
    Manual = case maps:get(manual_update, State, false) of
        #{token := _, monitor := _, pid := _, from := _} = ActiveManual -> ActiveManual;
        _ -> false
    end,
    {ok, State#{artifact_update => Update, manual_update => Manual}}.

status(State = #{started_at := StartedAt}) ->
    Now = erlang:monotonic_time(millisecond),
    Games = retinue_games:status(),
    with_build_identity(#{
        status => running,
        node => node(),
        pid => self(),
        uptime_ms => max(0, Now - StartedAt),
        version => retinue_build:version(),
        otp_release => erlang:system_info(otp_release),
        protocols => #{erlang_distribution_rpc => retinue_protocol:contract(),
                       native_socket_api => retinue_local_protocol:contract(),
                       poe2_socket_api => retinue_local_protocol:contract(<<"poe2">>)},
        installation => retinue_build:installation(),
        install_root => retinue_build:install_root(),
        flavor => retinue_build:flavor(),
        service => safe_status(retinue_activity),
        incident_log => retinue_incident_log:path(),
        local_api => safe_status(retinue_local_api),
        games => Games,
        paths => retinue_games:paths(Games),
        update => update_status(State)
    }).

update_status(#{manual_update := Manual, artifact_update := Artifact}) ->
    case {Manual, Artifact} of
        {false, false} -> idle;
        {#{}, false} -> manual;
        {false, #{}} -> artifact;
        {#{}, #{}} -> both
    end.

with_build_identity(Map) ->
    case retinue_hot_update:current_build_identity() of
        {ok, Identity} -> Map#{build => Identity};
        {error, Reason} -> Map#{build => undefined, build_error => Reason}
    end.

safe_status(Module) ->
    try Module:status() catch _:_ -> unavailable end.

safe_submit(Client, #{game := Game} = Request) ->
    retinue_games:submit(Game, Client, maps:remove(game, Request));
safe_submit(_Client, _Request) -> {error, game_required}.

stop_node_after_reply() ->
    timer:sleep(100),
    init:stop().

update_release([]) ->
    {error, release_name_required};
update_release(ReleaseName) ->
    case release_handler:unpack_release(ReleaseName) of
        {ok, Vsn} ->
            install_release(Vsn);
        {error, {existing_release, Vsn}} ->
            install_release(Vsn);
        Error ->
            Error
    end.

install_release(Vsn) ->
    case release_handler:install_release(Vsn) of
        {ok, _OldVsn, _Descr} ->
            release_handler:make_permanent(Vsn);
        {ok, _OldVsn, _Descr, _Warnings} ->
            release_handler:make_permanent(Vsn);
        Error ->
            Error
    end.

schedule_artifact_check() ->
    erlang:send_after(?ARTIFACT_CHECK_MS, self(), artifact_check).

check_artifact_update(State) ->
    try maybe_start_artifact_update(State)
    catch
        Class:Reason:Stacktrace ->
            logger:error("retinue_daemon artifact check failed: ~p", [{Class, Reason, Stacktrace}]),
            State
    end.

current_artifact_id() ->
    case retinue_build:artifact_id() of
        {ok, ArtifactId} -> ArtifactId;
        {error, _Reason} -> undefined
    end.

update_in_progress(State) ->
    maps:get(artifact_update, State, false) =/= false orelse
        maps:get(manual_update, State, false) =/= false.

start_manual_update(Bundles, From, State) ->
    Daemon = self(),
    Token = make_ref(),
    {Pid, Monitor} = spawn_monitor(fun() ->
        Daemon ! {manual_update, Token, safe_hot_update(Bundles)}
    end),
    State#{manual_update => #{token => Token, monitor => Monitor,
                              pid => Pid, from => From}}.

maybe_start_artifact_update(
  State = #{artifact_id := Current, artifact_update := false, manual_update := false}) ->
    case retinue_build:artifact_id() of
        {ok, Current} -> State;
        {ok, ArtifactId} ->
            Daemon = self(),
            {Pid, Monitor} = spawn_monitor(fun() ->
                Result = case retinue_build:hot_ebin_dirs() of
                    {ok, Dirs} ->
                        case retinue_hot_update:read_directories(Dirs) of
                            {ok, Bundles} -> retinue_hot_update:apply(Bundles);
                            {error, _Reason} = Error -> Error
                        end;
                    {error, _Reason} = Error -> Error
                end,
                Daemon ! {artifact_update, ArtifactId, Result}
            end),
            State#{artifact_update => #{artifact_id => ArtifactId,
                                        monitor => Monitor, pid => Pid}};
        {error, _Reason} -> State
    end;
maybe_start_artifact_update(State) -> State.

safe_hot_update(Bundles) ->
    try execute_hot_update(Bundles)
    catch
        Class:Reason:Stacktrace -> {error, {hot_update_crash, Class, Reason, Stacktrace}}
    end.

-ifdef(TEST).
execute_hot_update(Bundles) ->
    case application:get_env(retinue_daemon, daemon_hot_update_fun, undefined) of
        Fun when is_function(Fun, 1) -> Fun(Bundles);
        undefined -> retinue_hot_update:apply(Bundles)
    end.
-else.
execute_hot_update(Bundles) -> retinue_hot_update:apply(Bundles).
-endif.

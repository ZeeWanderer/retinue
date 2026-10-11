%%%-------------------------------------------------------------------
%% Warframe.market WebSocket status owner.
%%%-------------------------------------------------------------------
-module(wfcli_market_presence_service).

-behaviour(gen_server).

-export([start_link/0, token_changed/1, snapshot/0, status/0, set_mode/1,
         subscribe/1, unsubscribe/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(RETRY_MIN_MS, 1000).
-define(RETRY_MAX_MS, 60000).

-doc "Start the daemon-owned Market presence connection.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() -> gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Replace the JWT used for the current or next socket connection.".
-spec token_changed(binary() | undefined) -> ok.
token_changed(Token) -> gen_server:cast(?SERVER, {token_changed, Token}).

-doc "Return JSON-safe Market presence state.".
-spec snapshot() -> map().
snapshot() -> gen_server:call(?SERVER, snapshot).

-doc "Return Market presence state for daemon diagnostics.".
-spec status() -> map().
status() -> snapshot().

-doc "Persist and apply `auto`, `online`, `ingame`, or `invisible` mode.".
-spec set_mode(binary()) -> {ok, map()} | {error, term()}.
set_mode(Mode) -> gen_server:call(?SERVER, {set_mode, Mode}).

-doc "Subscribe a local process to presence changes.".
-spec subscribe(pid()) -> {ok, reference(), map()}.
subscribe(Client) -> gen_server:call(?SERVER, {subscribe, Client}).

-doc "Remove one presence subscription.".
-spec unsubscribe(reference()) -> ok.
unsubscribe(Ref) -> gen_server:call(?SERVER, {unsubscribe, Ref}).

init([]) ->
    ModePath = mode_path(),
    State0 = #{mode_path => ModePath, mode => load_mode(ModePath),
               token => account_token(), connection => undefined,
               connection_monitor => undefined, stream => undefined,
               phase => idle, status => undefined, last_error => undefined,
               retry_timer => undefined, retry_ms => ?RETRY_MIN_MS,
               player_ref => undefined, player_monitor => undefined,
               game_active => false, holding => false,
               subscribers => #{}, subscriber_monitors => #{}},
    State1 = subscribe_player(State0),
    self() ! connect,
    {ok, update_hold(State1)}.

handle_call(snapshot, _From, State) ->
    {reply, public_snapshot(State), State};
handle_call({set_mode, Mode}, _From, State) ->
    case valid_mode(Mode) of
        true ->
            case persist_mode(maps:get(mode_path, State), Mode) of
                ok ->
                    State1 = apply_desired(State#{mode => Mode,
                                                  last_error => undefined}),
                    State2 = notify_changed(State, State1),
                    {reply, {ok, public_snapshot(State2)}, State2};
                {error, Reason} ->
                    {reply, {error, {market_presence_write_failed, Reason}}, State}
            end;
        false -> {reply, {error, invalid_market_presence_mode}, State}
    end;
handle_call({subscribe, Client}, _From, State) when is_pid(Client) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Subscribers = maps:get(subscribers, State),
    Monitors = maps:get(subscriber_monitors, State),
    {reply, {ok, Ref, public_snapshot(State)},
     State#{subscribers => Subscribers#{Ref => #{client => Client, monitor => Monitor}},
            subscriber_monitors => Monitors#{Monitor => Ref}}};
handle_call({unsubscribe, Ref}, _From, State) ->
    {reply, ok, remove_subscriber(Ref, State)};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast({token_changed, Token}, State) when is_binary(Token); Token =:= undefined ->
    Current = maps:get(token, State, undefined),
    State1 = State#{token => Token},
    case {Current, Token} of
        {undefined, New} when is_binary(New) ->
            self() ! connect,
            State2 = update_hold(State1#{phase => idle, last_error => undefined}),
            {noreply, notify_changed(State, State2)};
        {_Old, undefined} ->
            State2 = update_hold(disconnect(State1#{status => undefined,
                                                    phase => idle,
                                                    last_error => undefined}, false)),
            {noreply, notify_changed(State, State2)};
        _ -> {noreply, notify_changed(State, State1)}
    end;
handle_cast(_Message, State) -> {noreply, State}.

handle_info(connect, State = #{token := undefined}) ->
    {noreply, update_hold(State#{retry_timer => undefined, phase => idle})};
handle_info(connect, State = #{connection := Connection}) when is_pid(Connection) ->
    {noreply, State#{retry_timer => undefined}};
handle_info(connect, State) ->
    case socket_module():open() of
        {ok, Connection} ->
            Monitor = erlang:monitor(process, Connection),
            {noreply, update_hold(State#{connection => Connection,
                                         connection_monitor => Monitor,
                                         retry_timer => undefined,
                                         phase => connecting,
                                         last_error => undefined})};
        {error, Reason} ->
            {noreply, retry(State#{retry_timer => undefined},
                            {market_socket_open_failed, Reason})}
    end;
handle_info({gun_up, Connection, _Protocol},
            State = #{connection := Connection, phase := connecting}) ->
    Stream = socket_module():upgrade(Connection),
    {noreply, State#{stream => Stream, phase => upgrading}};
handle_info({gun_upgrade, Connection, Stream, [<<"websocket">>], _Headers},
            State = #{connection := Connection, stream := Stream}) ->
    ok = send_message(State, <<"@wfm|cmd/auth/signIn">>,
                      #{<<"token">> => maps:get(token, State)}),
    {noreply, State#{phase => authenticating}};
handle_info({gun_response, Connection, Stream, _Fin, Status, _Headers},
            State = #{connection := Connection, stream := Stream}) ->
    {noreply, retry(disconnect(State, true),
                    {market_socket_upgrade_failed, Status})};
handle_info({gun_error, Connection, Stream, Reason},
            State = #{connection := Connection, stream := Stream}) ->
    {noreply, retry(disconnect(State, true), Reason)};
handle_info({gun_error, Connection, Reason}, State = #{connection := Connection}) ->
    {noreply, retry(disconnect(State, true), Reason)};
handle_info({gun_ws, Connection, Stream, {text, Data}},
            State = #{connection := Connection, stream := Stream}) ->
    {noreply, notify_changed(State, handle_socket_message(Data, State))};
handle_info({gun_ws, Connection, Stream, {binary, Data}},
            State = #{connection := Connection, stream := Stream}) ->
    {noreply, notify_changed(State, handle_socket_message(Data, State))};
handle_info({gun_ws, Connection, Stream, close},
            State = #{connection := Connection, stream := Stream}) ->
    {noreply, retry(disconnect(State, true), market_socket_closed)};
handle_info({gun_ws, Connection, Stream, {close, Code, Reason}},
            State = #{connection := Connection, stream := Stream}) ->
    {noreply, retry(disconnect(State, true),
                    {market_socket_closed, Code, Reason})};
handle_info({gun_down, Connection, _Protocol, Reason, _Killed},
            State = #{connection := Connection}) ->
    {noreply, retry(disconnect(State, false), Reason)};
handle_info({gun_down, Connection, _Protocol, Reason, _Killed, _Unprocessed},
            State = #{connection := Connection}) ->
    {noreply, retry(disconnect(State, false), Reason)};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{connection_monitor := Monitor}) ->
    {noreply, retry(disconnect(State, false), Reason)};
handle_info({'DOWN', Monitor, process, _Pid, _Reason},
            State = #{player_monitor := Monitor}) ->
    erlang:send_after(1000, self(), subscribe_player),
    {noreply, State#{player_ref => undefined, player_monitor => undefined,
                     game_active => false}};
handle_info({'DOWN', Monitor, process, _Pid, _Reason}, State) ->
    case maps:take(Monitor, maps:get(subscriber_monitors, State, #{})) of
        error -> {noreply, State};
        {Ref, Monitors} ->
            {noreply, State#{subscribers => maps:remove(
                                              Ref, maps:get(subscribers, State)),
                             subscriber_monitors => Monitors}}
    end;
handle_info({wfcli_player, Ref, _Source, Snapshot}, State = #{player_ref := Ref}) ->
    Active = game_active(Snapshot),
    {noreply, notify_changed(State,
                             apply_desired(State#{game_active => Active}))};
%% Accept notifications queued before a hot update of wfcli_player_service.
handle_info({wfcli_player, Ref, Snapshot}, State = #{player_ref := Ref}) ->
    Active = game_active(Snapshot),
    {noreply, notify_changed(State,
                             apply_desired(State#{game_active => Active}))};
handle_info(subscribe_player, State) ->
    {noreply, apply_desired(subscribe_player(State))};
handle_info(_Message, State) -> {noreply, State}.

terminate(_Reason, State) ->
    unsubscribe_player(State),
    _ = disconnect(State, true),
    release_hold(maps:get(holding, State, false)),
    ok.

code_change(_OldVsn, State, _Extra) ->
    Defaults = #{mode_path => mode_path(), mode => <<"invisible">>,
                 token => account_token(), connection => undefined,
                 connection_monitor => undefined, stream => undefined,
                 phase => idle, status => undefined, last_error => undefined,
                 retry_timer => undefined, retry_ms => ?RETRY_MIN_MS,
                 player_ref => undefined, player_monitor => undefined,
                 game_active => false, holding => false,
                 subscribers => #{}, subscriber_monitors => #{}},
    {ok, update_hold(maps:merge(Defaults, State))}.

handle_socket_message(Data, State) ->
    try jsone:decode(Data, [{object_format, map}]) of
        Message when is_map(Message) -> handle_route(Message, State);
        _Other -> State#{last_error => invalid_market_socket_message}
    catch error:Reason -> State#{last_error => {invalid_market_socket_json, Reason}}
    end.

handle_route(#{<<"route">> := <<"@wfm|cmd/auth/signIn:ok">>}, State) ->
    State#{phase => awaiting_status, retry_ms => ?RETRY_MIN_MS,
           last_error => undefined};
handle_route(#{<<"route">> := <<"@wfm|cmd/auth/signIn:error">>} = Message, State) ->
    update_hold(disconnect(State#{phase => auth_failed,
                                  last_error => maps:get(<<"payload">>, Message,
                                                         market_socket_auth_failed)},
                           true));
handle_route(#{<<"route">> := <<"@wfm|event/status/set">>,
               <<"payload">> := Payload}, State) when is_map(Payload) ->
    reconcile_status(Payload, State);
handle_route(#{<<"route">> := <<"@wfm|cmd/status/set:ok">>,
               <<"payload">> := Payload}, State) when is_map(Payload) ->
    reconcile_status(Payload, State);
handle_route(#{<<"route">> := <<"@wfm|cmd/status/set:error">>} = Message, State) ->
    State#{phase => ready,
           last_error => maps:get(<<"payload">>, Message,
                                  market_status_update_failed)};
handle_route(#{<<"route">> := <<"@wfm|protect/error">>} = Message, State) ->
    State#{last_error => maps:get(<<"payload">>, Message,
                                  market_socket_protected_route_failed)};
handle_route(_Message, State) -> State.

reconcile_status(Payload, State) ->
    Status = maps:get(<<"status">>, Payload, maps:get(status, State, undefined)),
    apply_desired(State#{status => Status, phase => ready,
                         retry_ms => ?RETRY_MIN_MS, last_error => undefined}).

apply_desired(State0) ->
    State = update_hold(State0),
    Desired = desired_status(State),
    case {maps:get(phase, State), maps:get(status, State, undefined)} of
        {ready, Desired} -> State;
        {ready, _Other} ->
            ok = send_message(State, <<"@wfm|cmd/status/set">>,
                              #{<<"status">> => Desired}),
            State#{phase => setting_status};
        {setting_status, _Other} -> State;
        _ -> State
    end.

send_message(State, Route, Payload) ->
    Message = #{<<"route">> => Route, <<"id">> => message_id(),
                <<"payload">> => Payload},
    socket_module():send(maps:get(connection, State), maps:get(stream, State),
                         Message).

retry(State = #{token := undefined}, _Reason) ->
    update_hold(State#{phase => idle, last_error => undefined});
retry(State = #{phase := auth_failed}, _Reason) -> update_hold(State);
retry(State, Reason) ->
    cancel_timer(maps:get(retry_timer, State, undefined)),
    Delay = maps:get(retry_ms, State, ?RETRY_MIN_MS),
    Timer = erlang:send_after(Delay, self(), connect),
    update_hold(State#{retry_timer => Timer, phase => reconnecting,
                       retry_ms => min(?RETRY_MAX_MS, Delay * 2),
                       last_error => Reason}).

disconnect(State, Close) ->
    cancel_timer(maps:get(retry_timer, State, undefined)),
    maybe_demonitor(maps:get(connection_monitor, State, undefined)),
    case {Close, maps:get(connection, State, undefined)} of
        {true, Connection} when is_pid(Connection) ->
            safe_close(Connection);
        _ -> ok
    end,
    State#{connection => undefined, connection_monitor => undefined,
           stream => undefined, retry_timer => undefined}.

subscribe_player(State = #{player_ref := Ref}) when is_reference(Ref) -> State;
subscribe_player(State) ->
    case whereis(wfcli_player_service) of
        undefined -> State;
        Player ->
            case wfcli_player_service:subscribe(self()) of
                {ok, Ref, Snapshot} ->
                    Monitor = erlang:monitor(process, Player),
                    State#{player_ref => Ref, player_monitor => Monitor,
                           game_active => game_active(Snapshot)};
                {error, _Reason} -> State
            end
    end.

unsubscribe_player(State) ->
    case maps:get(player_ref, State, undefined) of
        Ref when is_reference(Ref) -> safe_unsubscribe(Ref);
        _ -> ok
    end,
    maybe_demonitor(maps:get(player_monitor, State, undefined)).

game_active(#{data := Data}) -> game_active(Data);
game_active(#{<<"data">> := Data}) -> game_active(Data);
game_active(Data) when is_map(Data) ->
    Game = maps:get(<<"game">>, Data, #{}),
    is_map(Game) andalso maps:get(<<"running">>, Game, false) =:= true;
game_active(_Snapshot) -> false.

desired_status(#{mode := <<"auto">>, game_active := true}) -> <<"ingame">>;
desired_status(#{mode := <<"auto">>}) -> <<"invisible">>;
desired_status(State) -> maps:get(mode, State, <<"invisible">>).

update_hold(State) ->
    Desired = desired_status(State),
    ShouldHold = maps:get(token, State, undefined) =/= undefined andalso
                 Desired =/= <<"invisible">> andalso
                 maps:get(phase, State, idle) =/= auth_failed,
    Holding = maps:get(holding, State, false),
    case {Holding, ShouldHold} of
        {false, true} -> activity_start();
        {true, false} -> activity_end();
        _ -> ok
    end,
    State#{holding => ShouldHold}.

release_hold(true) -> activity_end();
release_hold(false) -> ok.

public_snapshot(State) ->
    Phase = maps:get(phase, State, idle),
    #{<<"authenticated">> => maps:get(token, State, undefined) =/= undefined,
      <<"connected">> => lists:member(Phase, [ready, setting_status]),
      <<"connecting">> => lists:member(Phase,
                                        [connecting, upgrading, authenticating,
                                         awaiting_status, reconnecting]),
      <<"mode">> => maps:get(mode, State),
      <<"status">> => nullable(maps:get(status, State, undefined)),
      <<"desired_status">> => desired_status(State),
      <<"error">> => nullable(format_error(maps:get(last_error, State, undefined)))}.

notify_changed(OldState, NewState) ->
    Old = public_snapshot(OldState),
    New = public_snapshot(NewState),
    case Old =:= New of
        true -> NewState;
        false ->
            maps:foreach(
              fun(Ref, #{client := Client}) ->
                  Client ! {wfcli_market_presence, Ref, New}
              end,
              maps:get(subscribers, NewState, #{})),
            NewState
    end.

remove_subscriber(Ref, State) ->
    case maps:take(Ref, maps:get(subscribers, State)) of
        error -> State;
        {#{monitor := Monitor}, Subscribers} ->
            erlang:demonitor(Monitor, [flush]),
            State#{subscribers => Subscribers,
                   subscriber_monitors => maps:remove(
                                            Monitor,
                                            maps:get(subscriber_monitors, State))}
    end.

account_token() ->
    case whereis(wfcli_market_account_service) of
        undefined -> undefined;
        _Pid -> wfcli_market_account_service:token()
    end.

socket_module() ->
    case application:get_env(retinue_daemon, market_socket_module) of
        {ok, Module} when is_atom(Module) -> Module;
        _ -> wfcli_market_socket
    end.

mode_path() ->
    case application:get_env(retinue_daemon, market_presence_file) of
        {ok, Path} -> Path;
        undefined -> wfcli_paths:state_file("market-presence.json")
    end.

load_mode(Path) ->
    case file:read_file(Path) of
        {ok, Data} ->
            try jsone:decode(Data, [{object_format, map}]) of
                #{<<"mode">> := Mode} ->
                    case valid_mode(Mode) of true -> Mode; false -> <<"invisible">> end;
                _ -> <<"invisible">>
            catch _:_ -> <<"invisible">>
            end;
        {error, _Reason} -> <<"invisible">>
    end.

persist_mode(Path, Mode) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            case file:write_file(Temp, jsone:encode(#{<<"mode">> => Mode}), [binary]) of
                ok ->
                    _ = file:change_mode(Temp, 8#600),
                    case file:rename(Temp, Path) of
                        ok -> file:change_mode(Path, 8#600);
                        Error -> Error
                    end;
                Error -> Error
            end;
        Error -> Error
    end.

valid_mode(<<"auto">>) -> true;
valid_mode(<<"online">>) -> true;
valid_mode(<<"ingame">>) -> true;
valid_mode(<<"invisible">>) -> true;
valid_mode(_Mode) -> false.

message_id() -> integer_to_binary(erlang:unique_integer([positive, monotonic])).

nullable(undefined) -> null;
nullable(Value) -> Value.

format_error(undefined) -> undefined;
format_error(Value) when is_binary(Value) -> Value;
format_error(Value) -> iolist_to_binary(io_lib:format("~p", [Value])).

maybe_demonitor(undefined) -> ok;
maybe_demonitor(Monitor) -> erlang:demonitor(Monitor, [flush]), ok.

safe_close(Connection) ->
    try socket_module():close(Connection)
    catch _:_ -> ok
    end.

safe_unsubscribe(Ref) ->
    try wfcli_player_service:unsubscribe(Ref)
    catch _:_ -> ok
    end.

cancel_timer(undefined) -> ok;
cancel_timer(Timer) -> _ = erlang:cancel_timer(Timer), ok.

activity_start() ->
    case whereis(retinue_activity) of
        undefined -> ok;
        _Pid -> retinue_activity:activity_start()
    end.

activity_end() ->
    case whereis(retinue_activity) of
        undefined -> ok;
        _Pid -> retinue_activity:activity_end()
    end.

%%%-------------------------------------------------------------------
%% Serialized owner for Warframe.market credentials and account orders.
%%%-------------------------------------------------------------------
-module(wfcli_market_account_service).

-behaviour(gen_server).

-export([start_link/0, submit/2, status/0, token/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).

-doc "Start the authenticated Market account owner.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() -> gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Queue account work; result arrives as `{wfcli_market_account, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) -> gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Return account state without exposing its token.".
-spec status() -> map().
status() -> gen_server:call(?SERVER, status).

-doc "Return the session token to daemon-internal Market services.".
-spec token() -> binary() | undefined.
token() -> gen_server:call(?SERVER, token).

init([]) ->
    Path = wfcli_market_account_store:path(),
    {ok, #{token_file => Path, token => wfcli_market_account_store:load(Path),
           profile => null, orders => [], updated_at => undefined,
           queue => queue:new(), current => undefined, monitors => #{},
           store_error => undefined}}.

handle_call({submit, Client, Request}, _From, State)
  when is_pid(Client), is_map(Request) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Job = #{ref => Ref, client => Client, monitor => Monitor, request => Request},
    activity_start(),
    self() ! process_queue,
    {reply, {ok, Ref}, State#{queue => queue:in(Job, maps:get(queue, State)),
                              monitors => (maps:get(monitors, State))#{Monitor => Ref}}};
handle_call(status, _From, State) ->
    {reply, public_status(State), State};
handle_call(token, _From, State) ->
    {reply, maps:get(token, State, undefined), State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast(_Message, State) -> {noreply, State}.

handle_info(process_queue, State = #{current := Current}) when Current =/= undefined ->
    {noreply, State};
handle_info(process_queue, State) ->
    case queue:out(maps:get(queue, State)) of
        {empty, Queue} -> {noreply, State#{queue => Queue}};
        {{value, Job}, Queue} ->
            Parent = self(),
            Token = make_ref(),
            Snapshot = worker_snapshot(State),
            {Pid, WorkerMonitor} = spawn_monitor(fun() ->
                Result = safe_execute(maps:get(request, Job), Snapshot),
                Parent ! {market_account_result, Token, Result}
            end),
            Current = #{token => Token, worker_pid => Pid,
                        worker_monitor => WorkerMonitor, job => Job},
            {noreply, State#{queue => Queue, current => Current}}
    end;
handle_info({market_account_result, Token, {Reply, Delta}},
            State = #{current := #{token := Token, worker_monitor := WorkerMonitor,
                                   job := Job}}) ->
    erlang:demonitor(WorkerMonitor, [flush]),
    State1 = apply_delta(Delta, State#{current => undefined}),
    State2 = complete_job(Job, Reply, State1),
    self() ! process_queue,
    {noreply, State2};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{current := #{worker_monitor := Monitor, job := Job}}) ->
    State1 = complete_job(Job, {error, {market_account_worker_down, Reason}},
                          State#{current => undefined}),
    self() ! process_queue,
    {noreply, State1};
handle_info({'DOWN', Monitor, process, _Pid, _Reason}, State) ->
    case maps:take(Monitor, maps:get(monitors, State)) of
        error -> {noreply, State};
        {_Ref, Monitors} ->
            State1 = cancel_client(Monitor, State#{monitors => Monitors}),
            self() ! process_queue,
            {noreply, State1}
    end;
handle_info(_Message, State) -> {noreply, State}.

terminate(_Reason, State) ->
    stop_worker(maps:get(current, State, undefined)),
    lists:foreach(fun(_Monitor) -> activity_end() end,
                  maps:keys(maps:get(monitors, State, #{}))),
    ok.

code_change(_OldVsn, State, _Extra) ->
    Path = maps:get(token_file, State, wfcli_market_account_store:path()),
    {ok, State#{token_file => Path,
                token => maps:get(token, State,
                                  wfcli_market_account_store:load(Path)),
                profile => maps:get(profile, State, null),
                orders => maps:get(orders, State, []),
                updated_at => maps:get(updated_at, State, undefined),
                queue => maps:get(queue, State, queue:new()),
                current => maps:get(current, State, undefined),
                monitors => maps:get(monitors, State, #{}),
                store_error => maps:get(store_error, State, undefined)}}.

safe_execute(Request, Snapshot) ->
    try execute(Request, Snapshot)
    catch Class:Reason:Stack ->
        logger:error("market account request failed: ~p:~p~n~p", [Class, Reason, Stack]),
        {{error, {market_account_request_failed, Class, Reason}}, #{}}
    end.

execute(#{action := snapshot}, #{token := undefined}) ->
    {{ok, empty_snapshot()}, #{}};
execute(#{action := snapshot}, Snapshot) -> refresh(Snapshot);
execute(#{action := login, email := Email, password := Password}, _Snapshot)
  when is_binary(Email), is_binary(Password), byte_size(Email) > 0,
       byte_size(Password) > 0 ->
    case wfcli_market_account_api:login(Email, Password) of
        {ok, Token} -> refresh(#{token => Token, profile => null, orders => []});
        {error, Reason} -> {{error, readable_error(Reason)}, #{}}
    end;
execute(#{action := logout}, _Snapshot) ->
    {{ok, empty_snapshot()}, #{token => undefined, profile => null, orders => [],
                              updated_at => erlang:system_time(millisecond)}};
execute(#{action := create_order, order := Order}, Snapshot) when is_map(Order) ->
    mutate(fun(Token) -> wfcli_market_account_api:create_order(Token, Order) end,
           Snapshot);
execute(#{action := update_order, id := Id, patch := Patch}, Snapshot)
  when is_binary(Id), is_map(Patch) ->
    mutate(fun(Token) -> wfcli_market_account_api:update_order(Token, Id, Patch) end,
           Snapshot);
execute(#{action := delete_order, id := Id}, Snapshot) when is_binary(Id) ->
    mutate(fun(Token) -> wfcli_market_account_api:delete_order(Token, Id) end,
           Snapshot);
execute(#{action := close_order, id := Id, quantity := Quantity}, Snapshot)
  when is_binary(Id), is_integer(Quantity), Quantity > 0 ->
    mutate(fun(Token) -> wfcli_market_account_api:close_order(Token, Id, Quantity) end,
           Snapshot);
execute(#{action := set_visibility, visible := Visible} = Request, Snapshot)
  when is_boolean(Visible) ->
    Type = maps:get(type, Request, undefined),
    mutate(fun(Token) ->
                   wfcli_market_account_api:set_group_visibility(Token, Visible, Type)
           end, Snapshot);
execute(Request, _Snapshot) ->
    {{error, {unsupported_market_account_action,
              maps:get(action, Request, undefined)}}, #{}}.

refresh(#{token := Token}) ->
    case wfcli_market_account_api:profile(Token) of
        {ok, Profile, Rotated1} when is_map(Profile) ->
            Token1 = choose_token(Rotated1, Token),
            case maps:get(<<"verification">>, Profile, false) of
                false -> {{error, <<"Verify the Warframe.market account first">>}, #{}};
                true -> refresh_orders(Token1, Profile)
            end;
        {error, market_account_unauthorized} -> unauthorized();
        {error, Reason} -> {{error, readable_error(Reason)}, #{}}
    end.

refresh_orders(Token, Profile) ->
    case wfcli_market_account_api:orders(Token) of
        {ok, Orders, Rotated} ->
            Token1 = choose_token(Rotated, Token),
            Updated = erlang:system_time(millisecond),
            Public = #{<<"authenticated">> => true, <<"profile">> => Profile,
                       <<"orders">> => Orders, <<"updated_at">> => Updated},
            {{ok, Public}, #{token => Token1, profile => Profile, orders => Orders,
                             updated_at => Updated}};
        {error, market_account_unauthorized} -> unauthorized();
        {error, Reason} -> {{error, readable_error(Reason)}, #{}}
    end.

mutate(_Fun, #{token := undefined}) ->
    {{error, <<"Sign in to Warframe.market first">>}, #{}};
mutate(Fun, Snapshot = #{token := Token}) ->
    case Fun(Token) of
        {ok, _Data, Rotated} ->
            refresh_orders(choose_token(Rotated, Token), maps:get(profile, Snapshot, null));
        {error, market_account_unauthorized} -> unauthorized();
        {error, Reason} -> {{error, readable_error(Reason)}, #{}}
    end.

unauthorized() ->
    {{error, <<"Warframe.market session expired">>},
     #{token => undefined, profile => null, orders => [],
       updated_at => erlang:system_time(millisecond)}}.

worker_snapshot(State) ->
    maps:with([token, profile, orders, updated_at], State).

apply_delta(Delta, State) when map_size(Delta) =:= 0 -> State;
apply_delta(Delta, State) ->
    State1 = maps:merge(State, Delta),
    Path = maps:get(token_file, State),
    TokenChange = maps:get(token, Delta, '$unchanged'),
    Result = case TokenChange of
        '$unchanged' -> ok;
        undefined -> wfcli_market_account_store:clear(Path);
        StoredToken -> wfcli_market_account_store:persist(Path, StoredToken)
    end,
    case TokenChange of
        '$unchanged' -> ok;
        PresenceToken -> notify_presence(PresenceToken)
    end,
    State1#{store_error => case Result of ok -> undefined; Error -> Error end}.

complete_job(Job, Reply, State) ->
    Monitor = maps:get(monitor, Job),
    erlang:demonitor(Monitor, [flush]),
    maps:get(client, Job) ! {wfcli_market_account, maps:get(ref, Job), Reply},
    activity_end(),
    State#{monitors => maps:remove(Monitor, maps:get(monitors, State))}.

cancel_client(Monitor, State) ->
    Queue = queue:filter(fun(Job) -> maps:get(monitor, Job) =/= Monitor end,
                         maps:get(queue, State)),
    case maps:get(current, State, undefined) of
        #{job := #{monitor := Monitor}} = Current ->
            stop_worker(Current),
            activity_end(),
            State#{queue => Queue, current => undefined};
        _ ->
            activity_end(),
            State#{queue => Queue}
    end.

stop_worker(undefined) -> ok;
stop_worker(#{worker_pid := Pid, worker_monitor := Monitor}) ->
    exit(Pid, shutdown),
    erlang:demonitor(Monitor, [flush]),
    ok.

public_status(State) ->
    #{authenticated => maps:get(token, State, undefined) =/= undefined,
      profile_loaded => is_map(maps:get(profile, State, null)),
      orders => length(maps:get(orders, State, [])),
      queued => queue:len(maps:get(queue, State)),
      processing => maps:get(current, State) =/= undefined,
      updated_at => maps:get(updated_at, State, undefined),
      token_file => maps:get(token_file, State),
      store_error => maps:get(store_error, State, undefined)}.

empty_snapshot() ->
    #{<<"authenticated">> => false, <<"profile">> => null,
      <<"orders">> => [], <<"updated_at">> => null}.

choose_token(undefined, Current) -> Current;
choose_token(Token, _Current) -> Token.

readable_error(invalid_market_credentials) -> <<"Invalid email or password">>;
readable_error(market_account_unauthorized) -> <<"Warframe.market session expired">>;
readable_error({market_account_http_status, Status, _Body}) ->
    iolist_to_binary(io_lib:format("Warframe.market returned HTTP ~p", [Status]));
readable_error({market_account_http_failed, _Reason}) ->
    <<"Could not reach Warframe.market">>;
readable_error(Reason) -> iolist_to_binary(io_lib:format("~p", [Reason])).

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

notify_presence(Token) ->
    case whereis(wfcli_market_presence_service) of
        undefined -> ok;
        _Pid -> wfcli_market_presence_service:token_changed(Token)
    end.

%%%-------------------------------------------------------------------
%% User-local Unix socket API for native companions.
%%%-------------------------------------------------------------------
-module(retinue_local_api).

-behaviour(gen_server).

-export([start_link/0, status/0, socket_path/0, companion_command/1,
         diagnostics_submit/2, diagnostics_consume/2, diagnostics_cancel/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(DEFAULT_GLOBAL_REQUEST_WORKERS, 16).

-type state() :: #{
    listen := socket:socket(),
    path := file:filename_all(),
    acceptor := pid(),
    connections := map(),
    monitors := map(),
    worker_holders := map(),
    worker_waiters := queue:queue(),
    worker_count := non_neg_integer(),
    worker_limit := pos_integer()
}.

-doc "Start owner-only Unix socket endpoint used by native companion processes.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Return local API socket and connected-client state.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-doc "Return configured native companion Unix socket path.".
-spec socket_path() -> file:filename_all().
socket_path() ->
    case application:get_env(retinue_daemon, local_socket) of
        {ok, Path} -> Path;
        undefined ->
            case os:getenv("RETINUE_DAEMON_SOCKET") of
                false -> retinue_paths:file(runtime, "daemon.sock");
                "" -> retinue_paths:file(runtime, "daemon.sock");
                Path -> Path
            end
    end.

-doc "Send an optional diagnostic command to every connected companion.".
-spec companion_command(map()) -> {ok, non_neg_integer()}.
companion_command(Command) when is_map(Command) ->
    gen_server:call(?SERVER, {companion_command, Command}).

diagnostics_submit(Client, Request) ->
    gen_server:call(?SERVER, {diagnostics_submit, Client, Request}).

diagnostics_consume(Client, Ref) ->
    gen_server:cast(?SERVER, {diagnostics_consume, Client, Ref}).

diagnostics_cancel(Ref) -> gen_server:call(?SERVER, {diagnostics_cancel, Ref}).

-spec init([]) -> {ok, state()} | {stop, term()}.
init([]) ->
    process_flag(trap_exit, true),
    Path = socket_path(),
    case open_listener(Path) of
        {ok, Listen} ->
            Parent = self(),
            Acceptor = spawn_link(fun() -> accept_loop(Listen, Parent) end),
            {ok, #{listen => Listen, path => Path, acceptor => Acceptor,
                   contract => required_local_contract(),
                   connections => #{}, monitors => #{}, diagnostics => #{}, worker_holders => #{},
                   worker_waiters => queue:new(), worker_count => 0,
                   worker_limit => global_local_worker_limit()}};
        {error, Reason} ->
            {stop, {local_api_listen_failed, Path, Reason}}
    end.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call(status, _From, State) ->
    Connections = maps:get(connections, State),
    CompanionDetails =
        [maps:without([monitor], Info)
         || {_Pid, Info} <- maps:to_list(Connections),
            maps:get(client, Info, undefined) =:= <<"wfcompanion">>,
            maps:get(game, Info, undefined) =:= <<"warframe">>],
    {reply, #{socket => maps:get(path, State),
              connections => map_size(Connections),
              companions => length(CompanionDetails),
              diagnostic_requests => map_size(maps:get(diagnostics, State)),
              companion_details => CompanionDetails,
              clients => [maps:without([monitor], Info) || Info <- maps:values(Connections),
                                                           maps:is_key(game, Info)],
              local_workers => maps:get(worker_count, State, 0),
              local_workers_queued => queue:len(
                                        maps:get(worker_waiters, State, queue:new())),
              local_worker_limit => maps:get(worker_limit, State,
                                             global_local_worker_limit()),
              contract => retinue_local_protocol:contract()}, State};
handle_call({acquire_local_workers, Pid, Count}, _From, State)
  when is_pid(Pid), is_integer(Count), Count > 0 ->
    {Granted, Queued, State1} = acquire_local_workers(Pid, Count, State),
    {reply, {Granted, Queued}, State1};
handle_call({companion_command, Command}, _From, State) ->
    Count = maps:fold(
      fun(Pid, Info, Acc) ->
          case {maps:get(client, Info, undefined),
                lists:member(<<"companion.command">>,
                             maps:get(features, Info, []))} of
              {<<"wfcompanion">>, true} ->
                  Pid ! {daemon_command, Command},
                  Acc + 1;
              _ -> Acc
          end
      end,
      0,
      maps:get(connections, State)),
    {reply, {ok, Count}, State};
handle_call({diagnostics_submit, Client, Request}, _From, State) ->
    {Reply, Pending} = wf_companion_diagnostics:submit(
                         Client, Request, maps:get(connections, State),
                         maps:get(diagnostics, State)),
    {reply, Reply, State#{diagnostics => Pending}};
handle_call({diagnostics_cancel, Ref}, _From, State) ->
    Pending = wf_companion_diagnostics:cancel(Ref, maps:get(diagnostics, State)),
    {reply, ok, State#{diagnostics => Pending}};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast({release_local_worker, Pid}, State) when is_pid(Pid) ->
    {noreply, assign_local_worker_waiters(release_local_workers(Pid, 1, State))};
handle_cast({diagnostics_consume, Client, Ref}, State) ->
    Pending = wf_companion_diagnostics:consume(Client, Ref, maps:get(diagnostics, State)),
    {noreply, State#{diagnostics => Pending}};
handle_cast(_Message, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info({connection_started, Pid}, State) ->
    Monitor = erlang:monitor(process, Pid),
    Connections = maps:get(connections, State),
    Monitors = maps:get(monitors, State),
    {noreply, State#{connections => Connections#{Pid => #{monitor => Monitor}},
                     monitors => Monitors#{Monitor => Pid}}};
handle_info({client_identified, Pid, ClientInfo}, State) ->
    Connections = maps:get(connections, State),
    case maps:get(Pid, Connections, undefined) of
        undefined -> {noreply, State};
        Info ->
            update_client_activity(Pid, Info, ClientInfo),
            {noreply, State#{connections => Connections#{Pid => maps:merge(Info, ClientInfo)}}}
    end;
handle_info({diagnostic_reply, Companion, Id, Data}, State) ->
    Pending = wf_companion_diagnostics:reply(Companion, Id, Data, maps:get(diagnostics, State)),
    {noreply, State#{diagnostics => Pending}};
handle_info({timeout, Timer, {diagnostic_timeout, Id}}, State) ->
    Pending = wf_companion_diagnostics:timeout(Id, Timer, maps:get(diagnostics, State)),
    {noreply, State#{diagnostics => Pending}};
handle_info({'DOWN', Monitor, process, Pid, _Reason}, State0) ->
    Pending = wf_companion_diagnostics:down(Monitor, Pid, maps:get(diagnostics, State0)),
    State = State0#{diagnostics => Pending},
    case maps:take(Monitor, maps:get(monitors, State)) of
        error -> {noreply, State};
        {Connection, Monitors} ->
            Connections0 = maps:get(connections, State),
            case maps:take(Connection, Connections0) of
                error ->
                    {noreply, State#{monitors => Monitors}};
                {Info, Connections} ->
                    release_client_activity(Connection, Info),
                    State1 = drop_local_worker_client(Connection, State),
                    {noreply, assign_local_worker_waiters(
                                State1#{connections => Connections,
                                        monitors => Monitors})}
            end
    end;
handle_info({'EXIT', Acceptor, Reason}, State = #{acceptor := Acceptor}) ->
    case Reason of
        normal -> {noreply, State};
        _ ->
            Listen = maps:get(listen, State),
            Parent = self(),
            NewAcceptor = spawn_link(fun() -> accept_loop(Listen, Parent) end),
            {noreply, State#{acceptor => NewAcceptor}}
    end;
handle_info(_Message, State) ->
    {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, State) ->
    wf_companion_diagnostics:close(maps:get(diagnostics, State, #{})),
    maps:foreach(fun(Pid, _Info) -> Pid ! shutdown end,
                 maps:get(connections, State, #{})),
    _ = socket:close(maps:get(listen, State)),
    _ = file:delete(maps:get(path, State)),
    ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    Contract = required_local_contract(),
    case maps:get(contract, State, undefined) of
        Contract -> ok;
        _Changed ->
            maps:foreach(fun(Pid, _Info) -> Pid ! shutdown end,
                         maps:get(connections, State, #{}))
    end,
    {ok, State#{contract => Contract,
                diagnostics => maps:get(diagnostics, State, #{}),
                worker_holders => maps:get(worker_holders, State, #{}),
                worker_waiters => maps:get(worker_waiters, State, queue:new()),
                worker_count => maps:get(worker_count, State, 0),
                worker_limit => maps:get(worker_limit, State,
                                         global_local_worker_limit())}}.

acquire_local_workers(Pid, Count, State) ->
    Available = max(0, maps:get(worker_limit, State) - maps:get(worker_count, State)),
    Granted = min(Count, Available),
    Queued = Count - Granted,
    State1 = add_local_worker_holders(Pid, Granted, State),
    Waiters = enqueue_local_worker_waiters(Pid, Queued,
                                            maps:get(worker_waiters, State1)),
    {Granted, Queued, State1#{worker_waiters => Waiters}}.

enqueue_local_worker_waiters(_Pid, 0, Queue) -> Queue;
enqueue_local_worker_waiters(Pid, Count, Queue) ->
    enqueue_local_worker_waiters(Pid, Count - 1, queue:in(Pid, Queue)).

add_local_worker_holders(_Pid, 0, State) -> State;
add_local_worker_holders(Pid, Count, State) ->
    Holders = maps:get(worker_holders, State),
    Held = maps:get(Pid, Holders, 0),
    State#{worker_holders => Holders#{Pid => Held + Count},
           worker_count => maps:get(worker_count, State) + Count}.

release_local_workers(Pid, Count, State) ->
    Holders = maps:get(worker_holders, State),
    Held = maps:get(Pid, Holders, 0),
    Released = min(Count, Held),
    Remaining = Held - Released,
    Holders1 = case Remaining of
        0 -> maps:remove(Pid, Holders);
        _ -> Holders#{Pid => Remaining}
    end,
    State#{worker_holders => Holders1,
           worker_count => maps:get(worker_count, State) - Released}.

drop_local_worker_client(Pid, State) ->
    Held = maps:get(Pid, maps:get(worker_holders, State), 0),
    Waiters = queue:filter(fun(WaitingPid) -> WaitingPid =/= Pid end,
                           maps:get(worker_waiters, State)),
    (release_local_workers(Pid, Held, State))#{worker_waiters => Waiters}.

assign_local_worker_waiters(State) ->
    case maps:get(worker_count, State) < maps:get(worker_limit, State) of
        false -> State;
        true ->
            case queue:out(maps:get(worker_waiters, State)) of
                {empty, Queue} -> State#{worker_waiters => Queue};
                {{value, Pid}, Queue} ->
                    State1 = State#{worker_waiters => Queue},
                    case maps:is_key(Pid, maps:get(connections, State1)) of
                        true ->
                            Pid ! local_worker_permit,
                            assign_local_worker_waiters(
                              add_local_worker_holders(Pid, 1, State1));
                        false -> assign_local_worker_waiters(State1)
                    end
            end
    end.

global_local_worker_limit() ->
    case application:get_env(retinue_daemon, local_request_global_workers,
                             ?DEFAULT_GLOBAL_REQUEST_WORKERS) of
        Count when is_integer(Count), Count > 0 -> Count;
        _ -> ?DEFAULT_GLOBAL_REQUEST_WORKERS
    end.

open_listener(Path) ->
    Prepare = case filename:dirname(Path) =:= retinue_paths:root(runtime) of
        true -> retinue_paths:ensure_runtime();
        false -> retinue_paths:ensure_private(filename:dirname(Path))
    end,
    case Prepare of
        ok -> open_listener_path(Path);
        {error, _Reason} = Error -> Error
    end.

open_listener_path(Path) ->
    _ = file:make_dir(filename:dirname(Path)),
    _ = file:change_mode(filename:dirname(Path), 8#700),
    _ = file:delete(Path),
    case socket:open(local, stream, default) of
        {ok, Listen} ->
            case socket:bind(Listen, #{family => local, path => Path}) of
                ok ->
                    case socket:listen(Listen) of
                        ok ->
                            _ = file:change_mode(Path, 8#600),
                            {ok, Listen};
                        {error, Reason} ->
                            _ = socket:close(Listen),
                            {error, Reason}
                    end;
                {error, Reason} ->
                    _ = socket:close(Listen),
                    {error, Reason}
            end;
        {error, _Reason} = Error -> Error
    end.

accept_loop(Listen, Parent) ->
    case socket:accept(Listen) of
        {ok, Socket} ->
            Handler = spawn(fun() -> await_socket(Parent) end),
            case socket:setopt(Socket, otp, controlling_process, Handler) of
                ok ->
                    Handler ! {accepted_socket, Socket},
                    Parent ! {connection_started, Handler};
                {error, Reason} ->
                    exit(Handler, {socket_transfer_failed, Reason}),
                    _ = socket:close(Socket)
            end,
            accept_loop(Listen, Parent);
        {error, closed} -> ok;
        {error, Reason} -> exit({accept_failed, Reason})
    end.

await_socket(Parent) ->
    receive
        {accepted_socket, Socket} -> retinue_connection:run(Socket, Parent)
    after 5000 ->
        exit(socket_transfer_timeout)
    end.

update_client_activity(Pid, Previous, Client) ->
    set_client_activity(is_active_client(Previous), is_active_client(Client)),
    set_gui_activity(Pid, is_warframe_gui(Previous), is_warframe_gui(Client)).

set_client_activity(true, false) ->
    retinue_activity:activity_end();
set_client_activity(false, true) ->
    retinue_activity:activity_start();
set_client_activity(_Previous, _Client) -> ok.

release_client_activity(Pid, Info) ->
    release_activity(is_active_client(Info)),
    set_gui_activity(Pid, is_warframe_gui(Info), false).

release_activity(true) ->
    retinue_activity:activity_end();
release_activity(false) -> ok.

is_active_client(#{game := <<"warframe">>, client := <<"wfcompanion">>}) -> true;
is_active_client(#{game := <<"poe2">>, client := <<"poe2companion">>}) -> true;
is_active_client(#{game := <<"warframe">>, client := <<"wfgui">>}) -> true;
is_active_client(_Client) -> false.

is_warframe_gui(#{game := <<"warframe">>, client := <<"wfgui">>}) -> true;
is_warframe_gui(_) -> false.

set_gui_activity(_Pid, Previous, Current) when Previous =:= Current -> ok;
set_gui_activity(Pid, false, true) ->
    wf_notification_service:gui_connected(Pid);
set_gui_activity(Pid, true, false) ->
    wf_notification_service:gui_disconnected(Pid).

required_local_contract() ->
    #{<<"envelope">> => retinue_local_protocol:envelope_version(),
      <<"games">> => maps:from_list([{Game, retinue_local_protocol:interfaces(Game)}
                                     || Game <- [<<"warframe">>, <<"poe2">>]])}.

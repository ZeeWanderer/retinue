-module(retinue_connection).
-export([run/2, send_json/2, send_error/3, send_ok/4, request_id/1, start_local_request/4, feature_enabled/2, daemon_identity/0]).
-define(MAX_BUFFER_BYTES, 8388608).
-define(DEFAULT_REQUEST_WORKERS, 4).
-define(SERVER, retinue_local_api).
run(Socket, Parent) ->
    process_flag(trap_exit, true),
    Handler = self(),
    {Reader, ReaderMonitor} = spawn_monitor(fun() -> socket_reader(Socket, Handler) end),
    Owners = [Parent],
    OwnerMonitors = maps:from_list([{erlang:monitor(process, Pid), Pid}
                                   || Pid <- Owners]),
    State = #{socket => Socket, parent => Parent, reader => Reader,
              owner_monitors => OwnerMonitors,
              reader_monitor => ReaderMonitor, buffer => <<>>,
              hello => false, client => <<"unknown">>, features => [],
              workers => #{}, worker_queue => queue:new(),
              worker_permits => 0, worker_permit_requests => 0,
              max_workers => local_worker_limit()},
    try
        FinalState = connection_loop(State),
        cleanup_connection(FinalState)
    after
        _ = socket:close(Socket)
    end.

connection_loop(State = #{reader_monitor := ReaderMonitor,
                          owner_monitors := OwnerMonitors}) ->
    receive
        {socket_data, Data} ->
            case consume_data(Data, State) of
                {ok, State1} -> connection_loop(State1);
                {stop, _Reason, State1} -> State1
            end;
        {socket_closed, _Reason} -> State;
        {local_request_result, Token, Id, Dataset, Reply} ->
            State1 = send_local_request_result(Token, Id, Dataset, Reply, State),
            connection_loop(State1);
        local_worker_permit ->
            Waiting = max(0, maps:get(worker_permit_requests, State) - 1),
            State1 = schedule_local_workers(
                       State#{worker_permits => maps:get(worker_permits, State) + 1,
                              worker_permit_requests => Waiting}),
            connection_loop(State1);
        {daemon_command, Command} ->
            send_json(maps:get(socket, State),
                      #{<<"event">> => <<"command">>, <<"data">> => Command}),
            connection_loop(State);
        {'DOWN', Monitor, process, _Owner, _Reason}
          when is_map_key(Monitor, OwnerMonitors) -> State;
        {'DOWN', ReaderMonitor, process, _Reader, Reason} ->
            case Reason of
                normal -> State;
                _ -> exit({socket_reader_failed, Reason})
            end;
        {'DOWN', Monitor, process, _Worker, Reason} ->
            State1 = local_request_down(Monitor, Reason, State),
            connection_loop(State1);
        {companion_diagnostics, Id, Command} ->
            send_json(maps:get(socket, State),
                      #{<<"event">> => <<"companion_diagnostics">>,
                        <<"request_id">> => Id, <<"request">> => Command}),
            connection_loop(State);
        shutdown -> State;
        Message ->
            Adapter = maps:get(adapter, State, undefined),
            State1 = case Adapter of undefined -> State; _ -> Adapter:event(Message, State) end,
            connection_loop(State1)
    end.

socket_reader(Socket, Handler) ->
    case socket:recv(Socket, 0) of
        {ok, <<>>} -> Handler ! {socket_closed, closed};
        {ok, Data} -> Handler ! {socket_data, Data}, socket_reader(Socket, Handler);
        {error, Reason} -> Handler ! {socket_closed, Reason}
    end.

consume_data(Data, State) ->
    Buffer = <<(maps:get(buffer, State))/binary, Data/binary>>,
    case byte_size(Buffer) > ?MAX_BUFFER_BYTES of
        true -> {stop, message_too_large, State};
        false ->
            Parts = binary:split(Buffer, <<"\n">>, [global]),
            {Lines, Rest} = split_last(Parts),
            case consume_lines(Lines, State#{buffer => Rest}) of
                {ok, State1} -> {ok, State1};
                Stop -> Stop
            end
    end.

split_last(Parts) ->
    [Rest | Reversed] = lists:reverse(Parts),
    {lists:reverse(Reversed), Rest}.

consume_lines([], State) -> {ok, State};
consume_lines([<<>> | Rest], State) -> consume_lines(Rest, State);
consume_lines([Line | Rest], State) ->
    case retinue_local_protocol:decode(Line) of
        {ok, Request} ->
            case handle_request(Request, State) of
                {ok, State1} -> consume_lines(Rest, State1);
                Stop -> Stop
            end;
        {error, Reason} ->
            send_error(maps:get(socket, State), 0, Reason),
            consume_lines(Rest, State)
    end.

handle_request(#{<<"op">> := <<"hello">>} = Request, State = #{hello := true}) ->
    send_error(maps:get(socket, State), request_id(Request), already_identified),
    {stop, already_identified, State};
handle_request(#{<<"op">> := <<"hello">>} = Request, State) ->
    Id = request_id(Request),
    Negotiation = retinue_local_protocol:negotiate(Request),
    Compatible = maps:get(<<"compatible">>, Negotiation),
    Features = maps:get(<<"features">>, Negotiation, []),
    Client = valid_client_name(maps:get(<<"client">>, Request, undefined)),
    Game = maps:get(<<"game">>, Request, undefined),
    ClientInfo = #{client => Client, game => Game,
                   version => maps:get(<<"version">>, Request, undefined),
                   os_pid => valid_os_pid(maps:get(<<"pid">>, Request, undefined)),
                   mode => valid_client_mode(maps:get(<<"mode">>, Request, undefined)),
                   features => Features},
    Reply = maps:merge(
              (daemon_identity())#{<<"id">> => Id, <<"ok">> => Compatible},
              Negotiation),
    send_json(maps:get(socket, State), Reply),
    case Compatible of
        true -> maps:get(parent, State) ! {client_identified, self(), ClientInfo};
        false -> ok
    end,
    State1 = State#{hello => Compatible, client => Client, game => Game, features => Features,
                    interfaces => maps:get(<<"interfaces">>, Request, #{}),
                    client_info => ClientInfo},
    {ok, case Compatible of
             true -> identify_game(Game, State1);
             false -> State1
         end};
handle_request(Request, State = #{hello := false}) ->
    send_error(maps:get(socket, State), request_id(Request), hello_required),
    {ok, State};
handle_request(#{<<"game">> := Game} = Request, State = #{game := Selected})
  when Game =/= Selected ->
    send_error(maps:get(socket, State), request_id(Request), wrong_game),
    {ok, State};
handle_request(#{<<"op">> := <<"publish">>, <<"dataset">> := <<"checks">>} = Request, State) ->
    Reply = case maps:get(<<"checks">>, maps:get(interfaces, State), undefined) of
        1 -> publish_checks(maps:get(<<"data">>, Request, undefined), State);
        _ -> {error, checks_interface_required}
    end,
    case Reply of
        ok -> send_ok(maps:get(socket, State), request_id(Request), <<"checks">>, #{<<"accepted">> => true});
        {error, Reason} -> send_error(maps:get(socket, State), request_id(Request), Reason)
    end,
    {ok, State};
handle_request(Request, State) ->
    Adapter = maps:get(adapter, State),
    Adapter:request(Request, State).

publish_checks(#{<<"context">> := Context, <<"checks">> := Checks}, State) ->
    case retinue_checks:valid_report(Context, Checks) of
        true ->
            Client = maps:get(client, State),
            Role = case Client of
                <<"wfcompanion">> -> <<"companion">>;
                <<"poe2companion">> -> <<"companion">>;
                <<"arccompanion">> -> <<"companion">>;
                _ -> Client
            end,
            Component = <<(maps:get(game, State))/binary, ".", Role/binary>>,
            Version = maps:get(version, maps:get(client_info, State), null),
            retinue_checks:observe(Component, Context#{<<"version">> => Version}, Checks);
        false -> {error, invalid_check_report}
    end;
publish_checks(_, _) -> {error, invalid_check_report}.

start_local_request(Id, Dataset, Work, State) ->
    Pending = queue:in({Id, Dataset, Work}, maps:get(worker_queue, State)),
    schedule_local_workers(State#{worker_queue => Pending}).

start_local_worker(Id, Dataset, Work, State) ->
    Parent = self(),
    Token = make_ref(),
    {Pid, Monitor} = spawn_opt(fun() ->
        Reply = try Work()
                catch Class:Reason:Stack ->
                    logger:error("local API request failed: ~p:~p~n~p",
                                 [Class, Reason, Stack]),
                    {error, {local_request_failed, Class, Reason}}
                end,
        Parent ! {local_request_result, Token, Id, Dataset, Reply}
    end, [link, monitor]),
    Worker = #{pid => Pid, monitor => Monitor, id => Id},
    Workers = maps:get(workers, State, #{}),
    State#{workers => Workers#{Token => Worker}}.

send_local_request_result(Token, Id, Dataset, Reply, State) ->
    case maps:take(Token, maps:get(workers, State, #{})) of
        error -> State;
        {#{monitor := Monitor}, Workers} ->
            erlang:demonitor(Monitor, [flush]),
            release_global_local_worker(),
            case Reply of
                {ok, Data} -> send_ok(maps:get(socket, State), Id, Dataset, Data);
                {error, Reason} -> send_error(maps:get(socket, State), Id, Reason)
            end,
            schedule_local_workers(State#{workers => Workers})
    end.

local_request_down(Monitor, Reason, State) ->
    case [{Token, Worker} ||
             {Token, Worker = #{monitor := WorkerMonitor}} <-
                 maps:to_list(maps:get(workers, State, #{})),
             WorkerMonitor =:= Monitor] of
        [{Token, #{id := Id}}] ->
            release_global_local_worker(),
            send_error(maps:get(socket, State), Id, {local_request_down, Reason}),
            schedule_local_workers(
              State#{workers => maps:remove(Token, maps:get(workers, State))});
        [] -> State
    end.

schedule_local_workers(State) ->
    State1 = start_permitted_local_workers(State),
    Slots = maps:get(max_workers, State1) - map_size(maps:get(workers, State1)),
    Needed0 = min(Slots, queue:len(maps:get(worker_queue, State1))),
    Reserved = maps:get(worker_permits, State1) +
               maps:get(worker_permit_requests, State1),
    case max(0, Needed0 - Reserved) of
        0 -> State1;
        Needed ->
            {Granted, Queued} = gen_server:call(
                                  ?SERVER, {acquire_local_workers, self(), Needed}),
            start_permitted_local_workers(
              State1#{worker_permits => maps:get(worker_permits, State1) + Granted,
                      worker_permit_requests =>
                          maps:get(worker_permit_requests, State1) + Queued})
    end.

start_permitted_local_workers(State) ->
    CanStart = maps:get(worker_permits, State) > 0 andalso
               map_size(maps:get(workers, State)) < maps:get(max_workers, State),
    case {CanStart, queue:out(maps:get(worker_queue, State))} of
        {true, {{value, {Id, Dataset, Work}}, Queue}} ->
            State1 = start_local_worker(
                       Id, Dataset, Work,
                       State#{worker_queue => Queue,
                              worker_permits => maps:get(worker_permits, State) - 1}),
            start_permitted_local_workers(State1);
        {_, {empty, Queue}} -> State#{worker_queue => Queue};
        _ -> State
    end.

release_global_local_worker() ->
    gen_server:cast(?SERVER, {release_local_worker, self()}).

local_worker_limit() ->
    case application:get_env(retinue_daemon, local_request_workers, ?DEFAULT_REQUEST_WORKERS) of
        Count when is_integer(Count), Count > 0 -> Count;
        _ -> ?DEFAULT_REQUEST_WORKERS
    end.

daemon_identity() ->
    Build = case retinue_hot_update:current_build_identity() of
        {ok, Identity} -> Identity;
        {error, _Reason} -> null
    end,
    Version = list_to_binary(retinue_build:version()),
    #{<<"status">> => <<"running">>,
      <<"node">> => atom_to_binary(node()),
      <<"version">> => Version,
      <<"otp">> => list_to_binary(erlang:system_info(otp_release)),
      <<"build">> => Build}.

request_id(Request) ->
    case maps:get(<<"id">>, Request, 0) of
        Id when is_integer(Id), Id >= 0 -> Id;
        _ -> 0
    end.

feature_enabled(Feature, State) ->
    lists:member(Feature, maps:get(features, State, [])).


send_ok(Socket, Id, Dataset, Snapshot) ->
    send_json(Socket, #{<<"id">> => Id, <<"ok">> => true,
                        <<"dataset">> => Dataset, <<"data">> => Snapshot}).

send_error(Socket, Id, Reason) ->
    send_json(Socket, #{<<"id">> => Id, <<"ok">> => false,
                        <<"error">> => format_reason(Reason)}).

send_json(Socket, Map) ->
    case socket:send(Socket, retinue_local_protocol:encode(Map)) of
        ok -> ok;
        {error, closed} -> ok;
        {error, Reason} -> exit({socket_send_failed, Reason})
    end.

format_reason(Reason) when is_binary(Reason) -> Reason;
format_reason(Reason) when is_atom(Reason) -> atom_to_binary(Reason);
format_reason(Reason) -> iolist_to_binary(io_lib:format("~p", [Reason])).

cleanup_connection(State) ->
    maps:foreach(
      fun(_Token, #{pid := Pid, monitor := Monitor}) ->
          exit(Pid, shutdown),
          erlang:demonitor(Monitor, [flush])
      end,
      maps:get(workers, State, #{})),
    exit(maps:get(reader, State), shutdown),
    erlang:demonitor(maps:get(reader_monitor, State), [flush]),
    ok.


valid_os_pid(Pid) when is_integer(Pid), Pid > 0 -> Pid;
valid_os_pid(_Pid) -> undefined.


valid_client_name(Client) when is_binary(Client), byte_size(Client) > 0,
                               byte_size(Client) =< 64 -> Client;
valid_client_name(_Client) -> <<"unknown">>.


valid_client_mode(<<"standalone">>) -> <<"standalone">>;
valid_client_mode(<<"watch">>) -> <<"watch">>;
valid_client_mode(<<"launch">>) -> <<"launch">>;
valid_client_mode(<<"desktop">>) -> <<"desktop">>;
valid_client_mode(<<"preview">>) -> <<"preview">>;
valid_client_mode(_Mode) -> <<"unknown">>.


identify_game(Game, State) ->
    {Adapter, Specs} = case Game of
        <<"warframe">> -> {wf_local_handler, wf_sup:child_specs()};
        <<"poe2">> -> {retinue_poe2_handler, retinue_poe2_sup:child_specs()}
    end,
    Existing = maps:get(owner_monitors, State),
    Monitors = maps:from_list([{erlang:monitor(process, Pid), Pid}
                              || #{id := Name} <- Specs,
                                 Pid <- [whereis(Name)], is_pid(Pid)]),
    Adapter:init(State#{adapter => Adapter, owner_monitors => maps:merge(Existing, Monitors)}).

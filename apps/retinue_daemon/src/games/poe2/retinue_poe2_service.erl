-module(retinue_poe2_service).
-behaviour(gen_server).

-export([start_link/0, status/0, health/0, events/0, request/1, request/2, subscribe/1, unsubscribe/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

start_link() -> gen_server:start_link({local, ?MODULE}, ?MODULE, [], []).
status() -> gen_server:call(?MODULE, status).
health() -> gen_server:call(?MODULE, health).
events() -> maps:get(<<"events">>, status()).
subscribe(Pid) -> gen_server:call(?MODULE, {subscribe, Pid}).
unsubscribe(Ref) -> gen_server:call(?MODULE, {unsubscribe, Ref}).

request(Request) -> request(Request, undefined).

request(#{<<"op">> := <<"get">>, <<"dataset">> := <<"session">>}, _) -> {ok, status()};
request(#{<<"op">> := <<"publish">>, <<"dataset">> := <<"session">>, <<"data">> := Data}, Producer) ->
    case retinue_poe2_session:sample(Data) of
        {ok, Sample} -> gen_server:call(?MODULE, {publish, Sample, Producer}, 30000);
        Error -> Error
    end;
request(_, _) -> {error, unsupported_poe2_request}.

init([]) ->
    Path = application:get_env(retinue_daemon, poe2_session_file,
                              retinue_paths:game_file(cache, poe2, "session.json")),
    {ok, #{path => Path, snapshot => load(Path), subscribers => #{}}}.

handle_call(status, _From, State) -> {reply, maps:get(snapshot, State), State};
handle_call(health, _From, State) ->
    Receipt = case maps:get(receipt, State, undefined) of
        {#{connection := Producer}, ReceivedAt} when is_pid(Producer) ->
            #{<<"connected">> => is_process_alive(Producer), <<"received_at">> => ReceivedAt};
        _ -> #{}
    end,
    {reply, (maps:get(snapshot, State))#{<<"receipt">> => Receipt}, State};
handle_call({publish, Sample, Producer}, _From, State) ->
    case retinue_poe2_session:apply(Sample, maps:get(snapshot, State)) of
        {error, _} = Error -> {reply, Error, State};
        {Kind, Next} when Kind =:= retry; Kind =:= heartbeat ->
            %% Heartbeats refresh read time and retry identity, not durable projection history.
            {reply, {ok, #{<<"revision">> => maps:get(<<"revision">>, Next),
                           <<"unchanged">> => true}}, received(Next, Producer, State)};
        {changed, Next} ->
            case persist(maps:get(path, State), Next) of
                ok ->
                    maps:foreach(fun(Ref, Pid) -> Pid ! {retinue_poe2_session, Ref, Next} end,
                                 maps:get(subscribers, State)),
                    {reply, {ok, #{<<"revision">> => maps:get(<<"revision">>, Next)}},
                     received(Next, Producer, State)};
                {error, Reason} ->
                    logger:warning("PoE2 session cache write failed: ~p", [Reason]),
                    {reply, {error, {session_persistence_failed, Reason}}, State}
            end
    end;
handle_call({subscribe, Pid}, _From, State) ->
    Ref = erlang:monitor(process, Pid),
    Subscribers = maps:get(subscribers, State),
    {reply, {ok, Ref, maps:get(snapshot, State)}, State#{subscribers => Subscribers#{Ref => Pid}}};
handle_call({unsubscribe, Ref}, _From, State) ->
    erlang:demonitor(Ref, [flush]),
    {reply, ok, State#{subscribers => maps:remove(Ref, maps:get(subscribers, State))}};
handle_call(_, _From, State) -> {reply, {error, unsupported_request}, State}.

handle_cast(_, State) -> {noreply, State}.
handle_info({'DOWN', Ref, process, _, _}, State) ->
    {noreply, State#{subscribers => maps:remove(Ref, maps:get(subscribers, State))}};
handle_info(_, State) -> {noreply, State}.
terminate(_, _) -> ok.
code_change(_, #{snapshot := #{<<"schema">> := 2}} = State, _) -> {ok, State};
code_change(_, State, _) -> {ok, State#{snapshot => retinue_poe2_session:empty()}}.

received(Snapshot, Producer, State) ->
    record_checks(Snapshot, Producer),
    State#{snapshot => Snapshot, receipt => {Producer, erlang:system_time(millisecond)}}.

record_checks(#{<<"observed_at">> := Time} = Snapshot, #{version := Version}) ->
    Context = #{<<"version">> => Version, <<"session">> => maps:get(<<"stream">>, Snapshot, null)},
    Checks = case Snapshot of
        #{<<"status">> := <<"observed">>, <<"current">> := Current} ->
            Issues = maps:from_list([{maps:get(<<"field">>, I), maps:get(<<"reason">>, I)}
                                    || I <- maps:get(<<"issues">>, Current)]),
            Fields = [<<"player">>, <<"area">>, <<"panels">>, <<"gold">>, <<"social">>, <<"entities">>] ++ maps:keys(Issues),
            maps:from_list([{Field, evidence(maps:get(Field, Issues, null), Time)}
                || Field <- lists:usort([<<"native_state">> | Fields]),
                   Field =:= <<"native_state">> orelse maps:is_key(Field, Issues) orelse
                   field_observed(Field, Current)]);
        #{<<"status">> := <<"error">>, <<"error">> := Error} ->
            #{<<"native_state">> => evidence(Error, Time)};
        _ -> #{}
    end,
    retinue_checks:observe(<<"poe2.companion">>, Context, Checks);
record_checks(_, _) -> ok.

field_observed(<<"gold">>, #{<<"player">> := Player}) when is_map(Player) ->
    maps:get(<<"gold">>, Player, null) =/= null;
field_observed(Field, Current) ->
    Value = maps:get(Field, Current, null),
    case Value of
        Map when is_map(Map) -> lists:any(fun(V) -> V =/= null end, maps:values(Map));
        _ -> Value =/= null
    end.

evidence(Error, Time) ->
    #{<<"checked_at">> => Time, <<"error">> => Error,
      <<"state">> => case Error of null -> <<"pass">>; _ -> <<"fail">> end}.

load(Path) ->
    case file:open(Path, [read, binary, raw]) of
        {ok, File} ->
            try file:read(File, 4 * 1024 * 1024 + 1) of
                {ok, Bytes} when byte_size(Bytes) =< 4 * 1024 * 1024 ->
                    case retinue_poe2_session:restore(json:decode(Bytes)) of
                        {ok, Snapshot} -> Snapshot;
                        _ -> invalid_cache(Path)
                    end;
                _ -> invalid_cache(Path)
            catch error:_ -> invalid_cache(Path)
            after file:close(File) end;
        {error, enoent} -> retinue_poe2_session:empty();
        {error, Reason} ->
            logger:warning("PoE2 session cache unavailable: ~p", [Reason]),
            retinue_poe2_session:empty()
    end.

invalid_cache(Path) ->
    logger:warning("Discarding invalid PoE2 session cache: ~ts", [Path]),
    retinue_poe2_session:empty().

persist(Path, Snapshot) ->
    case retinue_paths:ensure_private(filename:dirname(Path)) of
        ok ->
            Temp = Path ++ ".tmp",
            case file:write_file(Temp, json:encode(Snapshot)) of
                ok ->
                    Result = case file:change_mode(Temp, 8#600) of
                        ok -> file:rename(Temp, Path);
                        Error -> Error
                    end,
                    case Result of ok -> ok; _ -> file:delete(Temp), Result end;
                Error -> Error
            end;
        Error -> Error
    end.

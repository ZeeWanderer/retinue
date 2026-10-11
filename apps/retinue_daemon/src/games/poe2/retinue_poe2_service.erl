-module(retinue_poe2_service).
-behaviour(gen_server).

-export([start_link/0, status/0, events/0, request/1, subscribe/1, unsubscribe/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

start_link() -> gen_server:start_link({local, ?MODULE}, ?MODULE, [], []).
status() -> gen_server:call(?MODULE, status).
events() -> maps:get(<<"events">>, status()).
subscribe(Pid) -> gen_server:call(?MODULE, {subscribe, Pid}).
unsubscribe(Ref) -> gen_server:call(?MODULE, {unsubscribe, Ref}).

request(#{<<"op">> := <<"get">>, <<"dataset">> := <<"session">>}) -> {ok, status()};
request(#{<<"op">> := <<"publish">>, <<"dataset">> := <<"session">>, <<"data">> := Data}) ->
    case valid_batch(Data) of
        true -> gen_server:call(?MODULE, {publish, Data}, 30000);
        false -> {error, invalid_session_batch}
    end;
request(_) -> {error, unsupported_poe2_request}.

init([]) ->
    Path = application:get_env(retinue_daemon, poe2_session_file,
                              retinue_paths:game_file(cache, poe2, "session.json")),
    {ok, #{path => Path, snapshot => load(Path), subscribers => #{}}}.

handle_call(status, _From, State) -> {reply, maps:get(snapshot, State), State};
handle_call({publish, Data}, _From, State) ->
    Previous = maps:get(snapshot, State),
    Next = apply_batch(Data, Previous),
    case Next =:= Previous of
        true -> {reply, {ok, #{<<"revision">> => maps:get(<<"revision">>, Previous),
                              <<"unchanged">> => true}}, State};
        false ->
            case persist(maps:get(path, State), Next) of
                ok ->
                    maps:foreach(fun(Ref, Pid) -> Pid ! {retinue_poe2_session, Ref, Next} end,
                                 maps:get(subscribers, State)),
                    {reply, {ok, #{<<"revision">> => maps:get(<<"revision">>, Next)}},
                     State#{snapshot => Next}};
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
code_change(_, State, _) -> {ok, State}.

empty() -> #{<<"game">> => <<"poe2">>, <<"revision">> => 0, <<"updated_at">> => null,
             <<"source">> => null, <<"generation">> => null, <<"cursor">> => -1,
             <<"current">> => #{}, <<"events">> => []}.

apply_batch(#{<<"source">> := Source, <<"generation">> := Generation,
              <<"cursor">> := Cursor, <<"events">> := Events}, Previous) ->
    Same = Source =:= maps:get(<<"source">>, Previous) andalso
           Generation =:= maps:get(<<"generation">>, Previous),
    Last = case Same of true -> maps:get(<<"cursor">>, Previous); false -> -1 end,
    case Cursor =< Last of
        true -> Previous;
        false ->
            NewEvents = [sanitize(E) || E <- Events, maps:get(<<"offset">>, E) > Last],
            Current = case Same of true -> maps:get(<<"current">>, Previous); false -> #{} end,
            History = case Same of true -> maps:get(<<"events">>, Previous); false -> [] end,
            Combined = History ++ NewEvents,
            UpdatedAt = case NewEvents of
                [] when Same -> maps:get(<<"updated_at">>, Previous);
                [] -> null;
                _ -> erlang:system_time(millisecond)
            end,
            Previous#{<<"revision">> => maps:get(<<"revision">>, Previous) + 1,
                      <<"updated_at">> => UpdatedAt,
                      <<"source">> => Source, <<"generation">> => Generation, <<"cursor">> => Cursor,
                      <<"current">> => lists:foldl(fun project_session/2, Current, NewEvents),
                      <<"events">> => lists:nthtail(max(0, length(Combined) - 128), Combined)}
    end.

project_session(#{<<"client_pid">> := Pid, <<"tick_ms">> := Tick} = Event, Current) ->
    SameSession = maps:get(<<"client_pid">>, Current, Pid) =:= Pid andalso
                  Tick >= maps:get(<<"tick_ms">>, Current, 0),
    Base = case SameSession of true -> Current; false -> #{} end,
    project(Event, Base#{<<"client_pid">> => Pid, <<"tick_ms">> => Tick}).

project(#{<<"kind">> := <<"area_generated">>} = Event, Current) ->
    maps:merge(maps:remove(<<"area_name">>, Current),
               maps:with([<<"area_level">>, <<"area_id">>, <<"seed">>], Event));
project(#{<<"kind">> := <<"area_name">>, <<"area_name">> := Name}, Current) ->
    Current#{<<"area_name">> => Name};
project(#{<<"kind">> := <<"character_level">>, <<"character">> := Name} = Event, Current) ->
    Actors = maps:get(<<"actors">>, Current, #{}),
    Actor = maps:get(Name, Actors, #{}),
    Updated = maps:merge(Actor, maps:with([<<"character_class">>, <<"character_level">>], Event)),
    Current#{<<"actors">> => Actors#{Name => Updated}};
project(#{<<"kind">> := <<"death">>, <<"character">> := Name}, Current) ->
    Actors = maps:get(<<"actors">>, Current, #{}),
    Actor = maps:get(Name, Actors, #{}),
    Updated = Actor#{<<"observed_deaths">> => maps:get(<<"observed_deaths">>, Actor, 0) + 1},
    Current#{<<"actors">> => Actors#{Name => Updated}}.

valid_batch(#{<<"source">> := Source, <<"generation">> := Generation,
              <<"cursor">> := Cursor, <<"events">> := Events})
  when is_list(Events), length(Events) =< 128, is_integer(Cursor), Cursor >= 0 ->
    text(Source, 4096) andalso text(Generation, 256) andalso
    valid_events(Events, -1, Cursor);
valid_batch(_) -> false.

valid_events([], _, _) -> true;
valid_events([#{<<"offset">> := Offset, <<"timestamp_local">> := Time,
                <<"client_pid">> := Pid, <<"tick_ms">> := Tick} = Event | Rest], Last, Cursor)
  when is_integer(Offset), Offset > Last, Offset =< Cursor ->
    text(Time, 64) andalso number(Pid, 1, 16#ffffffff) andalso
    number(Tick, 0, 16#ffffffffffffffff) andalso valid_event(Event) andalso valid_events(Rest, Offset, Cursor);
valid_events(_, _, _) -> false.

valid_event(#{<<"kind">> := <<"area_generated">>, <<"area_id">> := Id,
              <<"area_level">> := Level, <<"seed">> := Seed}) ->
    text(Id, 256) andalso number(Level, 0, 1000) andalso number(Seed, 0, 16#ffffffffffffffff);
valid_event(#{<<"kind">> := <<"area_name">>, <<"area_name">> := Name}) -> text(Name, 512);
valid_event(#{<<"kind">> := <<"character_level">>, <<"character">> := Name,
              <<"character_class">> := Class, <<"character_level">> := Level}) ->
    text(Name, 256) andalso text(Class, 128) andalso number(Level, 1, 1000);
valid_event(#{<<"kind">> := <<"death">>, <<"character">> := Name}) -> text(Name, 256);
valid_event(_) -> false.

sanitize(Event) ->
    maps:with([<<"kind">>, <<"offset">>, <<"timestamp_local">>, <<"area_id">>,
               <<"area_level">>, <<"seed">>, <<"area_name">>, <<"character">>,
               <<"character_class">>, <<"character_level">>, <<"client_pid">>, <<"tick_ms">>], Event).

text(Value, Max) -> is_binary(Value) andalso byte_size(Value) > 0 andalso byte_size(Value) =< Max.
number(Value, Min, Max) -> is_integer(Value) andalso Value >= Min andalso Value =< Max.

load(Path) ->
    case file:read_file(Path) of
        {ok, Bytes} ->
            try json:decode(Bytes) of
                #{<<"game">> := <<"poe2">>, <<"revision">> := Revision,
                  <<"current">> := Current, <<"events">> := Events} = Snapshot
                  when is_integer(Revision), is_map(Current), is_list(Events) ->
                    maps:merge(empty(), Snapshot);
                _ -> logger:warning("Invalid PoE2 session cache: ~ts", [Path]), empty()
            catch error:_ -> logger:warning("Unreadable PoE2 session cache: ~ts", [Path]), empty() end;
        {error, enoent} -> empty();
        {error, Reason} -> logger:warning("PoE2 session cache unavailable: ~p", [Reason]), empty()
    end.

persist(Path, Snapshot) ->
    case retinue_paths:ensure_private(filename:dirname(Path)) of
        ok ->
            Temp = Path ++ ".tmp",
            case file:write_file(Temp, json:encode(Snapshot)) of
                ok ->
                    case file:change_mode(Temp, 8#600) of
                        ok -> file:rename(Temp, Path);
                        Error -> Error
                    end;
                Error -> Error
            end;
        Error -> Error
    end.

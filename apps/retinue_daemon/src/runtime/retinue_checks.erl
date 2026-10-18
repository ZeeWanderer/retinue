-module(retinue_checks).
-behaviour(gen_server).

-export([start_link/0, observe/3, report/0, read/0, stored_report/0, path/0, policy/0, valid_report/2]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).
-ifdef(TEST).
-export([merge/5, prune/2, effective_state/2]).
-endif.

-define(MAX_CHECKS, 512).
-define(HISTORY_MS, 30 * 24 * 60 * 60 * 1000).

start_link() -> gen_server:start_link({local, ?MODULE}, ?MODULE, [], []).
observe(Component, Context, Checks) when is_binary(Component), byte_size(Component) =< 128,
                                        is_map(Context), is_map(Checks), map_size(Checks) =< 64 ->
    gen_server:cast(?MODULE, {observe, Component, Context, Checks}).
report() -> gen_server:call(?MODULE, report).
path() -> application:get_env(retinue_daemon, checks_file, retinue_paths:file(state, "checks.json")).
policy() -> #{max_checks => ?MAX_CHECKS, resolved_retention_days => 30,
              resolution => successful_check, version_change => unverified,
              persistence => failures_and_recovery, write_interval_ms => 5000,
              overflow => oldest_resolved_then_oldest_open}.

valid_report(Context, Checks) when is_map(Context), is_map(Checks), map_size(Checks) =< 64 ->
    valid_context(Context) andalso maps:fold(fun(Id, Check, Valid) ->
        Valid andalso valid(Id, Check, now_ms())
    end, true, Checks);
valid_report(_, _) -> false.

valid_context(Context) when is_map(Context), map_size(Context) =< 4 ->
    lists:all(fun(Key) ->
        case maps:get(Key, Context, null) of
            null -> true;
            Value -> is_binary(Value) andalso byte_size(Value) =< 256
        end
    end, [<<"version">>, <<"game_build">>, <<"session">>]);
valid_context(_) -> false.

read() -> load(path()).
stored_report() ->
    case read() of
        {ok, Checks} -> {ok, #{checks => view(prune(Checks, now_ms())), retention => policy(), path => path(),
                               storage_error => undefined, offline => true}};
        Error -> Error
    end.

init([]) ->
    {Checks, Error} = case read() of
        {ok, Saved} -> {Saved, undefined};
        {error, Reason} -> {#{}, Reason}
    end,
    erlang:send_after(3600000, self(), maintenance),
    State = #{checks => Checks, timer => undefined, error => Error, dirty => false, evicted => 0},
    {ok, changed(prune(Checks, now_ms()), State)}.

handle_call(report, _From, State) ->
    {reply, #{checks => view(maps:get(checks, State)), retention => policy(),
              evicted_since_start => maps:get(evicted, State),
              storage_error => maps:get(error, State), path => path()}, State};
handle_call(_, _From, State) -> {reply, {error, unsupported_request}, State}.

handle_cast({observe, Component, Context, Checks}, State) ->
    Current = maps:get(checks, State),
    Merged = merge(Current, Component, Context, Checks, now_ms()),
    Next = prune(Merged, now_ms()),
    {noreply, changed(Next, State#{evicted := maps:get(evicted, State) +
                                           max(0, map_size(Merged) - ?MAX_CHECKS)})};
handle_cast(_, State) -> {noreply, State}.

handle_info(persist, State) -> {noreply, persist(State#{timer => undefined})};
handle_info(maintenance, State) ->
    erlang:send_after(3600000, self(), maintenance),
    {noreply, changed(prune(maps:get(checks, State), now_ms()), State)};
handle_info(_, State) -> {noreply, State}.
terminate(_, State) -> persist(State), ok.
code_change(_, State, _) -> {ok, State}.

merge(Existing, Component, Context0, Incoming, Now) ->
    Context = maps:from_list([{Key, V} || Key <- [<<"version">>, <<"game_build">>, <<"session">>],
                                          V <- [maps:get(Key, Context0, null)],
                                          V =:= null orelse (is_binary(V) andalso byte_size(V) =< 256)]),
    maps:fold(fun(Id, Evidence, Acc) ->
        case valid(Id, Evidence, Now) of
            true ->
                Key = {Component, Id},
                Previous = maps:get(Key, Acc, #{}),
                case duplicate(Previous, Evidence, Context) of
                    true -> Acc;
                    false -> Acc#{Key => update_with_failure(Previous, Component, Id, Evidence, Context, Now)}
                end;
            false -> Acc
        end
    end, Existing, Incoming).

valid(Id, #{<<"state">> := State, <<"checked_at">> := Time} = Check, Now)
  when is_binary(Id), byte_size(Id) > 0, byte_size(Id) =< 128,
       (State =:= <<"pass">> orelse State =:= <<"fail">>),
       is_integer(Time), Time > 0, Time =< Now + 5000 ->
    Valid = case maps:get(<<"error">>, Check, null) of
        null -> State =:= <<"pass">>;
        Error -> State =:= <<"fail">> andalso is_binary(Error) andalso byte_size(Error) =< 8192
    end,
    Valid andalso case maps:get(<<"last_failure">>, Check, null) of
        null -> true;
        #{<<"checked_at">> := Failed, <<"error">> := Reason} ->
            is_integer(Failed) andalso Failed > 0 andalso Failed =< Time andalso
            is_binary(Reason) andalso byte_size(Reason) =< 8192;
        _ -> false
    end;
valid(_, _, _) -> false.

duplicate(#{<<"checked_at">> := Before}, #{<<"checked_at">> := Time}, _) when Time < Before -> true;
duplicate(#{<<"context">> := Context, <<"state">> := State}, #{<<"state">> := <<"pass">>}, Context)
  when State =:= <<"healthy">>; State =:= <<"resolved">> -> false;
duplicate(#{<<"context">> := Context, <<"checked_at">> := Before}, Check, Context) ->
    maps:get(<<"checked_at">>, Check) =< Before;
duplicate(_, _, _) -> false.

update_with_failure(Previous, Component, Id, Check, Context, Now) ->
    Failure = maps:get(<<"last_failure">>, Check, null),
    WithFailure = case {maps:get(<<"state">>, Check), Failure} of
        {<<"pass">>, #{<<"checked_at">> := Time}} ->
            case Time > maps:get(<<"checked_at">>, Previous, 0) of
                true -> update(Previous, Component, Id, Failure#{<<"state">> => <<"fail">>}, Context, Now);
                false -> Previous
            end;
        _ -> Previous
    end,
    case {maps:get(<<"state">>, Check), maps:get(<<"state">>, WithFailure, undefined),
          maps:get(<<"context">>, WithFailure, undefined)} of
        {<<"pass">>, State, Context} when State =:= <<"healthy">>; State =:= <<"resolved">> -> WithFailure;
        _ -> update(WithFailure, Component, Id, Check, Context, Now)
    end.

update(Previous, Component, Id, Check, Context, Now) ->
    Time = maps:get(<<"checked_at">>, Check),
    Next = Previous#{<<"component">> => Component, <<"check">> => Id,
                     <<"context">> => Context, <<"checked_at">> => Time,
                     <<"received_at">> => Now},
    case maps:get(<<"state">>, Check) of
        <<"fail">> ->
            Next#{<<"state">> => <<"open">>,
                  <<"error">> => maps:get(<<"error">>, Check),
                  <<"first_failed_at">> => maps:get(<<"first_failed_at">>, Previous, Time),
                  <<"first_failure_context">> => maps:get(<<"first_failure_context">>, Previous, Context),
                  <<"last_failed_at">> => Time, <<"last_failure_context">> => Context,
                  <<"failed_observations">> => maps:get(<<"failed_observations">>, Previous, 0) + 1};
        <<"pass">> ->
            Next#{<<"state">> => case maps:is_key(<<"first_failed_at">>, Previous) of
                                    true -> <<"resolved">>; false -> <<"healthy">> end,
                  <<"resolved_at">> => case maps:get(<<"state">>, Previous, undefined) of
                      <<"resolved">> -> maps:get(<<"resolved_at">>, Previous, Time);
                      _ -> Time
                  end,
                  <<"last_success_at">> => Time}
    end.

prune(Checks, Now) ->
    Retained = [expire_history(Check, Now) || Check <- maps:values(Checks),
        maps:get(<<"state">>, Check) =:= <<"open">> orelse
        Now - maps:get(<<"received_at">>, Check) =< ?HISTORY_MS],
    Sorted = lists:sort(fun(A, B) ->
        {maps:get(<<"state">>, A) =:= <<"open">>, maps:get(<<"received_at">>, A),
         maps:get(<<"component">>, A), maps:get(<<"check">>, A)} >
        {maps:get(<<"state">>, B) =:= <<"open">>, maps:get(<<"received_at">>, B),
         maps:get(<<"component">>, B), maps:get(<<"check">>, B)}
    end, Retained),
    maps:from_list([{{maps:get(<<"component">>, C), maps:get(<<"check">>, C)}, C}
                   || C <- lists:sublist(Sorted, ?MAX_CHECKS)]).

expire_history(#{<<"state">> := State, <<"resolved_at">> := Time} = Check, Now)
  when State =/= <<"open">>, Now - Time > ?HISTORY_MS ->
    (maps:without([<<"first_failed_at">>, <<"last_failed_at">>, <<"first_failure_context">>,
                   <<"last_failure_context">>, <<"failed_observations">>, <<"error">>, <<"resolved_at">>], Check))#{
        <<"state">> := <<"healthy">>};
expire_history(Check, _) -> Check.

effective_state(Check, Version) ->
    case maps:get(<<"version">>, maps:get(<<"context">>, Check), null) of
        Version -> maps:get(<<"state">>, Check);
        _ -> <<"unverified">>
    end.

view(Checks) ->
    Version = unicode:characters_to_binary(retinue_build:version()),
    [C#{<<"state">> => effective_state(C, Version), <<"last_result">> => maps:get(<<"state">>, C)}
     || C <- lists:sort(maps:values(Checks))].

changed(Checks, #{checks := Checks} = State) -> State;
changed(Checks, State) ->
    Next = State#{checks => Checks},
    case durable(Checks) =:= durable(maps:get(checks, State)) of
        true -> Next;
        false ->
            Timer = case maps:get(timer, State) of
                undefined -> erlang:send_after(5000, self(), persist);
                Existing -> Existing
            end,
            Next#{dirty => true, timer => Timer}
    end.

durable(Checks) -> maps:filter(fun(_, C) -> maps:is_key(<<"first_failed_at">>, C) end, Checks).

persist(#{dirty := false} = State) -> State;
persist(State) ->
    Path = path(),
    Result = case retinue_paths:ensure_private(filename:dirname(Path)) of
        ok ->
            Temp = Path ++ ".tmp",
            Data = json:encode(#{<<"schema">> => 1, <<"checks">> => maps:values(durable(maps:get(checks, State)))}),
            case file:write_file(Temp, Data) of
                ok ->
                    case file:change_mode(Temp, 8#600) of
                        ok -> file:rename(Temp, Path);
                        Error -> Error
                    end;
                Error -> Error
            end;
        Error -> Error
    end,
    case Result of
        ok -> State#{error => undefined, dirty => false};
        {error, Reason} ->
            logger:warning("diagnostic check storage failed: ~p", [Reason]),
            State#{error => Reason, timer => erlang:send_after(30000, self(), persist)}
    end.

load(Path) ->
    case file:open(Path, [read, binary, raw]) of
        {ok, File} ->
            try case file:read(File, 16 * 1024 * 1024 + 1) of
                {ok, Bytes} when byte_size(Bytes) =< 16 * 1024 * 1024 ->
                    #{<<"schema">> := 1, <<"checks">> := Checks} = json:decode(Bytes),
                    true = length(Checks) =< ?MAX_CHECKS,
                    true = lists:all(fun valid_stored/1, Checks),
                    Rows = maps:from_list([{{maps:get(<<"component">>, C), maps:get(<<"check">>, C)}, C}
                                            || C <- Checks]),
                    {ok, Rows};
                _ -> {error, invalid_check_store}
            end
            catch error:_ -> {error, invalid_check_store}
            after file:close(File) end;
        {error, enoent} -> {ok, #{}};
        Error -> Error
    end.

valid_stored(#{<<"component">> := Component, <<"check">> := Id, <<"context">> := Context,
               <<"state">> := State, <<"checked_at">> := Checked,
               <<"received_at">> := Received, <<"first_failed_at">> := First,
               <<"last_failed_at">> := Last, <<"error">> := Error,
               <<"first_failure_context">> := FirstContext, <<"last_failure_context">> := LastContext,
               <<"failed_observations">> := Count} = Check) ->
    is_binary(Component) andalso byte_size(Component) =< 128 andalso
    is_binary(Id) andalso byte_size(Id) > 0 andalso byte_size(Id) =< 128 andalso valid_context(Context) andalso
    lists:member(State, [<<"open">>, <<"resolved">>]) andalso
    is_integer(Checked) andalso Checked > 0 andalso is_integer(Received) andalso Received > 0 andalso
    is_integer(First) andalso First > 0 andalso is_integer(Last) andalso Last >= First andalso
    is_binary(Error) andalso byte_size(Error) =< 8192 andalso
    valid_context(FirstContext) andalso valid_context(LastContext) andalso
    is_integer(Count) andalso Count > 0 andalso
    case State of
        <<"open">> -> true;
        <<"resolved">> -> is_integer(maps:get(<<"resolved_at">>, Check, null)) andalso
                          is_integer(maps:get(<<"last_success_at">>, Check, null))
    end;
valid_stored(_) -> false.

now_ms() -> erlang:system_time(millisecond).

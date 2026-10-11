%%%-------------------------------------------------------------------
%% Persistent worldstate fetch, query, and subscription service.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_service).

-behaviour(gen_server).

-include_lib("retinue_daemon/include/wfcli_worldstate.hrl").

-export([start_link/0, submit/2, subscribe/2, unsubscribe/1, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(DEFAULT_POLL_INTERVAL_MS, 60000).

-type request() :: map().
-type state() :: map().

-doc "Start daemon-owned worldstate store and scheduler.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Queue a one-shot request; result arrives as `{retinue_daemon, Ref, Reply}`.".
-spec submit(pid(), request()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Register persistent watch; updates arrive as `{retinue_daemon, Ref, Reply}`.".
-spec subscribe(pid(), request()) -> {ok, reference()} | {error, term()}.
subscribe(Client, Request) ->
    gen_server:call(?SERVER, {subscribe, Client, Request}).

-doc "Remove one watch or queued request by reference.".
-spec unsubscribe(reference()) -> ok.
unsubscribe(Ref) ->
    gen_server:call(?SERVER, {unsubscribe, Ref}).

-doc "Return queue, watch, and snapshot counts for daemon status.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-spec init([]) -> {ok, state()}.
init([]) ->
    process_flag(trap_exit, true),
    PollMs = daemon_env(daemon_poll_interval_ms, ?DEFAULT_POLL_INTERVAL_MS),
    {ok, #{datasets => #{}, one_shots => #{}, watches => #{}, monitors => #{},
           activity_held => false, poll_timer => undefined, poll_interval_ms => PollMs}}.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call({submit, Client, Request}, _From, State) when is_pid(Client), is_map(Request) ->
    case maps:get(source, Request, worldstate) of
        Source when Source =:= worldstate; Source =:= trader; Source =:= teshin ->
            case prepare_submission(Source, Request) of
                {ok, Prepared} ->
                    Ref = make_ref(),
                    Monitor = erlang:monitor(process, Client),
                    Key = dataset_key(Prepared),
                    Sub = #{kind => one_shot, ref => Ref, client => Client, monitor => Monitor,
                            request => Prepared, dataset => Key},
                    State1 = hold_activity(State),
                    State2 = put_subscription(one_shots, Ref, Sub, State1),
                    State3 = put_monitor(Monitor, {one_shot, Ref}, State2),
                    State4 = ensure_request_ready(Key, Ref, Prepared, State3),
                    {reply, {ok, Ref}, State4};
                {error, _Reason} = Error -> {reply, Error, State}
            end;
        Source ->
            {reply, {error, {unsupported_source, Source}}, State}
    end;
handle_call({subscribe, Client, Request0}, _From, State) when is_pid(Client), is_map(Request0) ->
    case maps:get(source, Request0, worldstate) of
        worldstate ->
            case wfcli_worldstate_ops:prepare_watch(Request0) of
                {ok, Request} ->
                    Ref = make_ref(),
                    Monitor = erlang:monitor(process, Client),
                    Key = dataset_key(Request),
                    IntervalMs = max(1000, maps:get(interval, Request, 60) * 1000),
                    Sub = #{kind => watch, ref => Ref, client => Client, monitor => Monitor,
                            request => Request, dataset => Key, previous => #{},
                            next_due => erlang:monotonic_time(millisecond), interval_ms => IntervalMs},
                    State1 = hold_activity(State),
                    State2 = put_subscription(watches, Ref, Sub, State1),
                    State3 = put_monitor(Monitor, {watch, Ref}, State2),
                    State4 = ensure_watch_ready(Key, Ref, Request, State3),
                    State5 = ensure_poll_timer(State4),
                    {reply, {ok, Ref}, State5};
                {error, _Reason} = Error -> {reply, Error, State}
            end;
        Source ->
            {reply, {error, {unsupported_source, Source}}, State}
    end;
handle_call({unsubscribe, Ref}, _From, State) ->
    {reply, ok, update_activity(remove_subscription(Ref, State))};
handle_call(status, _From, State) ->
    Datasets = maps:get(datasets, State),
    Reply = #{datasets => map_size(Datasets),
              snapshots => length([ok || {_Key, Dataset} <- maps:to_list(Datasets),
                                          maps:get(snapshot, Dataset, undefined) =/= undefined]),
              one_shots => map_size(maps:get(one_shots, State)),
              watches => map_size(maps:get(watches, State)),
              fetching => length([ok || {_Key, Dataset} <- maps:to_list(Datasets),
                                        maps:get(fetch, Dataset, undefined) =/= undefined])},
    {reply, Reply, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast(_Message, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info({dispatch_one_shots, Key}, State) ->
    {noreply, update_activity(dispatch_one_shots(Key, memory, State))};
handle_info({dispatch_one_shot, Ref, Delivery}, State) ->
    {noreply, update_activity(dispatch_one_shot(Ref, Delivery, State))};
handle_info({evaluate_watch, Ref}, State) ->
    {noreply, update_activity(evaluate_watch(Ref, memory, State))};
handle_info({evaluate_watch, Ref, Delivery}, State) ->
    {noreply, update_activity(evaluate_watch(Ref, Delivery, State))};
handle_info({worldstate_fetch_result, Key, Token, Result}, State) ->
    {noreply, update_activity(handle_fetch_result(Key, Token, Result, State))};
handle_info({'DOWN', Monitor, process, _Pid, Reason}, State) ->
    case find_fetch_monitor(Monitor, State) of
        {ok, Key, Token} ->
            {noreply, update_activity(handle_fetch_result(
                                   Key, Token, {error, {fetch_worker_down, Reason}}, State))};
        error ->
            handle_client_down(Monitor, State)
    end;
handle_info(poll, State) ->
    State1 = State#{poll_timer => undefined},
    Keys = watch_dataset_keys(State1),
    State2 = lists:foldl(fun poll_dataset/2, State1, Keys),
    {noreply, update_activity(ensure_poll_timer(State2))};
handle_info(_Message, State) ->
    {noreply, State}.

handle_client_down(Monitor, State) ->
    case maps:get(Monitor, maps:get(monitors, State), undefined) of
        undefined -> {noreply, State};
        {_Kind, Ref} -> {noreply, update_activity(remove_subscription(Ref, State))}
    end.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, State) ->
    cancel_timer(maps:get(poll_timer, State, undefined)),
    maps:foreach(fun(_, Dataset) ->
                     case maps:get(fetch, Dataset, undefined) of
                         #{pid := Pid, monitor := Ref} ->
                             exit(Pid, shutdown),
                             receive {'DOWN', Ref, process, Pid, _} -> ok
                             after 1000 -> ok
                             end;
                         _ -> ok
                     end
                 end, maps:get(datasets, State)),
    ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    process_flag(trap_exit, true),
    {ok, State}.

dataset_key(Request) ->
    Source = maps:get(source, Request, worldstate),
    Opts = maps:get(opts, Request, #{}),
    Default = case Source of
        trader -> wfcli_worldstate:default_trader_cache();
        teshin -> calculated;
        _ -> wfcli_worldstate:default_cache()
    end,
    {Source, maps:get(cache, Opts, Default)}.

new_dataset() ->
    #{snapshot => undefined, variants => #{}, fetched_at => 0,
      source => undefined, fetch => undefined, last_error => undefined}.

get_dataset(Key, State) ->
    maps:get(Key, maps:get(datasets, State), new_dataset()).

put_dataset(Key, Dataset, State) ->
    Datasets = maps:get(datasets, State),
    State#{datasets => Datasets#{Key => Dataset}}.

put_subscription(Bucket, Ref, Sub, State) ->
    Subs = maps:get(Bucket, State),
    State#{Bucket => Subs#{Ref => Sub}}.

put_monitor(Monitor, Value, State) ->
    Monitors = maps:get(monitors, State),
    State#{monitors => Monitors#{Monitor => Value}}.

ensure_request_ready(Key, Ref, Request, State) ->
    Dataset = get_dataset(Key, State),
    case snapshot_fresh(Request, Dataset) of
        true ->
            self() ! {dispatch_one_shot, Ref, memory},
            State;
        false ->
            start_fetch(Key, Request, State)
    end.

ensure_watch_ready(Key, Ref, Request, State) ->
    Dataset = get_dataset(Key, State),
    case snapshot_fresh(Request, Dataset) of
        true ->
            self() ! {evaluate_watch, Ref, memory},
            State;
        false ->
            start_fetch(Key, Request, State)
    end.

snapshot_fresh(Request, Dataset) ->
    case {maps:get(snapshot, Dataset, undefined), maps:get(refresh, maps:get(opts, Request, #{}), false)} of
        {undefined, _} -> false;
        {_, true} -> false;
        {_, false} ->
            TtlMs = maps:get(ttl, maps:get(opts, Request, #{}), 60) * 1000,
            age_ms(Dataset) =< TtlMs
    end.

age_ms(Dataset) ->
    max(0, erlang:monotonic_time(millisecond) - maps:get(fetched_at, Dataset, 0)).

start_fetch(Key, Request, State) ->
    Dataset = get_dataset(Key, State),
    case maps:get(fetch, Dataset, undefined) of
        undefined ->
            Token = make_ref(),
            Parent = self(),
            LoadOpts = fetch_opts(Key, Request, Dataset),
            Source = maps:get(source, Request, worldstate),
            {Pid, Monitor} = spawn_opt(fun() ->
                Result = try fetch_source(Source, LoadOpts)
                         catch Class:Reason:Stack -> {error, {fetch_crash, Class, Reason, Stack}}
                         end,
                Parent ! {worldstate_fetch_result, Key, Token, Result}
            end, [link, monitor]),
            put_dataset(Key, Dataset#{fetch => #{pid => Pid, token => Token,
                                                 monitor => Monitor}}, State);
        _ ->
            State
    end.

fetch_opts({_Source, Cache}, Request, Dataset) ->
    RequestOpts = maps:get(opts, Request, #{}),
    Force = maps:get(refresh, RequestOpts, false) orelse
            maps:get(snapshot, Dataset, undefined) =/= undefined,
    Base = #{cache => Cache,
             ttl => maps:get(ttl, RequestOpts, 60),
             refresh => Force,
             resolve_items => true,
             raw => false,
             search_raw => false},
    case daemon_env(daemon_worldstate_fetch_fun, undefined) of
        Fun when is_function(Fun, 0) -> Base#{fetch_fun => Fun};
        _ -> Base
    end.

handle_fetch_result(Key, Token, Result, State) ->
    Dataset = get_dataset(Key, State),
    case maps:get(fetch, Dataset, undefined) of
        #{token := Token, monitor := Monitor} ->
            erlang:demonitor(Monitor, [flush]),
            {State1, Delivery} = case Result of
                {ok, Ws, Source} ->
                    DefaultVariant = {true, false, false, undefined},
                    Dataset1 = Dataset#{snapshot => Ws, variants => #{DefaultVariant => Ws},
                                        fetched_at => erlang:monotonic_time(millisecond),
                                        source => Source, fetch => undefined,
                                        last_error => undefined},
                    {put_dataset(Key, Dataset1, State), Source};
                {error, FetchReason} ->
                    {put_dataset(Key, Dataset#{fetch => undefined, last_error => FetchReason}, State),
                     memory}
            end,
            State2 = dispatch_one_shots(Key, Delivery, State1),
            case {Result, maps:get(snapshot, get_dataset(Key, State2), undefined)} of
                {{error, NotifyReason}, undefined} -> notify_watch_errors(Key, NotifyReason, State2);
                _ -> evaluate_due_watches(Key, Delivery, State2)
            end;
        _ ->
            State
    end.

find_fetch_monitor(Monitor, State) ->
    case [{Key, maps:get(token, Fetch)}
          || {Key, Dataset} <- maps:to_list(maps:get(datasets, State)),
             Fetch <- [maps:get(fetch, Dataset, undefined)],
             is_map(Fetch), maps:get(monitor, Fetch, undefined) =:= Monitor] of
        [{Key, Token} | _] -> {ok, Key, Token};
        [] -> error
    end.

notify_watch_errors(Key, Reason, State) ->
    Refs = [Ref || {Ref, Sub} <- maps:to_list(maps:get(watches, State)),
                   maps:get(dataset, Sub) =:= Key],
    lists:foreach(
      fun(Ref) ->
          Sub = maps:get(Ref, maps:get(watches, State)),
          maps:get(client, Sub) ! {retinue_daemon, Ref, {error, Reason}}
      end,
      Refs),
    State.

dispatch_one_shots(Key, Delivery, State) ->
    Refs = [Ref || {Ref, Sub} <- maps:to_list(maps:get(one_shots, State)),
                   maps:get(dataset, Sub) =:= Key],
    lists:foldl(fun(Ref, Acc) -> dispatch_one_shot(Ref, Delivery, Acc) end, State, Refs).

dispatch_one_shot(Ref, Delivery, State) ->
    case maps:get(Ref, maps:get(one_shots, State), undefined) of
        undefined -> State;
        Sub ->
            Key = maps:get(dataset, Sub),
            Dataset = get_dataset(Key, State),
            Reply = case maps:get(snapshot, Dataset, undefined) of
                undefined -> {error, maps:get(last_error, Dataset, no_worldstate)};
                _ -> execute_request(maps:get(request, Sub), Dataset, Delivery)
            end,
            case Reply of
                {ok, Result1, Dataset2} ->
                    maps:get(client, Sub) ! {retinue_daemon, Ref, {ok, Result1}},
                    remove_subscription(Ref, put_dataset(Key, Dataset2, State));
                {error, Reason, Dataset2} ->
                    maps:get(client, Sub) ! {retinue_daemon, Ref, {error, Reason}},
                    remove_subscription(Ref, put_dataset(Key, Dataset2, State));
                Error ->
                    maps:get(client, Sub) ! {retinue_daemon, Ref, Error},
                    remove_subscription(Ref, State)
            end
    end.

execute_request(Request, Dataset, Delivery) ->
    case maps:get(source, Request, worldstate) of
        trader ->
            Result = maps:merge(
                       #{kind => trader, entries => maps:get(snapshot, Dataset)},
                       snapshot_metadata(Dataset, Delivery)),
            {ok, Result, Dataset};
        worldstate -> execute_worldstate_request(Request, Dataset, Delivery);
        teshin -> execute_worldstate_request(Request, Dataset, Delivery)
    end.

execute_worldstate_request(Request, Dataset, Delivery) ->
    {Ws, Dataset1} = ensure_variant(maps:get(opts, Request, #{}), Dataset),
    case wfcli_worldstate_ops:execute(Ws, Request) of
        {error, Reason} -> {error, Reason, Dataset1};
        Result0 ->
            Result = maps:merge(Result0, snapshot_metadata(Dataset1, Delivery)),
            {ok, Result, Dataset1}
    end.

prepare_submission(worldstate, Request) -> wfcli_worldstate_ops:prepare_request(Request);
prepare_submission(_Source, Request) -> {ok, Request}.

snapshot_metadata(Dataset, Delivery) ->
    AgeMs = age_ms(Dataset),
    Origin = maps:get(source, Dataset, undefined),
    #{source => Delivery,
      snapshot_origin => Origin,
      snapshot_age_ms => AgeMs,
      fetched_age_ms => AgeMs,
      stale => Origin =:= cached_stale orelse
               maps:get(last_error, Dataset, undefined) =/= undefined,
      fetch_error => maps:get(last_error, Dataset, undefined)}.

fetch_source(worldstate, Opts) -> wfcli_worldstate:load(Opts);
fetch_source(trader, Opts) -> wfcli_worldstate:load_trader_inventory(Opts);
fetch_source(teshin, Opts) -> wfcli_teshin:load(Opts).

ensure_variant(Opts, Dataset) ->
    StaticKey = variant_key(Opts),
    Context = wfcli_archimedea_loadout:context(),
    Key = {StaticKey, maps:get(key, Context, unavailable)},
    Variants = maps:get(variants, Dataset, #{}),
    case maps:get(Key, Variants, undefined) of
        undefined ->
            Base = maps:get(snapshot, Dataset),
            VariantOpts = (variant_opts(Opts))#{archimedea_loadout => Context},
            Ws = wfcli_worldstate:reindex(Base, VariantOpts),
            Current = maps:filter(
                        fun(StoredKey, _Ws) ->
                            not same_variant_family(StoredKey, StaticKey)
                        end,
                        Variants),
            {Ws, Dataset#{variants => Current#{Key => Ws}}};
        Ws ->
            {Ws, Dataset}
    end.

same_variant_family({StoredStatic, _Context}, Static)
  when is_tuple(StoredStatic) ->
    StoredStatic =:= Static;
same_variant_family(StoredStatic, Static) -> StoredStatic =:= Static.

variant_key(Opts) ->
    {maps:get(resolve_items, Opts, true),
     maps:get(raw, Opts, false),
     maps:get(search_raw, Opts, maps:get(raw, Opts, false)),
     maps:get(utc, Opts, false),
     maps:get(event_lang, Opts, undefined)}.

variant_opts(Opts) ->
    #{resolve_items => maps:get(resolve_items, Opts, true),
      raw => maps:get(raw, Opts, false),
      search_raw => maps:get(search_raw, Opts, maps:get(raw, Opts, false)),
      utc => maps:get(utc, Opts, false),
      event_lang => maps:get(event_lang, Opts, undefined)}.

evaluate_due_watches(Key, Delivery, State) ->
    Now = erlang:monotonic_time(millisecond),
    Refs = [Ref || {Ref, Sub} <- maps:to_list(maps:get(watches, State)),
                   maps:get(dataset, Sub) =:= Key,
                   maps:get(next_due, Sub, 0) =< Now],
    lists:foldl(fun(Ref, Acc) -> evaluate_watch(Ref, Delivery, Acc) end, State, Refs).

evaluate_watch(Ref, Delivery, State) ->
    case maps:get(Ref, maps:get(watches, State), undefined) of
        undefined -> State;
        Sub ->
            Key = maps:get(dataset, Sub),
            Dataset = get_dataset(Key, State),
            case maps:get(snapshot, Dataset, undefined) of
                undefined -> State;
                _ ->
                    Request = maps:get(request, Sub),
                    {Ws, Dataset1} = ensure_variant(maps:get(opts, Request, #{}), Dataset),
                    {Update0, Previous} = wfcli_worldstate_ops:evaluate_watch(
                                           Ws, Request, maps:get(previous, Sub, #{})),
                    Update = maps:merge(Update0, snapshot_metadata(Dataset1, Delivery)),
                    Initial = maps:get(previous, Sub, #{}) =:= #{},
                    Always = maps:get(always, Request, false),
                    ShouldSend = Initial orelse Always orelse maps:get(changed, Update, false),
                    case ShouldSend of
                        true -> maps:get(client, Sub) ! {retinue_daemon, Ref, {ok, Update}};
                        false -> ok
                    end,
                    State1 = put_dataset(Key, Dataset1, State),
                    case maps:get(once, Request, false) of
                        true -> remove_subscription(Ref, State1);
                        false ->
                            NextDue = erlang:monotonic_time(millisecond) + maps:get(interval_ms, Sub),
                            Sub1 = Sub#{previous => Previous, next_due => NextDue},
                            Watches = maps:get(watches, State1),
                            State1#{watches => Watches#{Ref => Sub1}}
                    end
            end
    end.

poll_dataset(Key, State) ->
    case first_watch_request(Key, State) of
        undefined -> State;
        Request ->
            Opts = maps:get(opts, Request, #{}),
            start_fetch(Key, Request#{opts => Opts#{refresh => true}}, State)
    end.

first_watch_request(Key, State) ->
    case [maps:get(request, Sub) || {_Ref, Sub} <- maps:to_list(maps:get(watches, State)),
                                    maps:get(dataset, Sub) =:= Key] of
        [Request | _] -> Request;
        [] -> undefined
    end.

watch_dataset_keys(State) ->
    lists:usort([maps:get(dataset, Sub) || {_Ref, Sub} <- maps:to_list(maps:get(watches, State))]).

ensure_poll_timer(State) ->
    case {map_size(maps:get(watches, State)), maps:get(poll_timer, State, undefined)} of
        {0, Timer} ->
            cancel_timer(Timer),
            State#{poll_timer => undefined};
        {_, undefined} ->
            Timer = erlang:send_after(maps:get(poll_interval_ms, State), self(), poll),
            State#{poll_timer => Timer};
        _ -> State
    end.

remove_subscription(Ref, State) ->
    case find_subscription(Ref, State) of
        undefined -> State;
        {Bucket, Sub} ->
            Monitor = maps:get(monitor, Sub),
            erlang:demonitor(Monitor, [flush]),
            Subs = maps:remove(Ref, maps:get(Bucket, State)),
            Monitors = maps:remove(Monitor, maps:get(monitors, State)),
            ensure_poll_timer(State#{Bucket => Subs, monitors => Monitors})
    end.

find_subscription(Ref, State) ->
    case maps:get(Ref, maps:get(one_shots, State), undefined) of
        undefined ->
            case maps:get(Ref, maps:get(watches, State), undefined) of
                undefined -> undefined;
                Sub -> {watches, Sub}
            end;
        Sub -> {one_shots, Sub}
    end.

idle(State) ->
    map_size(maps:get(one_shots, State)) =:= 0 andalso
    map_size(maps:get(watches, State)) =:= 0 andalso
    lists:all(fun({_Key, Dataset}) -> maps:get(fetch, Dataset, undefined) =:= undefined end,
              maps:to_list(maps:get(datasets, State))).

update_activity(State) ->
    case idle(State) of
        true -> release_activity(State);
        false -> hold_activity(State)
    end.

hold_activity(State = #{activity_held := true}) -> State;
hold_activity(State = #{activity_held := false}) ->
    ok = retinue_activity:activity_start(),
    State#{activity_held => true}.

release_activity(State = #{activity_held := false}) -> State;
release_activity(State = #{activity_held := true}) ->
    ok = retinue_activity:activity_end(),
    State#{activity_held => false}.

cancel_timer(undefined) -> ok;
cancel_timer(Timer) ->
    _ = erlang:cancel_timer(Timer),
    ok.

daemon_env(Key, Default) ->
    application:get_env(retinue_daemon, Key, Default).

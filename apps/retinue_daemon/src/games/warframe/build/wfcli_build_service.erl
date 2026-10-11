%%%-------------------------------------------------------------------
%% Source-neutral build catalog, revision cache, and request coordinator.
%%%-------------------------------------------------------------------
-module(wfcli_build_service).

-behaviour(gen_server).

-export([start_link/0, submit/2, submit_cli/2, status/0, revision/2, revision/3,
         groups/0, group/1, create_group/2, update_group/4, delete_group/2,
         add_source_member/5, add_config_member/5, remove_member/3,
         plan_group/3,
         subscribe/1, unsubscribe/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(DEFAULT_WORKERS, 4).
-define(DEFAULT_CATALOG_TTL_MS, 86400000).
-define(PERSIST_DELAY_MS, 250).

-doc "Start the daemon-owned build repository and source coordinator.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Submit a build request; result arrives as `{retinue_build, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) when is_pid(Client), is_map(Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-spec submit_cli(pid(), map()) -> {ok, reference()} | {error, term()}.
submit_cli(Client, Request) when is_pid(Client), is_map(Request) ->
    gen_server:call(?SERVER, {submit_cli, Client, Request}).

-doc "Return build worker, cache, and repository state.".
-spec status() -> map().
status() -> gen_server:call(?SERVER, status).

-doc "Return the latest stored revision for one source build identity.".
-spec revision(binary(), integer() | binary()) -> {ok, map()} | {error, term()}.
revision(Source, ExternalId) ->
    gen_server:call(?SERVER, {revision, Source, ExternalId}).

-doc "Return one exact immutable source revision.".
-spec revision(binary(), integer() | binary(), binary()) ->
    {ok, map()} | {error, term()}.
revision(Source, ExternalId, Fingerprint) ->
    gen_server:call(?SERVER, {revision, Source, ExternalId, Fingerprint}).

-doc "List saved build groups.".
-spec groups() -> {ok, map()} | {error, term()}.
groups() -> gen_server:call(?SERVER, groups).

-doc "Return one saved build group.".
-spec group(binary()) -> {ok, map()} | {error, term()}.
group(Id) -> gen_server:call(?SERVER, {group, Id}).

-doc "Create a build group against the supplied equipment snapshot.".
-spec create_group(map(), map()) -> {ok, map()} | {error, term()}.
create_group(Input, Equipment) ->
    gen_server:call(?SERVER, {create_group, Input, Equipment}).

-doc "Update group metadata using optimistic revision matching.".
-spec update_group(binary(), pos_integer(), map(), map()) ->
    {ok, map()} | {error, term()}.
update_group(Id, Revision, Patch, Equipment) ->
    gen_server:call(?SERVER, {update_group, Id, Revision, Patch, Equipment}).

-doc "Delete one group when its revision still matches.".
-spec delete_group(binary(), pos_integer()) -> ok | {error, term()}.
delete_group(Id, Revision) -> gen_server:call(?SERVER, {delete_group, Id, Revision}).

-doc "Attach a stored source revision to a group.".
-spec add_source_member(binary(), pos_integer(), binary(), integer() | binary(),
                        binary() | latest) -> {ok, map()} | {error, term()}.
add_source_member(Id, Revision, Source, ExternalId, Fingerprint) ->
    gen_server:call(?SERVER, {add_source_member, Id, Revision, Source,
                              ExternalId, Fingerprint}).

-doc "Capture one current player configuration into a group.".
-spec add_config_member(binary(), pos_integer(), binary(), non_neg_integer(), map()) ->
    {ok, map()} | {error, term()}.
add_config_member(Id, Revision, InstanceId, ConfigIndex, Equipment) ->
    gen_server:call(?SERVER, {add_config_member, Id, Revision, InstanceId,
                              ConfigIndex, Equipment}).

-doc "Remove one member from a group.".
-spec remove_member(binary(), pos_integer(), binary()) ->
    {ok, map()} | {error, term()}.
remove_member(Id, Revision, MemberId) ->
    gen_server:call(?SERVER, {remove_member, Id, Revision, MemberId}).

-doc "Queue a persisted Forma calculation for one exact group revision.".
-spec plan_group(pid(), binary(), pos_integer()) ->
    {ok, reference()} | {error, term()}.
plan_group(Client, Id, Revision) when is_pid(Client) ->
    gen_server:call(?SERVER, {plan_group, Client, Id, Revision}).

-doc "Subscribe to build-group mutations.".
-spec subscribe(pid()) -> {ok, reference()}.
subscribe(Client) when is_pid(Client) -> gen_server:call(?SERVER, {subscribe, Client}).

-doc "Remove a build-group subscription.".
-spec unsubscribe(reference()) -> ok.
unsubscribe(Ref) -> gen_server:call(?SERVER, {unsubscribe, Ref}).

init([]) ->
    {Store, StoreError} = case wfcli_build_store:load() of
        {ok, Loaded} -> {Loaded, undefined};
        {error, Reason} ->
            logger:warning("build store load failed: ~p", [Reason]),
            {wfcli_build_store:empty(), Reason}
    end,
    {ok, #{store => Store, store_error => StoreError,
           store_load_error => StoreError, dirty => false,
           persist_timer => undefined, cache => #{}, pending => #{},
           queue => queue:new(), workers => #{}, worker_monitors => #{},
           client_monitors => #{}, catalog_refresh => undefined,
           subscribers => #{}, subscriber_monitors => #{},
           plan_jobs => #{}, plan_keys => #{}, forma_owner => undefined,
           worker_limit => worker_limit()}}.

handle_call({Kind, Client, Request0}, _From, State)
  when Kind =:= submit; Kind =:= submit_cli ->
    Tag = case Kind of submit -> retinue_build; submit_cli -> retinue_daemon end,
    case normalize_request(Request0) of
        {ok, Request} ->
            Ref = make_ref(),
            case cached_reply(Request, State) of
                {ok, Reply} ->
                    Client ! {Tag, Ref, {ok, Reply}},
                    {reply, {ok, Ref}, State};
                miss ->
                    {reply, {ok, Ref}, add_request(Client, Ref, Tag, Request, State)}
            end;
        {error, _Reason} = Error -> {reply, Error, State}
    end;
handle_call(status, _From, State) ->
    Store = maps:get(store, State),
    {reply, #{queued => queue:len(maps:get(queue, State)),
              processing => map_size(maps:get(workers, State)),
              worker_limit => maps:get(worker_limit, State),
              pending => map_size(maps:get(pending, State)),
              cache_entries => map_size(maps:get(cache, State)),
              revisions => map_size(maps:get(revisions, Store)),
              goals => map_size(maps:get(goals, Store)),
              results => map_size(maps:get(results, Store)),
              planning => map_size(maps:get(plan_jobs, State)),
              catalog_refresh => maps:get(catalog_refresh, State) =/= undefined,
              store_error => case maps:get(store_load_error, State) of
                                 undefined -> maps:get(store_error, State);
                                 LoadError -> LoadError
                             end}, State};
handle_call({revision, Source, ExternalId}, _From, State) ->
    Reply = find_revision(Source, ExternalId, latest, maps:get(store, State)),
    {reply, Reply, State};
handle_call({revision, Source, ExternalId, Fingerprint}, _From, State) ->
    Reply = find_revision(Source, ExternalId, Fingerprint, maps:get(store, State)),
    {reply, Reply, State};
handle_call(groups, _From, State = #{store_load_error := Reason})
  when Reason =/= undefined ->
    {reply, {error, {build_store_unavailable, Reason}}, State};
handle_call(groups, _From, State) ->
    Store = maps:get(store, State),
    Groups = maps:values(maps:get(goals, Store)),
    Equipment = equipment_snapshot(Groups),
    Values = [group_summary(Group, Store, Equipment)
              || Group <- Groups],
    Sorted = lists:sort(fun group_before/2, Values),
    {reply, {ok, #{<<"schema">> => 1, <<"groups">> => Sorted,
                   <<"player_revision">> => maps:get(<<"player_revision">>,
                                                      Equipment, 0)}}, maybe_refresh_group_catalog(Groups, State)};
handle_call({group, Id}, _From, State) ->
    Reply = find_group(Id, State),
    Groups = case Reply of {ok, Group} -> [Group]; _ -> [] end,
    {reply, Reply, maybe_refresh_group_catalog(Groups, State)};
handle_call({create_group, Input, Equipment}, _From, State) ->
    Id = new_id(),
    Now = erlang:system_time(millisecond),
    case wfcli_build_group:create(Input, Equipment, Now, Id) of
        {ok, Group} ->
            store_group(created, Group, State);
        {error, _Reason} = Error -> {reply, Error, State}
    end;
handle_call({update_group, Id, Revision, Patch, Equipment}, _From, State) ->
    mutate_group(Id, Revision, updated, State,
                 fun(Group) -> wfcli_build_group:update(
                                 Group, Patch, Equipment,
                                 erlang:system_time(millisecond)) end);
handle_call({delete_group, Id, Revision}, _From, State) ->
    case checked_group(Id, Revision, State) of
        {ok, Group} ->
            Store0 = maps:get(store, State),
            Store = Store0#{goals => maps:remove(Id, maps:get(goals, Store0)),
                            results => maps:remove(Id, maps:get(results, Store0))},
            commit_group(deleted, Group, Store, State);
        {error, _Reason} = Error -> {reply, Error, State}
    end;
handle_call({add_source_member, Id, Revision, Source, ExternalId, Fingerprint},
            _From, State) ->
    case find_revision(Source, ExternalId, Fingerprint, maps:get(store, State)) of
        {ok, SourceRevision} ->
            mutate_group(Id, Revision, member_added, State,
                         fun(Group) -> wfcli_build_group:add_source(
                                         Group, SourceRevision,
                                         erlang:system_time(millisecond)) end);
        {error, _Reason} = Error -> {reply, Error, State}
    end;
handle_call({add_config_member, Id, Revision, InstanceId, ConfigIndex, Equipment},
            _From, State) ->
    mutate_group(Id, Revision, member_added, State,
                 fun(Group) -> wfcli_build_group:add_config(
                                 Group, InstanceId, ConfigIndex, Equipment,
                                 erlang:system_time(millisecond)) end);
handle_call({remove_member, Id, Revision, MemberId}, _From, State) ->
    mutate_group(Id, Revision, member_removed, State,
                 fun(Group) -> wfcli_build_group:remove_member(
                                 Group, MemberId,
                                 erlang:system_time(millisecond)) end);
handle_call({plan_group, Client, Id, Revision}, _From, State) ->
    queue_group_plan(Client, Id, Revision, State);
handle_call({subscribe, Client}, _From, State) when is_pid(Client) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Subscribers = maps:get(subscribers, State, #{}),
    Monitors = maps:get(subscriber_monitors, State, #{}),
    {reply, {ok, Ref},
     State#{subscribers => Subscribers#{Ref => #{client => Client,
                                                   monitor => Monitor}},
            subscriber_monitors => Monitors#{Monitor => Ref}}};
handle_call({unsubscribe, Ref}, _From, State) ->
    {reply, ok, remove_subscriber(Ref, State)};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast(_Message, State) -> {noreply, State}.

handle_info(process_queue, State) ->
    {noreply, schedule(State)};
handle_info({build_result, Key, Token, Reply}, State) ->
    case maps:get(Key, maps:get(workers, State), undefined) of
        #{token := Token, monitor := Monitor} ->
            erlang:demonitor(Monitor, [flush]),
            activity_end(),
            State1 = remove_worker(Key, Monitor, State),
            State2 = complete_request(Key, Reply, State1),
            {noreply, schedule(State2)};
        _ -> {noreply, State}
    end;
handle_info({catalog_result, Token, Reply},
            State = #{catalog_refresh := #{token := Token, monitor := Monitor}}) ->
    erlang:demonitor(Monitor, [flush]),
    activity_end(),
    State1 = State#{catalog_refresh => undefined},
    case Reply of
        {ok, Catalog} ->
            Store0 = maps:get(store, State1),
            Catalogs = maps:get(catalogs, Store0),
            Changed = catalog_identity(Store0) =/= maps:with([schema, version], Catalog),
            Store = Store0#{catalogs => Catalogs#{overframe => Catalog}},
            State2 = mark_dirty(State1#{store => Store, cache => #{},
                                        store_error => undefined}),
            Updated = [Group || Group <- maps:values(maps:get(goals, Store)),
                                Changed, needs_catalog(Group)],
            Published = lists:foldl(fun(Group, Acc) -> publish_group(updated, Group, Acc) end,
                                    State2, Updated),
            {noreply, schedule(release_blocked(Published))};
        {error, Reason} ->
            logger:warning("Overframe catalog refresh failed: ~p", [Reason]),
            {noreply, fail_blocked(Reason, State1#{store_error => Reason})}
    end;
handle_info({retinue_daemon, FormaRef, Reply}, State) ->
    {noreply, complete_group_plan(FormaRef, Reply, State)};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{forma_owner := {_Owner, Monitor}}) ->
    {noreply, fail_plans({forma_service_unavailable, Reason}, State)};
handle_info({'DOWN', Monitor, process, _Pid, Reason}, State) ->
    case maps:take(Monitor, maps:get(subscriber_monitors, State, #{})) of
        {Ref, SubscriberMonitors} ->
            {noreply, State#{subscriber_monitors => SubscriberMonitors,
                              subscribers => maps:remove(
                                               Ref,
                                               maps:get(subscribers, State, #{}))}};
        error ->
            case maps:take(Monitor, maps:get(client_monitors, State)) of
                {{Key, Ref}, ClientMonitors} ->
                    {noreply, schedule(cancel_waiter(
                                          Key, Ref,
                                          State#{client_monitors => ClientMonitors}))};
                error ->
                    case maps:take(Monitor, maps:get(worker_monitors, State)) of
                        {Key, WorkerMonitors} ->
                            activity_end(),
                            State1 = remove_worker(
                                       Key, Monitor,
                                       State#{worker_monitors => WorkerMonitors}),
                            State2 = complete_request(
                                       Key, {error, {build_worker_down, Reason}},
                                       State1),
                            {noreply, schedule(State2)};
                        error -> catalog_down(Monitor, Reason, State)
                    end
            end
    end;
handle_info(persist_store, State) ->
    case maps:get(dirty, State) of
        false -> {noreply, State#{persist_timer => undefined}};
        true ->
            case wfcli_build_store:save_cache(maps:get(store, State)) of
                ok ->
                    {noreply, State#{dirty => false, persist_timer => undefined,
                                      store_error => undefined}};
                {error, Reason} ->
                    logger:error("build store save failed: ~p", [Reason]),
                    Timer = erlang:send_after(5000, self(), persist_store),
                    {noreply, State#{persist_timer => Timer, store_error => Reason}}
            end
    end;
handle_info(_Message, State) -> {noreply, State}.

terminate(_Reason, State) ->
    maps:foreach(fun(_Key, Worker) -> stop_worker(Worker) end,
                 maps:get(workers, State, #{})),
    case maps:get(catalog_refresh, State, undefined) of
        undefined -> ok;
        Worker -> stop_worker(Worker)
    end,
    case maps:get(dirty, State, false) of
        true -> _ = wfcli_build_store:save_cache(maps:get(store, State));
        false -> ok
    end,
    ok.

code_change(_OldVsn, State, _Extra) ->
    LoadError = case wfcli_build_store:load() of
        {ok, _} -> undefined;
        {error, Reason} -> Reason
    end,
    case persist_legacy_groups(State, LoadError) of
        ok -> migrate_state(State, LoadError);
        Error -> Error
    end.

persist_legacy_groups(#{store_load_error := _}, _) -> ok;
persist_legacy_groups(#{dirty := true, store := Store}, undefined) ->
    wfcli_build_store:save(Store);
persist_legacy_groups(_, _) -> ok.

migrate_state(State, LoadError) ->
    {ok, State#{store => maps:get(store, State, wfcli_build_store:empty()),
                store_error => maps:get(store_error, State, undefined),
                store_load_error => maps:get(store_load_error, State, LoadError),
                dirty => maps:get(dirty, State, false),
                persist_timer => maps:get(persist_timer, State, undefined),
                cache => #{},
                pending => maps:get(pending, State, #{}),
                queue => maps:get(queue, State, queue:new()),
                workers => maps:get(workers, State, #{}),
                worker_monitors => maps:get(worker_monitors, State, #{}),
                client_monitors => maps:get(client_monitors, State, #{}),
                subscribers => maps:get(subscribers, State, #{}),
                subscriber_monitors => maps:get(subscriber_monitors, State, #{}),
                plan_jobs => maps:get(plan_jobs, State, #{}),
                plan_keys => maps:get(plan_keys, State, #{}),
                forma_owner => maps:get(forma_owner, State, undefined),
                catalog_refresh => maps:get(catalog_refresh, State, undefined),
                worker_limit => maps:get(worker_limit, State, worker_limit())}}.

normalize_request(#{source := build_stats, options := Options}) ->
    case wfcli_build_stats:validate(Options) of
        ok -> {ok, #{source => build_stats, action => stats, options => Options}};
        Error -> Error
    end;
normalize_request(#{source := overframe, action := search} = Request) ->
    Query = maps:get(query, Request, <<>>),
    Class = maps:get(class, Request, <<"all">>),
    Limit = maps:get(limit, Request, 40),
    case is_binary(Query) andalso byte_size(Query) =< 200 andalso
         is_binary(Class) andalso is_integer(Limit) andalso
         Limit >= 1 andalso Limit =< 100 of
        true -> {ok, #{source => overframe, action => search, query => Query,
                       class => Class, limit => Limit}};
        false -> {error, invalid_build_search}
    end;
normalize_request(#{source := overframe, action := list} = Request) ->
    {ok, maps:with([source, action, item, query, scope, ordering,
                    limit, offset, refresh], Request)};
normalize_request(#{source := overframe, action := detail} = Request) ->
    {ok, maps:with([source, action, id, refresh], Request)};
normalize_request(_Request) -> {error, unsupported_build_request}.

add_request(Client, Ref, Tag, Request, State) ->
    Key = request_key(Request),
    Monitor = erlang:monitor(process, Client),
    Waiter = #{client => Client, monitor => Monitor, reply_tag => Tag},
    ClientMonitors = maps:get(client_monitors, State),
    State1 = State#{client_monitors =>
                        ClientMonitors#{Monitor => {Key, Ref}}},
    case maps:get(Key, maps:get(pending, State1), undefined) of
        Pending when is_map(Pending) ->
            Waiters = maps:get(waiters, Pending),
            put_pending(Key, Pending#{waiters => Waiters#{Ref => Waiter}}, State1);
        undefined ->
            Pending0 = #{request => Request, waiters => #{Ref => Waiter}},
            case {maps:get(action, Request), catalog(State1)} of
                {stats, _} ->
                    enqueue(Key, put_pending(Key, Pending0#{phase => queued}, State1));
                {_, undefined} ->
                    State2 = put_pending(Key, Pending0#{phase => blocked}, State1),
                    ensure_catalog_refresh(State2);
                _Catalog ->
                    State2 = enqueue(Key,
                                     put_pending(Key, Pending0#{phase => queued}, State1)),
                    ensure_fresh_catalog(State2)
            end
    end.

schedule(State) ->
    Slots = maps:get(worker_limit, State) - map_size(maps:get(workers, State)),
    schedule(Slots, State).

schedule(Slots, State) when Slots =< 0 -> State;
schedule(Slots, State) ->
    case queue:out(maps:get(queue, State)) of
        {empty, Queue} -> State#{queue => Queue};
        {{value, Key}, Queue} ->
            State1 = State#{queue => Queue},
            case maps:get(Key, maps:get(pending, State1), undefined) of
                #{phase := queued, request := Request} = Pending ->
                    Catalog = catalog(State1),
                    Parent = self(),
                    Token = make_ref(),
                    {Pid, Monitor} = spawn_monitor(fun() ->
                        Reply = try run_request(Request, Catalog)
                                catch Class:Reason:Stack ->
                                    logger:error("build request failed: ~p:~p~n~p",
                                                 [Class, Reason, Stack]),
                                    {error, {build_request_crash, Class, Reason}}
                                end,
                        Parent ! {build_result, Key, Token, Reply}
                    end),
                    activity_start(),
                    Worker = #{pid => Pid, monitor => Monitor, token => Token},
                    Workers = maps:get(workers, State1),
                    WorkerMonitors = maps:get(worker_monitors, State1),
                    State2 = put_pending(
                               Key, Pending#{phase => running},
                               State1#{workers => Workers#{Key => Worker},
                                       worker_monitors =>
                                           WorkerMonitors#{Monitor => Key}}),
                    schedule(Slots - 1, State2);
                _ -> schedule(Slots, State1)
            end
    end.

run_request(#{action := stats, options := Options} = Request, _Catalog) ->
    source_call(Request, undefined, fun() -> wfcli_build_stats:run(Options) end);
run_request(#{action := search, query := Query, class := Class, limit := Limit},
            Catalog) ->
    source_call(#{action => search, query => Query, class => Class, limit => Limit},
                Catalog,
                fun() -> wfcli_overframe_source:search(Catalog, Query, Class, Limit) end);
run_request(#{action := list} = Request, Catalog) ->
    source_call(Request, Catalog,
                fun() -> wfcli_overframe_source:list(Catalog, Request) end);
run_request(#{action := detail} = Request, Catalog) ->
    source_call(Request, Catalog,
                fun() -> wfcli_overframe_source:detail(Catalog, Request) end).

source_call(Request, Catalog, Default) ->
    case application:get_env(retinue_daemon, build_source_fun) of
        {ok, Fun} when is_function(Fun, 2) -> Fun(Request, Catalog);
        _ -> Default()
    end.

complete_request(Key, Reply0, State) ->
    case maps:take(Key, maps:get(pending, State)) of
        error -> State;
        {Pending, PendingMap} ->
            {Reply, State1} = retain_reply(maps:get(request, Pending), Reply0,
                                           State#{pending => PendingMap}),
            maps:foreach(
              fun(Ref, #{client := Client, monitor := Monitor} = Waiter) ->
                  Client ! {maps:get(reply_tag, Waiter, retinue_build), Ref, Reply},
                  erlang:demonitor(Monitor, [flush])
              end, maps:get(waiters, Pending)),
            ClientMonitors = lists:foldl(
              fun(#{monitor := Monitor}, Acc) -> maps:remove(Monitor, Acc) end,
              maps:get(client_monitors, State1),
              maps:values(maps:get(waiters, Pending))),
            State1#{client_monitors => ClientMonitors}
    end.

retain_reply(#{action := stats}, Reply, State) -> {Reply, State};
retain_reply(Request = #{action := detail}, {ok, Revision}, State) ->
    Store0 = maps:get(store, State),
    Identity = maps:get(<<"identity">>, Revision),
    Source = maps:get(<<"source">>, Identity),
    ExternalId = maps:get(<<"external_id">>, Identity),
    Fingerprint = maps:get(<<"fingerprint">>, Revision),
    RevisionKey = {Source, ExternalId, Fingerprint},
    IdentityKey = {Source, ExternalId},
    Revisions = maps:get(revisions, Store0),
    Latest = maps:get(latest, Store0),
    Store = Store0#{revisions => Revisions#{RevisionKey => Revision},
                    latest => Latest#{IdentityKey => Fingerprint}},
    Public = public_revision(Revision, Store),
    State1 = cache_reply(Request, Public, mark_dirty(State#{store => Store})),
    {{ok, Public}, State1};
retain_reply(Request, {ok, Data}, State) ->
    {{ok, Data}, cache_reply(Request, Data, State)};
retain_reply(_Request, {error, _Reason} = Error, State) -> {Error, State};
retain_reply(_Request, Other, State) ->
    {{error, {invalid_build_source_reply, Other}}, State}.

public_revision(Revision, Store) ->
    Presented = case maps:get(<<"identity">>, Revision, #{}) of
        #{<<"source">> := <<"overframe">>} ->
            Catalog = maps:get(overframe, maps:get(catalogs, Store, #{}), #{}),
            wfcli_overframe_source:present_revision(Revision, Catalog);
        _ -> Revision
    end,
    maps:remove(<<"raw">>, Presented).

cached_reply(#{action := stats}, _State) -> miss;
cached_reply(Request, State) ->
    case maps:get(refresh, Request, false) of
        true -> miss;
        false ->
            Key = request_key(Request),
            Now = erlang:monotonic_time(millisecond),
            case maps:get(Key, maps:get(cache, State), undefined) of
                #{expires := Expires, data := Data} when Expires > Now ->
                    {ok, Data};
                _ -> miss
            end
    end.

cache_reply(Request, Data, State) ->
    Expires = erlang:monotonic_time(millisecond) + cache_ttl(Request),
    Cache = maps:get(cache, State),
    State#{cache => Cache#{request_key(Request) =>
                               #{expires => Expires, data => Data}}}.

cache_ttl(#{action := search}) -> 3600000;
cache_ttl(#{action := list}) -> 300000;
cache_ttl(#{action := detail}) -> 900000.

request_key(#{source := overframe} = Request) ->
    {overframe, wfcli_overframe_source:module_info(md5),
     maps:remove(refresh, Request)};
request_key(#{action := stats}) -> make_ref();
request_key(Request) -> maps:remove(refresh, Request).

ensure_catalog_refresh(State = #{catalog_refresh := Refresh})
  when Refresh =/= undefined -> State;
ensure_catalog_refresh(State) ->
    Parent = self(),
    Token = make_ref(),
    {Pid, Monitor} = spawn_monitor(fun() ->
        Reply = try refresh_catalog()
                catch Class:Reason:Stack ->
                    logger:error("Overframe catalog request failed: ~p:~p~n~p",
                                 [Class, Reason, Stack]),
                    {error, {catalog_crash, Class, Reason}}
                end,
        Parent ! {catalog_result, Token, Reply}
    end),
    activity_start(),
    State#{catalog_refresh => #{pid => Pid, monitor => Monitor, token => Token}}.

refresh_catalog() ->
    case application:get_env(retinue_daemon, build_catalog_fun) of
        {ok, Fun} when is_function(Fun, 0) -> Fun();
        _ -> wfcli_overframe_source:refresh_catalog()
    end.

ensure_fresh_catalog(State) ->
    case catalog_stale(catalog(State)) of
        true -> ensure_catalog_refresh(State);
        false -> State
    end.

catalog(State) ->
    ExpectedSchema = wfcli_overframe_source:catalog_schema(),
    case maps:get(overframe, maps:get(catalogs, maps:get(store, State)), undefined) of
        Catalog = #{schema := ExpectedSchema} -> Catalog;
        _ -> undefined
    end.

catalog_stale(undefined) -> true;
catalog_stale(Catalog) ->
    FetchedAt = maps:get(fetched_at, Catalog, 0),
    erlang:system_time(millisecond) - FetchedAt > catalog_ttl().

release_blocked(State) ->
    {Pending, Queue} = maps:fold(
      fun(Key, Entry = #{phase := blocked}, {PendingAcc, QueueAcc}) ->
              {PendingAcc#{Key => Entry#{phase => queued}}, queue:in(Key, QueueAcc)};
         (Key, Entry, {PendingAcc, QueueAcc}) ->
              {PendingAcc#{Key => Entry}, QueueAcc}
      end, {#{}, maps:get(queue, State)}, maps:get(pending, State)),
    State#{pending => Pending, queue => Queue}.

fail_blocked(Reason, State) ->
    Keys = [Key || {Key, #{phase := blocked}} <- maps:to_list(maps:get(pending, State))],
    lists:foldl(fun(Key, Acc) ->
                    complete_request(Key, {error, {catalog_unavailable, Reason}}, Acc)
                end, State, Keys).

catalog_down(Monitor, Reason,
             State = #{catalog_refresh := #{monitor := Monitor}}) ->
    activity_end(),
    logger:warning("Overframe catalog worker stopped: ~p", [Reason]),
    State1 = State#{catalog_refresh => undefined, store_error => Reason},
    {noreply, fail_blocked({catalog_worker_down, Reason}, State1)};
catalog_down(_Monitor, _Reason, State) -> {noreply, State}.

cancel_waiter(Key, Ref, State) ->
    case maps:get(Key, maps:get(pending, State), undefined) of
        undefined -> State;
        Pending ->
            Waiters = maps:remove(Ref, maps:get(waiters, Pending)),
            case map_size(Waiters) of
                N when N > 0 -> put_pending(Key, Pending#{waiters => Waiters}, State);
                0 -> cancel_empty_request(Key, Pending, State)
            end
    end.

cancel_empty_request(Key, #{phase := running}, State) ->
    case maps:get(Key, maps:get(workers, State), undefined) of
        undefined -> remove_pending(Key, State);
        Worker = #{monitor := Monitor} ->
            stop_worker(Worker),
            activity_end(),
            remove_pending(Key, remove_worker(Key, Monitor, State))
    end;
cancel_empty_request(Key, #{phase := queued}, State) ->
    Queue = queue:filter(fun(QueuedKey) -> QueuedKey =/= Key end,
                         maps:get(queue, State)),
    remove_pending(Key, State#{queue => Queue});
cancel_empty_request(Key, _Pending, State) -> remove_pending(Key, State).

remove_worker(Key, Monitor, State) ->
    State#{workers => maps:remove(Key, maps:get(workers, State)),
           worker_monitors => maps:remove(Monitor, maps:get(worker_monitors, State))}.

remove_pending(Key, State) ->
    State#{pending => maps:remove(Key, maps:get(pending, State))}.

put_pending(Key, Pending, State) ->
    State#{pending => (maps:get(pending, State))#{Key => Pending}}.

enqueue(Key, State) ->
    self() ! process_queue,
    State#{queue => queue:in(Key, maps:get(queue, State))}.

mark_dirty(State = #{persist_timer := undefined}) ->
    Timer = erlang:send_after(?PERSIST_DELAY_MS, self(), persist_store),
    State#{dirty => true, persist_timer => Timer};
mark_dirty(State) -> State#{dirty => true}.

find_revision(Source, ExternalId, latest, Store) ->
    case maps:get({Source, ExternalId}, maps:get(latest, Store), undefined) of
        undefined -> {error, build_revision_not_found};
        Fingerprint -> find_revision(Source, ExternalId, Fingerprint, Store)
    end;
find_revision(Source, ExternalId, Fingerprint, Store) when is_binary(Fingerprint) ->
    case stored_revision(Source, ExternalId, Fingerprint, Store) of
        undefined -> {error, build_revision_not_found};
        Revision -> {ok, public_revision(Revision, Store)}
    end;
find_revision(_Source, _ExternalId, _Fingerprint, _Store) ->
    {error, invalid_build_revision}.

find_group(_Id, #{store_load_error := Reason}) when Reason =/= undefined ->
    {error, {build_store_unavailable, Reason}};
find_group(Id, State) when is_binary(Id) ->
    Store = maps:get(store, State),
    case maps:get(Id, maps:get(goals, Store), undefined) of
        undefined -> {error, build_group_not_found};
        Group -> {ok, public_group(Group, Store)}
    end;
find_group(_Id, _State) -> {error, invalid_build_group_id}.

checked_group(_Id, _Revision, #{store_load_error := Reason})
  when Reason =/= undefined ->
    {error, {build_store_unavailable, Reason}};
checked_group(Id, Revision, State)
  when is_binary(Id), is_integer(Revision), Revision > 0 ->
    case maps:get(Id, maps:get(goals, maps:get(store, State)), undefined) of
        undefined -> {error, build_group_not_found};
        #{<<"revision">> := Revision} = Group -> {ok, Group};
        Group -> {error, {build_group_conflict,
                          maps:get(<<"revision">>, Group, 0)}}
    end;
checked_group(_Id, _Revision, _State) -> {error, invalid_build_group_revision}.

mutate_group(Id, Revision, Event, State, Fun) ->
    case checked_group(Id, Revision, State) of
        {ok, Before} ->
            case Fun(Before) of
                {ok, Before} ->
                    {reply, {ok, public_group(Before, maps:get(store, State))}, State};
                {ok, After} ->
                    store_group(Event, After, State);
                {error, _Reason} = Error -> {reply, Error, State}
            end;
        {error, _Reason} = Error -> {reply, Error, State}
    end.

store_group(Event, Group, State) ->
    Store0 = maps:get(store, State),
    Goals = maps:get(goals, Store0),
    Id = maps:get(<<"id">>, Group),
    Store = Store0#{goals => Goals#{Id => Group},
                    results => maps:remove(Id, maps:get(results, Store0))},
    commit_group(Event, Group, Store, State).

commit_group(_Event, _Group, _Store, State = #{store_load_error := Reason})
  when Reason =/= undefined ->
    {reply, {error, {build_store_unavailable, Reason}}, State};
commit_group(Event, Group, Store, State) ->
    case wfcli_build_store:save(Store) of
        ok ->
            State1 = publish_group(Event, Group, mark_dirty(State#{store => Store})),
            Reply = case Event of
                deleted -> ok;
                _ -> {ok, public_group(Group, Store)}
            end,
            {reply, Reply, State1};
        {error, Reason} ->
            {reply, {error, {build_store_save_failed, Reason}}, State}
    end.

publish_group(Event, Group, State) ->
    Public = public_group(Group, maps:get(store, State)),
    maps:foreach(
      fun(Ref, #{client := Client}) ->
          Client ! {wfcli_build_group, Ref, Event, Public}
      end, maps:get(subscribers, State, #{})),
    State.

remove_subscriber(Ref, State) ->
    case maps:take(Ref, maps:get(subscribers, State, #{})) of
        error -> State;
        {#{monitor := Monitor}, Subscribers} ->
            erlang:demonitor(Monitor, [flush]),
            State#{subscribers => Subscribers,
                   subscriber_monitors => maps:remove(
                                            Monitor,
                                            maps:get(subscriber_monitors,
                                                     State, #{}))}
    end.

group_summary(Group, Store, Equipment) ->
    Public = public_group(Group, Store, Equipment),
    Members = [maps:remove(<<"snapshot">>, Member)
               || Member <- maps:get(<<"members">>, Public, [])],
    Public#{<<"members">> => Members}.

public_group(Group, Store) ->
    public_group(Group, Store, equipment_snapshot([Group])).

public_group(Group, Store, Equipment) ->
    Current = wfcli_build_group:refresh_target(Group, Equipment),
    Public = (wfcli_build_group:public(hydrate_group(Current, Store)))#{
                 <<"player_revision">> => maps:get(<<"player_revision">>, Equipment, 0)},
    case current_result(Current, Store) of
        undefined -> Public#{<<"plan_result">> => null};
        Result -> Public#{<<"plan_result">> => Result}
    end.

-ifdef(TEST).
equipment_snapshot(Groups) ->
    case application:get_env(retinue_daemon, build_equipment_fun) of
        {ok, Fun} -> Fun();
        _ -> current_equipment(Groups)
    end.
-else.
equipment_snapshot(Groups) -> current_equipment(Groups).
-endif.

current_equipment(Groups) ->
    InstanceIds = [Id || #{<<"instance_id">> := Id} <- Groups, is_binary(Id)],
    {ok, Equipment} = wfcli_build_equipment:snapshot(InstanceIds),
    Equipment.

maybe_refresh_group_catalog(Groups, State) ->
    case lists:any(fun needs_catalog/1, Groups) of
        true -> ensure_fresh_catalog(State);
        false -> State
    end.

needs_catalog(Group) -> maps:get(<<"members">>, Group, []) =/= [].

catalog_identity(Store) ->
    maps:with([schema, version], maps:get(overframe, maps:get(catalogs, Store), #{})).

plan_catalog(Group, Store) ->
    case needs_catalog(Group) of
        true -> #{atom_to_binary(Key) => Value || Key := Value <- catalog_identity(Store)};
        false -> null
    end.

target_fingerprint(#{<<"baseline">> := #{<<"fingerprint">> := Fingerprint}}) ->
    Fingerprint;
target_fingerprint(_) -> null.

public_member(Member = #{<<"kind">> := <<"source_revision">>,
                         <<"source">> := Source,
                         <<"external_id">> := ExternalId,
                         <<"fingerprint">> := Fingerprint}, Store) ->
    Revision = case stored_revision(Source, ExternalId, Fingerprint, Store) of
        undefined -> maps:get(<<"snapshot">>, Member, #{});
        StoredRevision -> StoredRevision
    end,
    Member#{<<"snapshot">> => public_revision(Revision, Store)};
public_member(Member = #{<<"kind">> := <<"player_config">>,
                         <<"snapshot">> := Snapshot = #{<<"config">> := Config}}, Store) ->
    Catalog = maps:get(overframe, maps:get(catalogs, Store, #{}), #{}),
    Upgrades = [Upgrade#{<<"elemental_types">> => wfcli_overframe_source:mod_elements(
                         Catalog, maps:get(<<"item_type">>, Upgrade, null))}
                || Upgrade <- maps:get(<<"upgrade_slots">>, Config, [])],
    Member#{<<"snapshot">> => Snapshot#{<<"config">> => Config#{<<"upgrade_slots">> => Upgrades}}};
public_member(Member, _Store) -> Member.

hydrate_group(Group, Store) ->
    Members = [public_member(Member, Store)
               || Member <- maps:get(<<"members">>, Group, [])],
    Group#{<<"members">> => Members}.

stored_revision(Source, ExternalId, Fingerprint, Store) ->
    maps:get({Source, ExternalId, Fingerprint}, maps:get(revisions, Store, #{}),
             undefined).

current_result(Group, Store) ->
    Id = maps:get(<<"id">>, Group),
    Revision = maps:get(<<"revision">>, Group),
    Fingerprint = target_fingerprint(Group),
    Catalog = plan_catalog(Group, Store),
    case maps:get(Id, maps:get(results, Store, #{}), undefined) of
        #{<<"schema">> := 2, <<"group_revision">> := Revision,
          <<"catalog">> := Catalog,
          <<"target_fingerprint">> := Fingerprint} = Result when Fingerprint =/= null ->
            Result;
        _ -> undefined
    end.

queue_group_plan(Client, Id, Revision, State) ->
    case checked_group(Id, Revision, State) of
        {ok, Group} ->
            Current = wfcli_build_group:refresh_target(Group, equipment_snapshot([Group])),
            queue_current_group_plan(Client, Current, State);
        {error, _Reason} = Error -> {reply, Error, State}
    end.

queue_current_group_plan(_Client, #{<<"target_status">> := <<"missing">>}, State) ->
    {reply, {error, build_instance_not_found}, State};
queue_current_group_plan(Client, Group, State) ->
    PlanningGroup = hydrate_group(Group, maps:get(store, State)),
    case wfcli_build_plan:request(PlanningGroup) of
        {ok, Request} ->
            State0 = ensure_forma_owner(State),
            RequestRef = make_ref(),
            Key = {maps:get(<<"id">>, Group), maps:get(<<"revision">>, Group),
                   target_fingerprint(Group), plan_catalog(Group, maps:get(store, State0))},
            case maps:get(Key, maps:get(plan_keys, State0), undefined) of
                FormaRef when is_reference(FormaRef) ->
                    Jobs = maps:get(plan_jobs, State0),
                    Job = maps:get(FormaRef, Jobs),
                    Waiters = maps:get(waiters, Job),
                    {reply, {ok, RequestRef},
                     State0#{plan_jobs => Jobs#{FormaRef => Job#{
                                 waiters => Waiters#{RequestRef => Client}}}}};
                undefined ->
                    submit_group_plan(Client, RequestRef, Key, PlanningGroup,
                                      Request, State0)
            end;
        {error, _Reason} = Error -> {reply, Error, State}
    end.

ensure_forma_owner(State) ->
    Pid = whereis(wfcli_forma_service),
    case maps:get(forma_owner, State) of
        {Pid, _} -> State;
        Previous ->
            case Previous of
                {_, Monitor} -> erlang:demonitor(Monitor, [flush]);
                undefined -> ok
            end,
            State0 = fail_plans(forma_service_restarted, State),
            case Pid of
                undefined -> State0;
                _ -> State0#{forma_owner => {Pid, erlang:monitor(process, Pid)}}
            end
    end.

fail_plans(Reason, State) ->
    maps:foreach(fun(_, Job) ->
                     notify_plan_waiters(maps:get(waiters, Job), {error, Reason})
                 end, maps:get(plan_jobs, State)),
    State#{plan_jobs => #{}, plan_keys => #{}, forma_owner => undefined}.

submit_group_plan(Client, RequestRef, Key, Group, Request, State) ->
    try wfcli_forma_service:submit(self(), Request) of
        {ok, FormaRef} ->
            Job = #{key => Key, group => Group,
                    catalog => plan_catalog(Group, maps:get(store, State)),
                    waiters => #{RequestRef => Client}},
            {reply, {ok, RequestRef},
             State#{plan_jobs => (maps:get(plan_jobs, State))#{FormaRef => Job},
                    plan_keys => (maps:get(plan_keys, State))#{Key => FormaRef}}};
        {error, _Reason} = Error -> {reply, Error, State}
    catch exit:Reason -> {reply, {error, {forma_service_unavailable, Reason}}, State}
    end.

complete_group_plan(FormaRef, Reply, State) ->
    case maps:take(FormaRef, maps:get(plan_jobs, State)) of
        error -> State;
        {Job, Jobs} ->
            Key = maps:get(key, Job),
            State0 = State#{plan_jobs => Jobs,
                            plan_keys => maps:remove(Key,
                                                    maps:get(plan_keys, State))},
            Group = maps:get(group, Job),
            Id = maps:get(<<"id">>, Group),
            Revision = maps:get(<<"revision">>, Group),
            case checked_plan_target(Id, Revision, Job, State0) of
                {ok, Current} ->
                    case wfcli_build_plan:result(Group, Reply) of
                        {ok, Result0} ->
                            Result = Result0#{<<"catalog">> => maps:get(catalog, Job)},
                            Store0 = maps:get(store, State0),
                            Results = maps:get(results, Store0),
                            State1 = mark_dirty(
                                       State0#{store => Store0#{results =>
                                                                    Results#{Id =>
                                                                                 Result}}}),
                            State2 = publish_group(planned, Current, State1),
                            notify_plan_waiters(maps:get(waiters, Job),
                                                {ok, Result}),
                            State2;
                        {error, _Reason} = Error ->
                            notify_plan_waiters(maps:get(waiters, Job), Error),
                            State0
                    end;
                {error, _Reason} = Error ->
                    notify_plan_waiters(maps:get(waiters, Job), Error),
                    State0
            end
    end.

checked_plan_target(Id, Revision, #{group := Planned, catalog := Catalog}, State) ->
    case checked_group(Id, Revision, State) of
        {ok, Group} ->
            Current = wfcli_build_group:refresh_target(Group, equipment_snapshot([Group])),
            case {target_fingerprint(Current) =:= target_fingerprint(Planned),
                  plan_catalog(Current, maps:get(store, State)) =:= Catalog} of
                {true, true} -> {ok, Current};
                {false, _} -> {error, build_target_changed};
                {_, false} -> {error, build_catalog_changed}
            end;
        Error -> Error
    end.

notify_plan_waiters(Waiters, Reply) ->
    maps:foreach(fun(Ref, Client) -> Client ! {retinue_build, Ref, Reply} end,
                 Waiters).

group_before(A, B) ->
    UpdatedA = maps:get(<<"updated_at">>, A, 0),
    UpdatedB = maps:get(<<"updated_at">>, B, 0),
    NameA = maps:get(<<"name">>, A, <<>>),
    NameB = maps:get(<<"name">>, B, <<>>),
    IdA = maps:get(<<"id">>, A, <<>>),
    IdB = maps:get(<<"id">>, B, <<>>),
    UpdatedA > UpdatedB orelse
        (UpdatedA =:= UpdatedB andalso
             (NameA < NameB orelse (NameA =:= NameB andalso IdA < IdB))).

new_id() ->
    <<"group-", (binary:encode_hex(crypto:strong_rand_bytes(16), lowercase))/binary>>.

stop_worker(#{pid := Pid, monitor := Monitor}) ->
    exit(Pid, kill),
    erlang:demonitor(Monitor, [flush]),
    ok.

worker_limit() ->
    case application:get_env(retinue_daemon, build_source_workers, ?DEFAULT_WORKERS) of
        Count when is_integer(Count), Count > 0, Count =< 32 -> Count;
        _ -> ?DEFAULT_WORKERS
    end.

catalog_ttl() ->
    case application:get_env(retinue_daemon, build_catalog_ttl_ms,
                             ?DEFAULT_CATALOG_TTL_MS) of
        Ttl when is_integer(Ttl), Ttl > 0 -> Ttl;
        _ -> ?DEFAULT_CATALOG_TTL_MS
    end.

activity_start() ->
    try retinue_activity:activity_start()
    catch _:_ -> ok
    end,
    ok.

activity_end() ->
    try retinue_activity:activity_end()
    catch _:_ -> ok
    end,
    ok.

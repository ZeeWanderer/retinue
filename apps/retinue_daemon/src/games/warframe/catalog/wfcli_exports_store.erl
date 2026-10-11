%%%-------------------------------------------------------------------
%% Persistent searchable export/knowledge catalog and concurrent query workers.
%%%-------------------------------------------------------------------
-module(wfcli_exports_store).

-behaviour(gen_server).

-include_lib("kernel/include/file.hrl").

-export([start_link/0, submit/2, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(CACHE_VERSION, 4).
-define(DEFAULT_WORKERS, 8).

-type state() :: map().

-doc "Start daemon-owned searchable export and knowledge cache.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Start one export query; reply arrives as `{retinue_daemon, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Return export worker and searchable catalog cache counts.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-spec init([]) -> {ok, state()}.
init([]) ->
    {ok, #{queue => queue:new(), workers => #{},
           monitors => #{}, cache => #{}, max_workers => worker_limit()}}.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call({submit, Client, Request}, _From, State) when is_pid(Client), is_map(Request) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Item = #{ref => Ref, client => Client, monitor => Monitor, request => Request},
    Monitors = maps:get(monitors, State),
    retinue_activity:activity_start(),
    State1 = State#{monitors => Monitors#{Monitor => Ref}},
    {reply, {ok, Ref}, enqueue_item(Item, State1)};
handle_call(status, _From, State) ->
    {reply, #{queued => queued_requests(maps:get(queue, State)),
              processing => map_size(maps:get(workers, State, #{})) > 0,
              active => map_size(maps:get(workers, State, #{})),
              cached_datasets => map_size(maps:get(cache, State)),
              cached_catalogs => map_size(maps:get(cache, State))}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast(_Message, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info(process_queue, State) ->
    {noreply, start_queued(State)};
handle_info({catalog_result, Token, Reply, CacheChanges}, State) ->
    case maps:take(Token, maps:get(workers, State, #{})) of
        error -> {noreply, State};
        {#{worker_monitor := WorkerMonitor, item := Item}, Workers} ->
            erlang:demonitor(WorkerMonitor, [flush]),
            MergedCache = apply_cache_changes(CacheChanges, maps:get(cache, State)),
            {noreply, start_queued(
                        complete_group(
                          Item, Reply, State#{workers => Workers, cache => MergedCache}))}
    end;
handle_info({'DOWN', Monitor, process, _Pid, Reason}, State) ->
    case take_worker_by_monitor(Monitor, maps:get(workers, State, #{})) of
        {ok, Item, Workers} ->
            {noreply, start_queued(
                        complete_group(Item, {error, {catalog_worker_down, Reason}},
                                      State#{workers => Workers}))};
        error ->
            case cancel_client(Monitor, State) of
                {not_found, State1} -> {noreply, State1};
                {canceled, State1} -> {noreply, start_queued(State1)}
            end
    end;
handle_info(_Message, State) ->
    {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, State) ->
    maps:foreach(fun(_Token, Worker) -> stop_worker(Worker) end,
                 maps:get(workers, State, #{})),
    ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    Cache = maps:get(cache, State, #{}),
    Current = maps:filter(fun(Key, _Value) -> current_cache_key(Key) end, Cache),
    Workers0 = migrate_current(maps:get(current, State, undefined),
                               maps:get(workers, State, #{})),
    Workers = maps:map(fun(_Token, Worker) -> normalize_worker(Worker) end, Workers0),
    Queue = queue:from_list([normalize_group(Item)
                             || Item <- queue:to_list(maps:get(queue, State, queue:new()))]),
    State1 = maps:without([processing, current], State),
    self() ! process_queue,
    {ok, State1#{cache => Current,
                 queue => Queue,
                 workers => Workers,
                 monitors => maps:get(monitors, State1, #{}),
                 max_workers => maps:get(max_workers, State1, worker_limit())}}.

current_cache_key(Key) when is_tuple(Key) ->
    case tuple_to_list(Key) of
        [?CACHE_VERSION, entities | _] -> true;
        _ -> false
    end;
current_cache_key(_Key) -> false.

start_item(Item, State) ->
    Parent = self(),
    Token = make_ref(),
    Cache = maps:get(cache, State),
    {Pid, WorkerMonitor} = spawn_monitor(fun() ->
        {Reply, ExecuteState} = safe_execute(maps:get(request, Item), #{cache => Cache}),
        Changes = cache_changes(Cache, maps:get(cache, ExecuteState)),
        Parent ! {catalog_result, Token, Reply, Changes}
    end),
    Worker = #{worker_pid => Pid, worker_monitor => WorkerMonitor, item => Item},
    State#{workers => (maps:get(workers, State, #{}))#{Token => Worker}}.

enqueue_item(Item, State) ->
    Request = maps:get(request, Item),
    case add_to_matching_worker(Request, Item, maps:get(workers, State)) of
        {ok, Workers} -> State#{workers => Workers};
        error ->
            case add_to_matching_queue(Request, Item, maps:get(queue, State)) of
                {ok, Queue} -> State#{queue => Queue};
                error ->
                    case map_size(maps:get(workers, State)) < maps:get(max_workers, State) of
                        true -> start_item(normalize_group(Item), State);
                        false -> State#{queue => queue:in(normalize_group(Item),
                                                         maps:get(queue, State))}
                    end
            end
    end.

add_to_matching_worker(Request, Item, Workers) ->
    case [{Token, Worker} || {Token, Worker = #{item := Existing}} <- maps:to_list(Workers),
                             maps:get(request, Existing) =:= Request] of
        [{Token, Worker = #{item := Existing}} | _] ->
            {ok, Workers#{Token => Worker#{item => add_follower(Existing, Item)}}};
        [] -> error
    end.

add_to_matching_queue(Request, Item, Queue) ->
    add_to_matching_queue(Request, Item, queue:to_list(Queue), []).

add_to_matching_queue(_Request, _Item, [], _Acc) -> error;
add_to_matching_queue(Request, Item, [Existing | Rest], Acc) ->
    case maps:get(request, Existing) =:= Request of
        true ->
            {ok, queue:from_list(lists:reverse(Acc) ++
                                 [add_follower(Existing, Item) | Rest])};
        false -> add_to_matching_queue(Request, Item, Rest, [Existing | Acc])
    end.

add_follower(Item, Follower) ->
    Item#{followers => maps:get(followers, Item, []) ++ [maps:remove(followers, Follower)]}.

normalize_group(Item) -> Item#{followers => maps:get(followers, Item, [])}.

normalize_worker(Worker = #{item := Item}) -> Worker#{item => normalize_group(Item)}.

migrate_current(undefined, Workers) -> Workers;
migrate_current(Current = #{token := Token}, Workers) ->
    Workers#{Token => maps:remove(token, Current)};
migrate_current(_Current, Workers) -> Workers.

queued_requests(Queue) ->
    lists:sum([1 + length(maps:get(followers, Item, [])) || Item <- queue:to_list(Queue)]).

cache_changes(Before, After) ->
    maps:fold(
      fun(Key, Value, Acc) ->
          case maps:find(Key, Before) of
              {ok, Value} -> Acc;
              Previous -> [{Key, Previous, Value} | Acc]
          end
      end,
      [], After).

apply_cache_changes(Changes, Cache) ->
    lists:foldl(
      fun({Key, Previous, Value}, Acc) ->
          case {Previous, maps:find(Key, Acc)} of
              {error, error} -> Acc#{Key => Value};
              {{ok, Expected}, {ok, Expected}} -> Acc#{Key => Value};
              _ -> Acc
          end
      end,
      Cache, Changes).

start_queued(State) ->
    case map_size(maps:get(workers, State)) < maps:get(max_workers, State) of
        false -> State;
        true ->
            case queue:out(maps:get(queue, State)) of
                {empty, Queue} -> State#{queue => Queue};
                {{value, Item}, Queue} -> start_queued(start_item(Item, State#{queue => Queue}))
            end
    end.

worker_limit() ->
    case application:get_env(retinue_daemon, catalog_workers, ?DEFAULT_WORKERS) of
        Count when is_integer(Count), Count > 0 -> Count;
        _ -> ?DEFAULT_WORKERS
    end.

complete_group(Item, Reply, State) ->
    lists:foldl(fun(Queued, Acc) -> complete_item(Queued, Reply, Acc) end,
                State, group_items(Item)).

complete_item(Item, Reply, State) ->
    Ref = maps:get(ref, Item),
    Monitor = maps:get(monitor, Item),
    case maps:is_key(Monitor, maps:get(monitors, State)) of
        false -> State;
        true ->
            maps:get(client, Item) ! {retinue_daemon, Ref, Reply},
            erlang:demonitor(Monitor, [flush]),
            Monitors = maps:remove(Monitor, maps:get(monitors, State)),
            retinue_activity:activity_end(),
            State#{monitors => Monitors}
    end.

cancel_client(Monitor, State) ->
    case maps:take(Monitor, maps:get(monitors, State)) of
        error ->
            {not_found, State};
        {Ref, Monitors} ->
            Queue = remove_queued_ref(Ref, maps:get(queue, State)),
            Workers = cancel_worker(Ref, maps:get(workers, State, #{})),
            retinue_activity:activity_end(),
            {canceled, State#{queue => Queue, workers => Workers,
                              monitors => Monitors}}
    end.

cancel_worker(Ref, Workers) ->
    maps:filtermap(
      fun(_Token, Worker = #{item := Item}) ->
          case remove_group_ref(Ref, Item) of
              empty -> stop_worker(Worker), false;
              Updated -> {true, Worker#{item => Updated}}
          end
      end, Workers).

remove_queued_ref(Ref, Queue) ->
    queue:from_list(
      lists:filtermap(
        fun(Item) ->
            case remove_group_ref(Ref, Item) of
                empty -> false;
                Updated -> {true, Updated}
            end
        end, queue:to_list(Queue))).

remove_group_ref(Ref, Item) ->
    case [Queued || Queued <- group_items(Item), maps:get(ref, Queued) =/= Ref] of
        [] -> empty;
        [Primary | Followers] -> Primary#{followers => Followers}
    end.

group_items(Item) ->
    [maps:remove(followers, Item) | maps:get(followers, Item, [])].

take_worker_by_monitor(Monitor, Workers) ->
    case [{Token, Worker} ||
             {Token, Worker = #{worker_monitor := WorkerMonitor}} <- maps:to_list(Workers),
             WorkerMonitor =:= Monitor] of
        [{Token, #{item := Item}}] -> {ok, Item, maps:remove(Token, Workers)};
        [] -> error
    end.

stop_worker(undefined) -> ok;
stop_worker(Current) ->
    case maps:get(worker_pid, Current, undefined) of
        Pid when is_pid(Pid) -> exit(Pid, kill);
        undefined -> ok
    end,
    erlang:demonitor(maps:get(worker_monitor, Current), [flush]),
    ok.

safe_execute(Request, State) ->
    safe_execute_hook(Request, State).

-ifdef(TEST).
safe_execute_hook(Request, State) ->
    case application:get_env(retinue_daemon, daemon_catalog_execute_fun, undefined) of
        Fun when is_function(Fun, 2) -> Fun(Request, State);
        undefined -> safe_execute_default(Request, State)
    end.
-else.
safe_execute_hook(Request, State) ->
    safe_execute_default(Request, State).
-endif.

safe_execute_default(Request, State) ->
    try execute(Request, State) of
        Result -> Result
    catch
        Class:Reason:Stacktrace ->
            logger:error("catalog query failed: ~p:~p~n~p", [Class, Reason, Stacktrace]),
            {{error, {catalog_query_failed, Class, Reason}}, State}
    end.

execute(Request, State) ->
    Command = maps:get(command, Request),
    Cwd = maps:get(cwd, Request, filename:absname(".")),
    case {query_service(Command), maps:find(query, Request)} of
        {{ok, Service}, {ok, Query0}} ->
            RequestQuery = normalize_query_paths(Query0, Cwd),
            case prepare_query(Service, Command, RequestQuery) of
                {ok, Query} ->
                    case ensure_sources(Command, Query) of
                        ok ->
                            case cached_entries(Command, Query, State) of
                                {ok, Cached, State1} ->
                                    case execute_entries(Service, Command, Query, Cached) of
                                        {ok, Query1, Results} ->
                                            {{ok, #{command => Command, query => Query1,
                                                    results => Results}}, State1};
                                        {error, Errors} -> {{error, {query_errors, Errors}}, State1}
                                    end;
                                {error, Reason, State1} ->
                                    {{error, Reason}, State1}
                            end;
                        {error, Reason} ->
                            {{error, Reason}, State}
                    end;
                {error, Errors} ->
                    {{error, {query_errors, Errors}}, State}
            end;
        {error, _} ->
            {{error, {unsupported_catalog_command, Command}}, State};
        {_, error} ->
            {{error, missing_typed_query}, State}
    end.

query_service(Command) when Command =:= "mods"; Command =:= "items" ->
    {ok, wfcli_exports_query};
query_service(Command) when Command =:= "codex"; Command =:= "enemies"; Command =:= "drops" ->
    {ok, wfcli_knowledge_query};
query_service(_Command) -> error.

prepare_query(wfcli_exports_query, Command, Query) ->
    wfcli_exports_query:prepare(Command, Query);
prepare_query(wfcli_knowledge_query, Command, Query) ->
    wfcli_knowledge_query:prepare(Command, Query).

ensure_sources(Command, Query) ->
    try wfcli_source_manager:ensure_catalog(Command, Query)
    catch
        exit:{noproc, _} -> {error, source_manager_unavailable};
        exit:Reason -> {error, {source_manager_failed, Reason}}
    end.

execute_entries(wfcli_exports_query, Command, Query, #{entries := Entries}) ->
    wfcli_exports_query:execute_entries(Command, Query, Entries);
execute_entries(wfcli_knowledge_query, Command, Query,
                #{entries := Entries, source_meta := Meta}) ->
    wfcli_knowledge_query:execute_entries(Command, Query, Entries, Meta).

cached_entries("mods", Parsed, State) ->
    Dir = maps:get(exports_dir, Parsed, undefined),
    Raw = maps:get(raw, Parsed, false),
    Paths = [wfcli_exports:mod_source(Dir)],
    cached({?CACHE_VERSION, entities, mods, Dir, Raw}, Paths,
           fun() ->
               case wfcli_exports:load_mods(Dir) of
                   {ok, Data} ->
                       {ok, #{entries => wfcli_exports_query:build_entries("mods", Parsed, Data)}};
                   {error, Reason} -> {error, Reason}
               end
           end, State);
cached_entries("items", Parsed, State) ->
    Dir = maps:get(exports_dir, Parsed, undefined),
    Raw = maps:get(raw, Parsed, false),
    Files = maps:get(files, Parsed, []),
    Sources = wfcli_exports:item_sources(Dir, Files),
    Paths = [Path || {_File, Path} <- Sources],
    Key = {?CACHE_VERSION, entities, items, Dir, [File || {File, _Path} <- Sources], Raw},
    cached(Key, Paths,
           fun() ->
               case wfcli_exports:load_items(Dir, Files) of
                   {ok, Data} ->
                       {ok, #{entries => wfcli_exports_query:build_entries("items", Parsed, Data)}};
                   {error, Reason} -> {error, Reason}
               end
           end, State);
cached_entries("codex", Parsed, State) ->
    Dir = maps:get(exports_dir, Parsed, undefined),
    Sources = wfcli_knowledge:codex_sources(Dir),
    Paths = [Path || {_File, Path} <- Sources],
    cached({?CACHE_VERSION, entities, codex, Dir}, Paths,
           fun() -> build_knowledge_cache("codex", Parsed) end, State);
cached_entries(Command, Parsed, State) when Command =:= "enemies"; Command =:= "drops" ->
    Dir = maps:get(knowledge_dir, Parsed, undefined),
    Path = wfcli_knowledge:wfcd_source(Dir),
    cached({?CACHE_VERSION, entities, Command, Path}, [Path],
           fun() -> build_knowledge_cache(Command, Parsed) end, State).

build_knowledge_cache(Command, Parsed) ->
    case wfcli_knowledge_query:load_data(Command, Parsed) of
        {ok, Data, Meta} ->
            Entries = wfcli_knowledge_query:build_entries(Command, Parsed, Data),
            {ok, #{entries => Entries, source_meta => Meta}};
        {error, Reason} -> {error, Reason}
    end.

cached(Key, Paths, Loader, State) ->
    Signature = signature(Paths),
    Cache = maps:get(cache, State),
    case maps:get(Key, Cache, undefined) of
        #{signature := Signature, data := Data} ->
            {ok, Data, State};
        _ ->
            case Loader() of
                {ok, Data} ->
                    Entry = #{signature => Signature, data => Data},
                    {ok, Data, State#{cache => Cache#{Key => Entry}}};
                {error, Reason} ->
                    {error, Reason, State}
            end
    end.

signature(Paths) ->
    [{Path, file_signature(Path)} || Path <- Paths].

file_signature(Path) ->
    case file:read_file_info(Path, [{time, posix}]) of
        {ok, Info} -> {Info#file_info.mtime, Info#file_info.size};
        {error, Reason} -> {missing, Reason}
    end.

normalize_query_paths(Query, Cwd) ->
    normalize_query_path(knowledge_dir, normalize_query_path(exports_dir, Query, Cwd), Cwd).

normalize_query_path(Key, Query, Cwd) ->
    case maps:get(Key, Query, undefined) of
        undefined -> Query;
        Path -> Query#{Key => absolute_path(Path, Cwd)}
    end.

absolute_path(Path, Cwd) ->
    case filename:pathtype(Path) of
        absolute -> Path;
        _ -> filename:absname(Path, Cwd)
    end.

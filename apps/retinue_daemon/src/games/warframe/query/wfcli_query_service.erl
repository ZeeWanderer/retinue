%%%-------------------------------------------------------------------
%% Concurrent daemon-owned unified query coordinator.
%%%-------------------------------------------------------------------
-module(wfcli_query_service).

-behaviour(gen_server).

-export([start_link/0, submit/2, status/0, execute/1, select_datasets/1]).
-ifdef(TEST).
-export([paginate_worldstate/2]).
-endif.
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(WORKER_TIMEOUT_MS, 120000).
-define(DEFAULT_WORKERS, 8).

-doc "Start the unified query coordinator.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Start one unified query; result arrives as `{retinue_daemon, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Return unified query worker state.".
-spec status() -> map().
status() -> gen_server:call(?SERVER, status).

init([]) ->
    {ok, #{queue => queue:new(), workers => #{},
           client_monitors => #{}, max_workers => worker_limit()}}.

handle_call({submit, Client, Request}, _From, State) when is_pid(Client), is_map(Request) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Job = #{client => Client, ref => Ref, monitor => Monitor, request => Request},
    Monitors = maps:get(client_monitors, State),
    retinue_activity:activity_start(),
    State1 = State#{client_monitors => Monitors#{Monitor => Ref}},
    {reply, {ok, Ref}, enqueue_job(Job, State1)};
handle_call(status, _From, State) ->
    {reply, #{queued => queue:len(maps:get(queue, State)),
              processing => map_size(maps:get(workers, State, #{})) > 0,
              active => map_size(maps:get(workers, State, #{}))}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast(_Message, State) -> {noreply, State}.

handle_info(process_queue, State) ->
    {noreply, start_queued(State)};
handle_info({query_result, Token, Result}, State) ->
    case maps:take(Token, maps:get(workers, State, #{})) of
        error -> {noreply, State};
        {#{worker_monitor := WorkerMonitor, job := Job}, Workers} ->
            erlang:demonitor(WorkerMonitor, [flush]),
            {noreply, start_queued(complete_job(Job, Result, State#{workers => Workers}))}
    end;
handle_info({'DOWN', Monitor, process, _Pid, Reason}, State) ->
    case take_worker_by_monitor(Monitor, maps:get(workers, State, #{})) of
        {ok, Job, Workers} ->
            {noreply, start_queued(
                        complete_job(Job, {error, {query_worker_down, Reason}},
                                     State#{workers => Workers}))};
        error ->
            case cancel_client(Monitor, State) of
                {not_found, State1} -> {noreply, State1};
                {canceled, State1} -> {noreply, start_queued(State1)}
            end
    end;
handle_info(_Message, State) -> {noreply, State}.

terminate(_Reason, State) ->
    maps:foreach(fun(_Token, Worker) -> stop_worker(Worker) end,
                 maps:get(workers, State, #{})),
    ok.

code_change(_OldVsn, State, _Extra) ->
    Workers0 = migrate_current(maps:get(current, State, undefined),
                               maps:get(workers, State, #{})),
    State1 = maps:without([current, processing], State),
    self() ! process_queue,
    {ok, State1#{queue => maps:get(queue, State, queue:new()),
                workers => Workers0,
                client_monitors => maps:get(client_monitors, State, #{}),
                max_workers => maps:get(max_workers, State, worker_limit())}}.

migrate_current(undefined, Workers) -> Workers;
migrate_current(Current = #{token := Token}, Workers) ->
    Workers#{Token => maps:remove(token, Current)};
migrate_current(_Current, Workers) -> Workers.

enqueue_job(Job, State) ->
    case map_size(maps:get(workers, State)) < maps:get(max_workers, State) of
        true -> start_job(Job, State);
        false -> State#{queue => queue:in(Job, maps:get(queue, State))}
    end.

start_job(Job, State) ->
    Parent = self(),
    Token = make_ref(),
    {Pid, WorkerMonitor} = spawn_monitor(fun() ->
        Result = try execute_request(maps:get(request, Job))
                 catch Class:Reason:Stack ->
                     {error, {query_crash, Class, Reason, Stack}}
                 end,
        Parent ! {query_result, Token, Result}
    end),
    Worker = #{worker_pid => Pid, worker_monitor => WorkerMonitor, job => Job},
    State#{workers => (maps:get(workers, State, #{}))#{Token => Worker}}.

start_queued(State) ->
    case map_size(maps:get(workers, State)) < maps:get(max_workers, State) of
        false -> State;
        true ->
            case queue:out(maps:get(queue, State)) of
                {empty, Queue} -> State#{queue => Queue};
                {{value, Job}, Queue} -> start_queued(start_job(Job, State#{queue => Queue}))
            end
    end.

worker_limit() ->
    case application:get_env(retinue_daemon, query_workers, ?DEFAULT_WORKERS) of
        Count when is_integer(Count), Count > 0 -> Count;
        _ -> ?DEFAULT_WORKERS
    end.

-ifdef(TEST).
execute_request(Request) ->
    case application:get_env(retinue_daemon, daemon_query_execute_fun, undefined) of
        Fun when is_function(Fun, 1) -> Fun(Request);
        undefined -> execute(Request)
    end.
-else.
execute_request(Request) -> execute(Request).
-endif.

-doc "Parse and execute one unified query entirely inside retinue_daemon.".
-spec execute(map()) -> {ok, map()} | {error, term()}.
execute(Request) ->
    Tokens0 = maps:get(query_tokens, Request, []),
    case select_datasets(Tokens0) of
        {ok, Tokens, Datasets, Explicit} ->
            case wfcli_query_parse:parse_arguments(Tokens) of
                {ok, Ast} ->
                    Query = string:join(Tokens, " "),
                    Results = execute_datasets(Datasets, Query, Ast, Request),
                    {ok, #{query_tokens => Tokens, datasets => Results,
                           dataset_explicit => Explicit}};
                {error, Error} -> {error, {query_errors, [Error]}}
            end;
        {error, Error} -> {error, {query_errors, [Error]}}
    end.

execute_datasets(Datasets, Query, Ast, Request) ->
    Parent = self(),
    Batch = make_ref(),
    _ = [spawn_link(fun() ->
             Result = try execute_dataset_request(Dataset, Query, Ast, Request)
                      catch Class:Reason:Stack ->
                          #{dataset => Dataset,
                            reply => {error, {dataset_query_crash, Class, Reason, Stack}}}
                      end,
             Parent ! {query_dataset_result, Batch, Index, Result}
         end)
         || {Index, Dataset} <- lists:enumerate(0, Datasets)],
    collect_dataset_results(Batch, length(Datasets), #{}).

collect_dataset_results(_Batch, 0, Results) ->
    [Result || {_Index, Result} <- lists:sort(maps:to_list(Results))];
collect_dataset_results(Batch, Remaining, Results) ->
    receive
        {query_dataset_result, Batch, Index, Result} ->
            collect_dataset_results(Batch, Remaining - 1, Results#{Index => Result})
    end.

-ifdef(TEST).
execute_dataset_request(Dataset, Query, Ast, Request) ->
    case application:get_env(retinue_daemon, query_dataset_execute_fun, undefined) of
        Fun when is_function(Fun, 4) -> Fun(Dataset, Query, Ast, Request);
        undefined -> execute_dataset(Dataset, Query, Ast, Request)
    end.
-else.
execute_dataset_request(Dataset, Query, Ast, Request) ->
    execute_dataset(Dataset, Query, Ast, Request).
-endif.

execute_dataset(worldstate, Query, _Ast, Request) ->
    Opts = worldstate_opts(Request),
    WorkerRequest = #{source => worldstate, opts => Opts, query => Query,
                      type_filter => undefined, day_filter => undefined,
                      mode => list, inventory => false},
    Reply = submit_and_wait(wfcli_worldstate_service, WorkerRequest),
    #{dataset => worldstate, reply => paginate_worldstate(Reply, Request)};
execute_dataset(player, _Query, Ast, Request) ->
    Snapshot = wfcli_player_service:snapshot(),
    #{dataset => player, reply => wfcli_player_query:execute(Ast, Request, Snapshot)};
execute_dataset(market, _Query, Ast, Request) ->
    WorkerRequest = #{source => market, action => query, query_ast => Ast,
                      limit => maps:get(limit, Request, infinity),
                      offset => maps:get(offset, Request, 0),
                      raw => maps:get(raw, Request, false),
                      output_format => maps:get(output_format, Request, table)},
    #{dataset => market, reply => submit_and_wait(wfcli_market_service, WorkerRequest)};
execute_dataset(diagnostics, _Query, Ast, Request) ->
    Reply = case wfcli_resolution_issues:refresh() of
        ok -> wfcli_diagnostics_query:execute(
                Ast, Request, wfcli_resolution_issues:list());
        {error, _Reason} = Error -> Error
    end,
    #{dataset => diagnostics, reply => Reply};
execute_dataset(Dataset, _Query, Ast, Request) when Dataset =:= incidents; Dataset =:= captures ->
    Rows = case Dataset of
        incidents -> wfcli_runtime_observations:incidents();
        captures -> wfcli_runtime_observations:captures()
    end,
    Reply = case Rows of
        {ok, #{entries := Values} = Report} ->
            case wfcli_diagnostics_query:execute(Dataset, Ast, Request, Values) of
                {ok, #{results := Results} = Result} ->
                    {ok, Result#{results := maps:merge(maps:remove(entries, Report), Results)}};
                Error -> Error
            end;
        {error, _} = Error -> Error
    end,
    #{dataset => Dataset, reply => Reply};
execute_dataset(Dataset, _Query, Ast, Request) ->
    Command = atom_to_list(Dataset),
    QueryMap = catalog_query(Dataset, Ast, Request),
    WorkerRequest = #{source => exports, command => Command, query => QueryMap,
                      cwd => maps:get(cwd, Request, filename:absname("."))},
    #{dataset => Dataset, reply => submit_and_wait(wfcli_exports_store, WorkerRequest)}.

submit_and_wait(Service, Request) ->
    case Service:submit(self(), Request) of
        {ok, Ref} ->
            receive
                {retinue_daemon, Ref, Reply} -> Reply
            after ?WORKER_TIMEOUT_MS -> {error, {worker_timeout, Service}}
            end;
        {error, Reason} -> {error, Reason}
    end.

worldstate_opts(Request) ->
    Raw = maps:get(raw, Request, false),
    Opts0 = #{refresh => maps:get(refresh, Request, false),
              ttl => maps:get(ttl, Request, 60), resolve_items => not Raw,
              raw => Raw, search_raw => Raw,
              utc => maps:get(utc, Request, false) orelse maps:get(output_format, Request, table) =:= json,
              event_lang => maps:get(event_lang, Request, undefined)},
    maybe_path(cache, Request, Opts0).

catalog_query(codex, Ast, Request) ->
    (base_catalog_query(Ast, Request))#{exports_dir => path(exports_dir, Request)};
catalog_query(Dataset, Ast, Request) when Dataset =:= enemies; Dataset =:= drops ->
    (base_catalog_query(Ast, Request))#{knowledge_dir => path(knowledge_dir, Request)};
catalog_query(_Dataset, Ast, Request) ->
    (base_catalog_query(Ast, Request))#{exports_dir => path(exports_dir, Request)}.

base_catalog_query(Ast, Request) ->
    #{query_ast => Ast, limit => maps:get(limit, Request, infinity),
      offset => maps:get(offset, Request, 0), raw => maps:get(raw, Request, false),
      output_format => maps:get(output_format, Request, table)}.

paginate_worldstate({ok, Result = #{entries := Entries}}, Request) ->
    Offset = maps:get(offset, Request, 0),
    Limit = maps:get(limit, Request, infinity),
    {ok, Result#{entries => take(Limit, drop(Offset, Entries))}};
paginate_worldstate(Reply, _Request) ->
    Reply.

drop(N, List) when N =< 0 -> List;
drop(_N, []) -> [];
drop(N, [_ | Rest]) -> drop(N - 1, Rest).

take(infinity, List) -> List;
take(Limit, List) -> lists:sublist(List, Limit).

maybe_path(Key, Request, Acc) ->
    case path(Key, Request) of undefined -> Acc; Value -> Acc#{Key => Value} end.

path(Key, Request) ->
    case maps:get(Key, Request, undefined) of
        undefined -> undefined;
        Value -> absolute_path(Value, maps:get(cwd, Request, filename:absname(".")))
    end.

absolute_path(Path, Cwd) ->
    case filename:pathtype(Path) of absolute -> Path; _ -> filename:absname(Path, Cwd) end.

-doc "Extract and validate the query-level dataset selector.".
-spec select_datasets([string()]) ->
    {ok, [string()], [atom()], boolean()} | {error, string()}.
select_datasets(Tokens0) ->
    Tokens = lists:append([string:lexemes(Token, " \t") || Token <- Tokens0]),
    select_datasets(Tokens, undefined, []).

select_datasets([], undefined, QueryAcc) ->
    {ok, lists:reverse(QueryAcc), wfcli_protocol:default_datasets(), false};
select_datasets([], Datasets, QueryAcc) ->
    {ok, lists:reverse(QueryAcc), Datasets, true};
select_datasets([Token | Rest], Selected, QueryAcc) ->
    case dataset_token(Token) of
        no -> select_datasets(Rest, Selected, [Token | QueryAcc]);
        {ok, Datasets} when Selected =:= undefined -> select_datasets(Rest, Datasets, QueryAcc);
        {ok, _Datasets} -> {error, "dataset selector may appear only once"};
        {error, Reason} -> {error, Reason}
    end.

dataset_token(Token) ->
    case wfcli_query_parse:parse_op(Token) of
        {ok, Key0, Op, Value} ->
            case string:lowercase(string:trim(Key0)) of
                "dataset" when Op =:= eq; Op =:= default -> parse_datasets(Value);
                "dataset" -> {error, "dataset supports only '=' or ':'"};
                _ -> no
            end;
        error -> no
    end.

parse_datasets(Value) ->
    Names = [string:lowercase(Name) || Name <- wfcli_query_parse:split_vals(Value)],
    case [Name || Name <- Names, dataset_name(Name) =:= undefined] of
        [] when Names =:= [] -> {error, "dataset requires at least one value"};
        [] ->
            case lists:member("all", Names) of
                true -> {ok, wfcli_protocol:all_datasets()};
                false ->
                    {ok, unique_datasets(lists:flatten(
                           [dataset_values(Name) || Name <- Names]))}
            end;
        [Unknown | _] ->
            {error, lists:flatten(io_lib:format(
              "unknown dataset: ~s (use default, worldstate, mods, items, codex, enemies, drops, player, market, diagnostics, incidents, captures, or all)",
              [Unknown]))}
    end.

dataset_name("worldstate") -> worldstate;
dataset_name("mods") -> mods;
dataset_name("items") -> items;
dataset_name("codex") -> codex;
dataset_name("enemies") -> enemies;
dataset_name("drops") -> drops;
dataset_name("player") -> player;
dataset_name("market") -> market;
dataset_name("diagnostics") -> diagnostics;
dataset_name("incidents") -> incidents;
dataset_name("captures") -> captures;
dataset_name("default") -> default;
dataset_name("all") -> all;
dataset_name(_) -> undefined.

dataset_values("default") -> wfcli_protocol:default_datasets();
dataset_values(Name) -> [dataset_name(Name)].

unique_datasets(Datasets) ->
    lists:reverse(lists:foldl(fun(Dataset, Acc) ->
        case lists:member(Dataset, Acc) of true -> Acc; false -> [Dataset | Acc] end
    end, [], Datasets)).

complete_job(#{client := Client, ref := Ref, monitor := Monitor}, Result, State) ->
    case maps:is_key(Monitor, maps:get(client_monitors, State)) of
        true -> Client ! {retinue_daemon, Ref, Result};
        false -> ok
    end,
    erlang:demonitor(Monitor, [flush]),
    retinue_activity:activity_end(),
    State#{client_monitors => maps:remove(Monitor, maps:get(client_monitors, State))}.

cancel_client(Monitor, State) ->
    case maps:take(Monitor, maps:get(client_monitors, State)) of
        error ->
            {not_found, State};
        {Ref, Monitors} ->
            Queue = queue:filter(fun(Job) -> maps:get(ref, Job) =/= Ref end,
                                 maps:get(queue, State)),
            Workers = cancel_worker(Ref, maps:get(workers, State, #{})),
            retinue_activity:activity_end(),
            {canceled, State#{queue => Queue, workers => Workers,
                              client_monitors => Monitors}}
    end.

cancel_worker(Ref, Workers) ->
    maps:filter(
      fun(_Token, Worker = #{job := Job}) ->
          case maps:get(ref, Job) =:= Ref of
              true -> stop_worker(Worker), false;
              false -> true
          end
      end,
      Workers).

take_worker_by_monitor(Monitor, Workers) ->
    case [{Token, Worker} ||
             {Token, Worker = #{worker_monitor := WorkerMonitor}} <- maps:to_list(Workers),
             WorkerMonitor =:= Monitor] of
        [{Token, #{job := Job}}] -> {ok, Job, maps:remove(Token, Workers)};
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

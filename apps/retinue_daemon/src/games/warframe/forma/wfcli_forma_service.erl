%%%-------------------------------------------------------------------
%% Serialized daemon-owned Forma planning jobs.
%%%-------------------------------------------------------------------
-module(wfcli_forma_service).

-behaviour(gen_server).

-export([start_link/0, submit/2, status/0, plan_request/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).

-doc "Start the single Forma planning queue.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Queue one planner request; result arrives as `{retinue_daemon, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Return planner queue state.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-doc "Load, normalize, and plan configs without presentation formatting.".
-spec plan_request(map()) -> {ok, map()} | {error, term()}.
plan_request(Request) ->
    Flags = maps:get(flags, Request, #{}),
    case raw_configs(Request) of
        {ok, RawConfigs} ->
            case normalize_configs(RawConfigs) of
                {ok, Configs} ->
                    case maps:get(action, Request, plan) of
                        config_layout ->
                            {ok, #{results => [config_layout(Config) || Config <- Configs]}};
                        plan ->
                            {ok, #{results => [run_single(Config, Flags) || Config <- Configs]}};
                        Action ->
                            {error, {unsupported_forma_action, Action}}
                    end;
                {error, Errors} -> {error, {config_errors, Errors}}
            end;
        {error, Errors} -> {error, {config_errors, Errors}}
    end.

raw_configs(#{config_data := Configs}) when is_list(Configs) ->
    {ok, Configs};
raw_configs(Request) ->
    wfcli_forma_config:load_files(maps:get(configs, Request, [])).

init([]) ->
    {ok, #{queue => queue:new(), current => undefined, client_monitors => #{}}}.

handle_call({submit, Client, Request}, _From, State) when is_pid(Client), is_map(Request) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Job = #{client => Client, ref => Ref, monitor => Monitor, request => Request},
    Monitors = maps:get(client_monitors, State),
    retinue_activity:activity_start(),
    self() ! process_queue,
    {reply, {ok, Ref},
     State#{queue => queue:in(Job, maps:get(queue, State)),
            client_monitors => Monitors#{Monitor => Ref}}};
handle_call(status, _From, State) ->
    {reply, #{queued => queue:len(maps:get(queue, State)),
              processing => maps:get(current, State) =/= undefined}, State};
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
            {Pid, WorkerMonitor} = spawn_monitor(
              fun() ->
                  Result = try run_job(maps:get(request, Job))
                           catch Class:Reason:Stack ->
                               {error, {planner_crash, Class, Reason, Stack}}
                           end,
                  Parent ! {planner_result, Token, Result}
              end),
            Current = #{token => Token, worker_pid => Pid,
                        worker_monitor => WorkerMonitor, job => Job},
            {noreply, State#{queue => Queue, current => Current}}
    end;
handle_info({planner_result, Token, Result},
            State = #{current := #{token := Token, worker_monitor := WorkerMonitor,
                                   job := Job}}) ->
    erlang:demonitor(WorkerMonitor, [flush]),
    State1 = complete_job(Job, Result, State#{current => undefined}),
    self() ! process_queue,
    {noreply, State1};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{current := #{worker_monitor := Monitor, job := Job}}) ->
    State1 = complete_job(Job, {error, {planner_worker_down, Reason}},
                          State#{current => undefined}),
    self() ! process_queue,
    {noreply, State1};
handle_info({'DOWN', Monitor, process, _Pid, _Reason}, State) ->
    case cancel_client(Monitor, State) of
        {not_found, State1} -> {noreply, State1};
        {canceled, State1} ->
            self() ! process_queue,
            {noreply, State1}
    end;
handle_info(_Message, State) -> {noreply, State}.

terminate(_Reason, State) ->
    stop_worker(maps:get(current, State, undefined)),
    ok.

code_change(_OldVsn, State, _Extra) ->
    {ok, State#{queue => maps:get(queue, State, queue:new()),
                current => maps:get(current, State, undefined),
                client_monitors => maps:get(client_monitors, State, #{})}}.

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
            Current = maps:get(current, State, undefined),
            Current1 = case Current of
                #{job := #{ref := Ref}} ->
                    stop_worker(Current),
                    undefined;
                _ -> Current
            end,
            retinue_activity:activity_end(),
            {canceled, State#{queue => Queue, current => Current1,
                              client_monitors => Monitors}}
    end.

stop_worker(undefined) -> ok;
stop_worker(Current) ->
    case maps:get(worker_pid, Current, undefined) of
        Pid when is_pid(Pid) -> exit(Pid, kill);
        undefined -> ok
    end,
    erlang:demonitor(maps:get(worker_monitor, Current), [flush]),
    ok.

-ifdef(TEST).
run_job(Request) ->
    case maps:get(test_fun, Request, undefined) of
        Fun when is_function(Fun, 0) -> Fun();
        undefined -> plan_request(Request)
    end.
-else.
run_job(Request) -> plan_request(Request).
-endif.

normalize_configs(Configs) ->
    {Norms, Errors} = lists:foldl(
      fun(Config, {Acc, ErrorAcc}) ->
          case wfcli_forma_model:normalize_config(Config) of
              {ok, Normalized} -> {[Normalized | Acc], ErrorAcc};
              {error, ConfigErrors} ->
                  File = maps:get(file, Config, <<"unknown">>),
                  Prefixed = [io_lib:format("~s: ~s", [File, Error])
                              || Error <- ConfigErrors],
                  {Acc, Prefixed ++ ErrorAcc}
          end
      end,
      {[], []}, Configs),
    case Errors of
        [] -> {ok, lists:reverse(Norms)};
        _ -> {error, lists:reverse(Errors)}
    end.

run_single(Config, Flags) ->
    Config0 = decorate_current_layout(Config),
    case wfcli_forma_planner:plan(Config, Flags) of
        {Plan, Cost} when is_map(Plan) ->
            Config1 = decorate_config(Config0, Plan),
            {ok, Config1, Plan, Cost};
        {error, Reason} -> {error, Config0, Reason};
        Other -> {error, Config0, Other}
    end.

-doc "Attach daemon-computed assignment and arcane data for one polarity plan.".
-spec decorate_config(map(), map()) -> map().
decorate_config(Config, Plan) ->
    Config#{computed_slot_mods => slot_mod_labels(Config, Plan),
            computed_build_arcanes => build_arcane_entries(Config)}.

config_layout(Config) ->
    Config1 = decorate_current_layout(Config),
    {ok, Config1, maps:get(computed_current_plan, Config1), 0}.

decorate_current_layout(Config = #{item := Item}) ->
    Plan = current_plan(Item),
    Config#{computed_current_plan => Plan,
            computed_current_slot_mods => slot_mod_labels(Config, Plan),
            computed_build_arcanes => build_arcane_entries(Config)}.

current_plan(Item) ->
    Slots = maps:get(slots, Item, []),
    Normal = maps:from_list(lists:zip(lists:seq(1, length(Slots)), Slots)),
    Normal#{aura => maps:get(aura_slot, Item, none),
            exilus => maps:get(exilus_slot, Item, none)}.

slot_mod_labels(Config, Plan) ->
    case wfcli_forma_planner:assignments_for_plan(Config, Plan) of
        {ok, SlotMap} ->
            [{Slot, lists:sort(Mods)}
             || {Slot, Mods} <- lists:sort(fun slot_order/2, maps:to_list(SlotMap))];
        _ -> []
    end.

build_arcane_entries(#{builds := Builds}) when is_list(Builds) ->
    {Reversed, _} = lists:foldl(
      fun(Build, {Acc, Index}) ->
          Name = maps:get(name, Build, maps:get(<<"name">>, Build,
                                               io_lib:format("build-~B", [Index]))),
          Arcanes = maps:get(arcanes, Build, maps:get(<<"arcanes">>, Build, [])),
          case Arcanes of
              [] -> {Acc, Index + 1};
              _ -> {[{Name, Arcanes} | Acc], Index + 1}
          end
      end,
      {[], 1}, Builds),
    lists:reverse(Reversed);
build_arcane_entries(_Other) -> [].

slot_order(aura, aura) -> true;
slot_order(aura, _) -> true;
slot_order(_, aura) -> false;
slot_order(exilus, exilus) -> true;
slot_order(exilus, _) -> true;
slot_order(_, exilus) -> false;
slot_order(A, B) when is_integer(A), is_integer(B) -> A =< B;
slot_order(A, _B) when is_integer(A) -> true;
slot_order(_, _) -> true.

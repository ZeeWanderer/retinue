%%%-------------------------------------------------------------------
%% Daemon-owned catalog dependency validation and metadata updates.
%%%-------------------------------------------------------------------
-module(wfcli_source_manager).

-behaviour(gen_server).

-include_lib("kernel/include/file.hrl").

-export([start_link/0, ensure_catalog/2, submit/2, status/0, requirements/2]).
-ifdef(TEST).
-export([expand_selections/1, stale_file/3, stale_wfcd/3]).
-endif.
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(DEFAULT_REFRESH_INTERVAL_MS, 3600000).
-define(DEFAULT_MAX_AGE_SECONDS, 86400).

-type requirement() :: #{kind := export | wfcd | star_chart,
                         id := string(),
                         path := file:filename_all(),
                         managed := boolean()}.

-doc "Start the serialized metadata/source job manager.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Ensure every source required by a catalog request is valid and locally available.".
-spec ensure_catalog(string(), map()) -> ok | {error, term()}.
ensure_catalog(Command, Query) ->
    gen_server:call(?SERVER, {ensure_catalog, Command, Query}, infinity).

-doc "Queue a metadata refresh; result arrives as `{retinue_daemon, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Return source queue state.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-doc "Map one catalog command to concrete source artifacts.".
-spec requirements(string(), map()) -> [requirement()].
requirements("mods", Query) ->
    Dir = maps:get(exports_dir, Query, undefined),
    [export_requirement("ExportUpgrades_en.json", wfcli_exports:mod_source(Dir), Dir)];
requirements("items", Query) ->
    Dir = maps:get(exports_dir, Query, undefined),
    [export_requirement(File, Path, Dir)
     || {File, Path} <- wfcli_exports:item_sources(Dir, maps:get(files, Query, []))];
requirements("codex", Query) ->
    Dir = maps:get(exports_dir, Query, undefined),
    [export_requirement(File, Path, Dir)
     || {File, Path} <- wfcli_knowledge:codex_sources(Dir)];
requirements(Command, Query) when Command =:= "enemies"; Command =:= "drops" ->
    Dir = maps:get(knowledge_dir, Query, undefined),
    [#{kind => wfcd, id => "WFCDEnemy.json",
       path => wfcli_knowledge:wfcd_source(Dir), managed => Dir =:= undefined}];
requirements("player_views", _Query) ->
    [#{kind => wfcd, id => "WFCDItems.json",
       path => wfcli_item_catalog:source(), managed => true},
     export_requirement("ExportRecipes_en.json",
                        wfcli_item_catalog:recipe_source(), undefined)];
requirements("archimedea", _Query) ->
    [export_requirement(File, Path, undefined)
     || {File, Path} <- wfcli_exports:item_sources(
                          undefined,
                          ["ExportWarframes_en.json", "ExportWeapons_en.json"])];
requirements("mastery_star_chart", _Query) ->
    [#{kind => star_chart, id => "StarChart.json",
       path => wfcli_star_chart:source(), managed => true}];
requirements(_Command, _Query) ->
    [].

export_requirement(File, Path, Dir) ->
    #{kind => export, id => File, path => Path, managed => Dir =:= undefined}.

init([]) ->
    Interval = daemon_env(source_refresh_interval_ms, ?DEFAULT_REFRESH_INTERVAL_MS),
    MaxAge = daemon_env(source_max_age_seconds, ?DEFAULT_MAX_AGE_SECONDS),
    State = #{queue => queue:new(), current => undefined, client_monitors => #{},
              refresh_interval_ms => Interval, max_age_seconds => MaxAge,
              refresh_timer => undefined},
    {ok, schedule_refresh(State, initial_refresh_delay(Interval))}.

handle_call({ensure_catalog, Command, Query}, From, State) ->
    Job = #{kind => ensure, command => Command, query => Query, reply => {call, From}},
    {noreply, enqueue(Job, State)};
handle_call({submit, Client, Request}, _From, State) when is_pid(Client), is_map(Request) ->
    case maps:get(action, Request, undefined) of
        refresh ->
            Ref = make_ref(),
            Monitor = erlang:monitor(process, Client),
            Job = #{kind => refresh, request => Request,
                    reply => {client, Client, Ref, Monitor}},
            retinue_activity:activity_start(),
            Monitors = maps:get(client_monitors, State),
            {reply, {ok, Ref}, enqueue(Job, State#{client_monitors => Monitors#{Monitor => Ref}})};
        Action ->
            {reply, {error, {unsupported_source_action, Action}}, State}
    end;
handle_call(status, _From, State) ->
    {reply, #{queued => queue:len(maps:get(queue, State)),
              processing => maps:get(current, State) =/= undefined,
              refresh_interval_ms => maps:get(refresh_interval_ms, State),
              max_age_seconds => maps:get(max_age_seconds, State)}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast(_Message, State) ->
    {noreply, State}.

handle_info(process_queue, State = #{current := Current}) when Current =/= undefined ->
    {noreply, State};
handle_info(process_queue, State) ->
    case queue:out(maps:get(queue, State)) of
        {empty, Queue} ->
            {noreply, State#{queue => Queue}};
        {{value, Job}, Queue} ->
            Parent = self(),
            Token = make_ref(),
            {_Pid, WorkerMonitor} = spawn_monitor(
              fun() ->
                  Result = try execute_job(Job)
                           catch Class:Reason:Stack ->
                               {error, {source_job_crash, Class, Reason, Stack}}
                           end,
                  Parent ! {source_job_result, Token, Result}
              end),
            Current = #{token => Token, worker_monitor => WorkerMonitor, job => Job},
            {noreply, State#{queue => Queue, current => Current}}
    end;
handle_info({source_job_result, Token, Result},
            State = #{current := #{token := Token, worker_monitor := WorkerMonitor, job := Job}}) ->
    erlang:demonitor(WorkerMonitor, [flush]),
    State1 = complete_job(Job, Result, State#{current => undefined}),
    self() ! process_queue,
    {noreply, State1};
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{current := #{worker_monitor := Monitor, job := Job}}) ->
    State1 = complete_job(Job, {error, {source_worker_down, Reason}},
                          State#{current => undefined}),
    self() ! process_queue,
    {noreply, State1};
handle_info({'DOWN', Monitor, process, _Pid, _Reason}, State) ->
    case maps:take(Monitor, maps:get(client_monitors, State)) of
        error -> {noreply, State};
        {Ref, Monitors} ->
            Queue = queue:filter(fun(Job) -> not job_ref(Job, Ref) end, maps:get(queue, State)),
            retinue_activity:activity_end(),
            {noreply, State#{queue => Queue, client_monitors => Monitors}}
    end;
handle_info(refresh_stale_sources, State) ->
    State1 = State#{refresh_timer => undefined},
    State2 = maybe_enqueue_stale_refresh(State1),
    {noreply, schedule_refresh(State2, maps:get(refresh_interval_ms, State2))};
handle_info(_Message, State) ->
    {noreply, State}.

terminate(_Reason, State) ->
    cancel_refresh_timer(maps:get(refresh_timer, State, undefined)),
    ok.

code_change(_OldVsn, State, _Extra) ->
    Interval = maps:get(refresh_interval_ms, State,
                        daemon_env(source_refresh_interval_ms,
                                   ?DEFAULT_REFRESH_INTERVAL_MS)),
    State1 = State#{queue => maps:get(queue, State, queue:new()),
                    current => maps:get(current, State, undefined),
                    client_monitors => maps:get(client_monitors, State, #{}),
                    refresh_interval_ms => Interval,
                    max_age_seconds =>
                        maps:get(max_age_seconds, State,
                                 daemon_env(source_max_age_seconds,
                                            ?DEFAULT_MAX_AGE_SECONDS)),
                    refresh_timer => maps:get(refresh_timer, State, undefined)},
    {ok, ensure_refresh_timer(State1)}.

enqueue(Job, State) ->
    self() ! process_queue,
    State#{queue => queue:in(Job, maps:get(queue, State))}.

complete_job(#{reply := {call, From}}, Result, State) ->
    gen_server:reply(From, Result),
    State;
complete_job(#{reply := {client, Client, Ref, Monitor}}, Result, State) ->
    case maps:is_key(Monitor, maps:get(client_monitors, State)) of
        true -> Client ! {retinue_daemon, Ref, Result};
        false -> ok
    end,
    erlang:demonitor(Monitor, [flush]),
    retinue_activity:activity_end(),
    State#{client_monitors => maps:remove(Monitor, maps:get(client_monitors, State))};
complete_job(#{reply := none}, {ok, _}, State) ->
    State;
complete_job(#{reply := none}, Result, State) ->
    logger:warning("automatic knowledge refresh failed: ~p", [Result]),
    State.

job_ref(#{reply := {client, _Client, Ref, _Monitor}}, Ref) -> true;
job_ref(_Job, _Ref) -> false.

execute_job(#{kind := ensure, command := Command, query := Query}) ->
    ensure_requirements(Command, Query);
execute_job(#{kind := refresh, request := Request}) ->
    refresh_sources(maps:get(selections, Request, [default]));
execute_job(#{kind := periodic, selections := Selections}) ->
    refresh_sources(Selections).

ensure_requirements(Command, Query) ->
    Requirements = requirements(Command, Query),
    Failures = lists:filtermap(
      fun(Requirement) ->
          case validate_requirement(Requirement) of
              ok -> false;
              Error -> {true, {Requirement, Error}}
          end
      end, Requirements),
    case [Failure || {#{managed := false}, _} = Failure <- Failures] of
        [{Requirement, {error, Reason}} | _] ->
            source_error(Requirement, Reason, custom);
        [] ->
            Managed = [Requirement || {Requirement, _} <- Failures],
            case refresh_requirements(Managed) of
                ok -> validate_requirements(requirements(Command, Query));
                {error, _Reason} = Error -> Error
            end
    end.

validate_requirements([]) -> ok;
validate_requirements([Requirement | Rest]) ->
    case validate_requirement(Requirement) of
        ok -> validate_requirements(Rest);
        {error, Reason} -> source_error(Requirement, Reason, managed)
    end.

validate_requirement(#{kind := export, path := Path}) ->
    validate_json(Path, fun(Map) -> is_map(Map) end);
validate_requirement(#{kind := wfcd, path := Path}) ->
    validate_json(Path,
                  fun(#{<<"entries">> := Entries}) -> is_list(Entries);
                     (_) -> false
                  end);
validate_requirement(#{kind := star_chart, path := Path}) ->
    validate_json(Path,
                  fun(#{<<"nodes">> := Nodes, <<"junctions">> := Junctions}) ->
                          is_map(Nodes) andalso is_list(Junctions);
                     (_) -> false
                  end).

validate_json(Path, Valid) ->
    case file:read_file(Path) of
        {ok, Body} ->
            try jsone:decode(Body, [{object_format, map}]) of
                Value -> case Valid(Value) of true -> ok; false -> {error, invalid_payload} end
            catch _:_ -> {error, invalid_json}
            end;
        {error, Reason} -> {error, Reason}
    end.

source_error(Requirement, Reason, Policy) ->
    {error, {source_unavailable, maps:get(kind, Requirement), maps:get(id, Requirement),
             maps:get(path, Requirement), Reason, Policy}}.

refresh_requirements([]) -> ok;
refresh_requirements(Requirements) ->
    ExportFiles = lists:usort([maps:get(id, R) || R <- Requirements,
                                                   maps:get(kind, R) =:= export]),
    NeedWfcd = lists:any(fun(R) -> maps:get(kind, R) =:= wfcd end, Requirements),
    NeedStarChart = lists:any(fun(R) -> maps:get(kind, R) =:= star_chart end,
                              Requirements),
    case maybe_update_exports(ExportFiles) of
        ok ->
            case maybe_run_update(NeedWfcd, wfcd) of
                ok -> maybe_run_update(NeedStarChart, star_chart);
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

maybe_update_exports([]) -> ok;
maybe_update_exports(Files) -> run_update({exports, Files}).

maybe_run_update(true, Source) -> run_update(Source);
maybe_run_update(false, _Source) -> ok.

refresh_sources(Selections0) ->
    Selections = expand_selections(Selections0),
    Results = [#{source => Selection, result => run_update(Selection)}
               || Selection <- Selections],
    {ok, #{results => Results,
           success => lists:all(fun(#{result := Result}) -> Result =:= ok end, Results)}}.

expand_selections(Selections) ->
    Expanded = lists:flatten([expand_selection(Selection) || Selection <- Selections]),
    unique(Expanded, []).

expand_selection(default) -> [nodes, languages, exports, wfcd, star_chart];
expand_selection(all) -> [nodes, languages, exports, wfcd, star_chart];
expand_selection(Selection) -> [Selection].

unique([], Acc) -> lists:reverse(Acc);
unique([Value | Rest], Acc) ->
    case lists:member(Value, Acc) of
        true -> unique(Rest, Acc);
        false -> unique(Rest, [Value | Acc])
    end.

run_update(Action) ->
    Result = try
        case daemon_env(source_update_fun, undefined) of
            Fun when is_function(Fun, 1) -> Fun(Action);
            _ -> run_real_update(Action)
        end
    catch Class:Reason:Stack -> {error, {source_update_crash, Class, Reason, Stack}}
    end,
    case Result of
        ok -> ok;
        {error, Failure} ->
            logger:warning(#{event => knowledge_refresh_failed, source => Action, reason => Failure})
    end,
    Result.

run_real_update(nodes) -> wfcli_worldstate:update_nodes();
run_real_update(languages) -> wfcli_worldstate:update_languages();
run_real_update(manifest) -> wfcli_worldstate:update_manifest();
run_real_update(exports) -> wfcli_worldstate:update_all_exports();
run_real_update({exports, Files}) -> wfcli_worldstate:update_exports(Files);
run_real_update(recipes) -> wfcli_worldstate:update_export("ExportRecipes_en.json");
run_real_update(upgrades) -> wfcli_worldstate:update_export("ExportUpgrades_en.json");
run_real_update(weapons) -> wfcli_worldstate:update_export("ExportWeapons_en.json");
run_real_update(warframes) -> wfcli_worldstate:update_export("ExportWarframes_en.json");
run_real_update(resources) -> wfcli_worldstate:update_export("ExportResources_en.json");
run_real_update(wfcd) -> wfcli_wfcd:update();
run_real_update(star_chart) -> wfcli_star_chart:update();
run_real_update(Action) -> {error, {unknown_source_update, Action}}.

maybe_enqueue_stale_refresh(State) ->
    case has_refresh_job(State) of
        true -> State;
        false ->
            Selections = stale_selections(
                           maps:get(max_age_seconds, State),
                           erlang:system_time(second)),
            case Selections of
                [] -> State;
                _ -> enqueue(#{kind => periodic, selections => Selections, reply => none},
                             State)
            end
    end.

has_refresh_job(State) ->
    is_refresh(maps:get(current, State, undefined)) orelse
    lists:any(fun is_refresh/1, queue:to_list(maps:get(queue, State))).

is_refresh(#{job := Job}) -> is_refresh(Job);
is_refresh(#{kind := Kind}) when Kind =:= periodic; Kind =:= refresh -> true;
is_refresh(_) -> false.

stale_selections(MaxAge, Now) ->
    Candidates = [
        {nodes, stale_file(managed_path("solNodes.json"), MaxAge, Now)},
        {languages, stale_file(managed_path("languages.json"), MaxAge, Now)},
        {exports, exports_stale(MaxAge, Now)},
        {star_chart, stale_file(wfcli_star_chart:source(), MaxAge, Now)},
        {wfcd, stale_wfcd(wfcli_knowledge:wfcd_source(undefined), MaxAge, Now)
               orelse stale_wfcd(wfcli_item_catalog:source(), MaxAge, Now)}
    ],
    [Selection || {Selection, true} <- Candidates].

exports_stale(MaxAge, Now) ->
    Sources = wfcli_exports:item_sources(undefined, wfcli_worldstate:export_files()),
    lists:any(fun({_File, Path}) -> stale_file(Path, MaxAge, Now) end, Sources).

managed_path(Name) ->
    Paths = wfcli_worldstate:metadata_paths(Name),
    case [Path || Path <- Paths, filelib:is_file(Path)] of
        [Path | _] -> Path;
        [] ->
            case Paths of
                [Path | _] -> Path;
                [] -> wfcli_paths:cache_file(Name)
            end
    end.

stale_file(Path, MaxAge, Now) ->
    case file:read_file_info(Path, [{time, posix}]) of
        {ok, #file_info{mtime = Modified}} when is_integer(Modified) ->
            Now - Modified >= MaxAge;
        _ -> true
    end.

stale_wfcd(Path, MaxAge, Now) ->
    case file:read_file(Path) of
        {ok, Body} ->
            try jsone:decode(Body, [{object_format, map}]) of
                #{<<"fetchedAt">> := FetchedAt, <<"entries">> := Entries}
                  when is_integer(FetchedAt), is_list(Entries) ->
                    Now - FetchedAt >= MaxAge;
                _ -> true
            catch _:_ -> true
            end;
        {error, _} -> true
    end.

initial_refresh_delay(Interval) when is_integer(Interval), Interval > 0 ->
    min(60000, Interval);
initial_refresh_delay(_Interval) ->
    0.

ensure_refresh_timer(#{refresh_timer := Timer} = State)
  when is_reference(Timer) ->
    State;
ensure_refresh_timer(State) ->
    schedule_refresh(State, maps:get(refresh_interval_ms, State)).

schedule_refresh(State, Delay) when is_integer(Delay), Delay > 0 ->
    Timer = erlang:send_after(Delay, self(), refresh_stale_sources),
    State#{refresh_timer => Timer};
schedule_refresh(State, _Delay) ->
    State#{refresh_timer => undefined}.

cancel_refresh_timer(Timer) when is_reference(Timer) ->
    _ = erlang:cancel_timer(Timer),
    ok;
cancel_refresh_timer(_) ->
    ok.

daemon_env(Key, Default) ->
    application:get_env(retinue_daemon, Key, Default).

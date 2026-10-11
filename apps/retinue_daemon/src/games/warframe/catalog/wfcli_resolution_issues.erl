%%%-------------------------------------------------------------------
%% Current identity, metadata, and asset-resolution failures.
%%%-------------------------------------------------------------------
-module(wfcli_resolution_issues).

-behaviour(gen_server).

-export([start_link/0, reconcile/2, refresh/0, list/0, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(SCHEMA, 2).
-define(PERSIST_DELAY_MS, 250).

-doc "Start current resolution-issue registry.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() -> gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Replace all current issues reported by one complete projection scope.".
-spec reconcile(binary(), [map()]) -> ok.
reconcile(Scope, Issues) when is_binary(Scope), is_list(Issues) ->
    gen_server:call(?SERVER, {reconcile, Scope, Issues}).

-doc "Re-audit current player equipment against current metadata sources.".
-spec refresh() -> ok | {error, term()}.
refresh() -> gen_server:call(?SERVER, refresh, 120000).

-doc "Return sorted current resolution issues.".
-spec list() -> [map()].
list() -> gen_server:call(?SERVER, list).

-doc "Return registry count, persistence path, and last refresh failure.".
-spec status() -> map().
status() -> gen_server:call(?SERVER, status).

init([]) ->
    Path = path(),
    Issues = load(Path),
    {Subscription, Snapshot} = subscribe_player(),
    self() ! {refresh, Snapshot},
    {ok, #{schema => ?SCHEMA, path => Path, issues => Issues,
           subscription => Subscription,
           dirty => false, persist_timer => undefined, refresh_error => undefined}}.

handle_call({reconcile, Scope, Issues}, _From, State) ->
    {reply, ok, apply_reconcile(Scope, Issues, State)};
handle_call(refresh, _From, State) ->
    case audit(wfcli_player_service:snapshot()) of
        {ok, Issues} -> {reply, ok, apply_reconcile(<<"player_metadata">>, Issues,
                                                   State#{refresh_error => undefined})};
        {error, Reason} -> {reply, {error, Reason}, State#{refresh_error => Reason}}
    end;
handle_call(list, _From, State) ->
    {reply, sorted(maps:values(maps:get(issues, State))), State};
handle_call(status, _From, State) ->
    {reply, #{schema => ?SCHEMA,
              count => map_size(maps:get(issues, State)),
              path => maps:get(path, State),
              refresh_error => maps:get(refresh_error, State)}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

handle_cast(_Message, State) -> {noreply, State}.

handle_info({refresh, Snapshot}, State) ->
    {noreply, refresh_snapshot(Snapshot, State)};
handle_info({wfcli_player, Ref, _Source, Snapshot},
            State = #{subscription := Ref}) ->
    {noreply, refresh_snapshot(Snapshot, State)};
handle_info(refresh_current, State) ->
    {noreply, refresh_snapshot(wfcli_player_service:snapshot(), State)};
handle_info(persist, State) ->
    {noreply, persist(State#{persist_timer => undefined})};
handle_info(_Message, State) -> {noreply, State}.

terminate(_Reason, State) ->
    cancel_timer(maps:get(persist_timer, State, undefined)),
    _ = persist(State#{persist_timer => undefined}),
    ok.

code_change(_OldVsn, State, _Extra) ->
    Current = #{schema => ?SCHEMA,
                path => maps:get(path, State, path()),
                issues => maps:get(issues, State, #{}),
                subscription => maps:get(subscription, State, undefined),
                dirty => maps:get(dirty, State, false),
                persist_timer => maps:get(persist_timer, State, undefined),
                refresh_error => maps:get(refresh_error, State, undefined)},
    case maps:get(schema, State, undefined) of
        ?SCHEMA -> {ok, Current};
        _OldSchema ->
            self() ! refresh_current,
            {ok, mark_dirty(Current#{issues => #{}})}
    end.

subscribe_player() ->
    case wfcli_player_service:subscribe(self()) of
        {ok, Ref, Snapshot} -> {Ref, Snapshot};
        {error, Reason} -> {undefined, #{revision => 0, updated_at => undefined,
                                        data => #{<<"resolution_error">> => Reason}}}
    end.

refresh_snapshot(Snapshot, State) ->
    case audit(Snapshot) of
        {ok, Issues} -> apply_reconcile(<<"player_metadata">>, Issues,
                                       State#{refresh_error => undefined});
        {error, Reason} -> State#{refresh_error => Reason}
    end.

audit(Snapshot) ->
    case wfcli_item_catalog:load() of
        {ok, Catalog, Meta} ->
            try
                {BuildView, BuildIssues} =
                    wfcli_build_equipment:from_snapshot_with_issues(Snapshot, Catalog),
                Views = [BuildView,
                         view(wfcli_player_views:inventory(Snapshot, Catalog)),
                         view(wfcli_player_views:foundry(Snapshot, Catalog)),
                         view(wfcli_player_views:mastery(Snapshot, Catalog))],
                Issues0 = BuildIssues ++ wfcli_resolution_audit:scan(Views),
                Revision = maps:get(version, Meta, <<"unknown">>),
                {ok, [Issue#{<<"catalog_revision">> => Revision}
                      || Issue <- unique_issues(Issues0)]}
            catch Class:Reason:Stack ->
                {error, {resolution_audit_crash, Class, Reason, Stack}}
            end;
        {error, Reason} ->
            {ok, [#{<<"kind">> => <<"source_health">>,
                    <<"identity">> => <<"wfcd-items">>,
                    <<"reason">> => error_text(Reason),
                    <<"fallback">> => <<"WFCD item catalog">>,
                    <<"attempts">> => [<<"wfcd">>]}]}
    end.

apply_reconcile(Scope, Issues0, State) ->
    Now = erlang:system_time(millisecond),
    Existing = maps:get(issues, State),
    Retained = maps:filter(
                 fun(_Key, Issue) -> maps:get(<<"scope">>, Issue, <<>>) =/= Scope end,
                 Existing),
    Incoming = maps:from_list(
                 [{issue_key(Issue), Issue}
                  || Raw <- Issues0,
                     Issue <- [normalize_issue(Raw, Scope)],
                     Issue =/= undefined]),
    Current = maps:fold(
      fun(Key, Issue, Acc) ->
          Previous = maps:get(Key, Existing, #{}),
          FirstSeen = maps:get(<<"first_seen">>, Previous, Now),
          Count = maps:get(<<"count">>, Previous, 0) + 1,
          Acc#{Key => Issue#{<<"first_seen">> => FirstSeen,
                            <<"last_seen">> => Now,
                            <<"count">> => Count}}
      end, Retained, Incoming),
    case Current =:= Existing of
        true -> State;
        false -> mark_dirty(State#{issues => Current})
    end.

normalize_issue(Issue, Scope) when is_map(Issue) ->
    Kind = maps:get(<<"kind">>, Issue, undefined),
    Identity = maps:get(<<"identity">>, Issue, undefined),
    case is_binary(Kind) andalso is_binary(Identity) andalso
         byte_size(Kind) > 0 andalso byte_size(Identity) > 0 of
        true ->
            Allowed = [<<"kind">>, <<"identity">>, <<"reason">>, <<"fallback">>,
                       <<"class">>, <<"collection">>, <<"attempts">>,
                       <<"catalog_revision">>],
            (maps:with(Allowed, Issue))#{<<"scope">> => Scope};
        false -> undefined
    end;
normalize_issue(_Issue, _Scope) -> undefined.

issue_key(Issue) ->
    {maps:get(<<"scope">>, Issue, <<>>), maps:get(<<"kind">>, Issue),
     maps:get(<<"identity">>, Issue)}.

sorted(Issues) ->
    lists:sort(
      fun(A, B) ->
          {maps:get(<<"kind">>, A), maps:get(<<"identity">>, A)} =<
              {maps:get(<<"kind">>, B), maps:get(<<"identity">>, B)}
      end, Issues).

mark_dirty(State = #{persist_timer := undefined}) ->
    Timer = erlang:send_after(?PERSIST_DELAY_MS, self(), persist),
    State#{dirty => true, persist_timer => Timer};
mark_dirty(State) -> State#{dirty => true}.

persist(State = #{dirty := false}) -> State;
persist(State) ->
    Path = maps:get(path, State),
    Result = case map_size(maps:get(issues, State)) of
        0 -> delete(Path);
        _ -> write(Path, sorted(maps:values(maps:get(issues, State))))
    end,
    case Result of
        ok -> State#{dirty => false};
        {error, Reason} ->
            logger:warning("resolution issue registry write failed: ~p", [Reason]),
            Timer = erlang:send_after(5000, self(), persist),
            State#{persist_timer => Timer}
    end.

write(Path, Issues) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            Data = jsone:encode(#{<<"schema">> => ?SCHEMA, <<"issues">> => Issues}),
            case file:write_file(Temp, Data, [binary]) of
                ok ->
                    _ = file:change_mode(Temp, 8#600),
                    case file:rename(Temp, Path) of
                        ok -> file:change_mode(Path, 8#600);
                        {error, _Reason} = Error -> Error
                    end;
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

delete(Path) ->
    case file:delete(Path) of
        ok -> ok;
        {error, enoent} -> ok;
        {error, _Reason} = Error -> Error
    end.

load(Path) ->
    case file:read_file(Path) of
        {ok, Data} ->
            try jsone:decode(Data, [{object_format, map}]) of
                #{<<"schema">> := ?SCHEMA, <<"issues">> := Issues}
                  when is_list(Issues) ->
                    maps:from_list(
                      [{issue_key(Issue), Issue}
                       || Issue <- Issues, is_map(Issue),
                          maps:is_key(<<"kind">>, Issue),
                          maps:is_key(<<"identity">>, Issue)]);
                _ -> #{}
            catch _:_ -> #{}
            end;
        {error, _Reason} -> #{}
    end.

path() ->
    case application:get_env(retinue_daemon, resolution_issues_file) of
        {ok, Path} -> Path;
        undefined -> wfcli_paths:state_file("resolution-issues.json")
    end.

cancel_timer(undefined) -> ok;
cancel_timer(Timer) -> _ = erlang:cancel_timer(Timer), ok.

error_text(Reason) -> iolist_to_binary(io_lib:format("~p", [Reason])).

view({ok, Value}) when is_map(Value) -> Value;
view(_Result) -> #{}.

unique_issues(Issues) ->
    maps:values(maps:from_list(
                  [{{maps:get(<<"kind">>, Issue), maps:get(<<"identity">>, Issue)}, Issue}
                   || Issue <- Issues])).

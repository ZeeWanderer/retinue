%%%-------------------------------------------------------------------
%% Daemon-owned notification policy and worldstate watch.
%%%-------------------------------------------------------------------
-module(wfcli_notification_service).

-behaviour(gen_server).

-export([start_link/0, settings/0, update/1, gui_connected/1,
         gui_disconnected/1, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-ifdef(TEST).
-export([select_new/4, matches_filter/2, notification_text/1,
         notification_args/1]).
-endif.

-define(SERVER, ?MODULE).
-define(RETRY_MS, 1000).
-define(SEEN_TTL_MS, 10800000).

-type mode() :: off | session | persistent.
-type state() :: map().

-doc "Start persisted notification policy and its optional worldstate watch.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Return JSON-safe notification settings.".
-spec settings() -> map().
settings() ->
    gen_server:call(?SERVER, settings).

-doc "Merge and persist JSON-safe notification settings.".
-spec update(map()) -> {ok, map()} | {error, term()}.
update(Patch) when is_map(Patch) ->
    gen_server:call(?SERVER, {update, Patch}).

-doc "Activate session notification policy for one GUI connection.".
-spec gui_connected(pid()) -> ok.
gui_connected(Pid) when is_pid(Pid) ->
    gen_server:cast(?SERVER, {gui_connected, Pid}).

-doc "Release one GUI connection from session notification policy.".
-spec gui_disconnected(pid()) -> ok.
gui_disconnected(Pid) when is_pid(Pid) ->
    gen_server:cast(?SERVER, {gui_disconnected, Pid}).

-doc "Return notification mode, watch, and connected-GUI state.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-spec init([]) -> {ok, state()}.
init([]) ->
    Path = settings_path(),
    Settings = load_settings(Path),
    State = #{path => Path, settings => Settings,
              watch_ref => undefined, worldstate_monitor => undefined,
              retry_timer => undefined, initialized => false, seen => #{},
              gui_clients => #{}, gui_monitors => #{}},
    {ok, reconcile_watch(State)}.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call(settings, _From, State) ->
    {reply, maps:get(settings, State), State};
handle_call(status, _From, State) ->
    Fissures = maps:get(<<"fissures">>, maps:get(settings, State)),
    {reply, #{mode => mode_atom(maps:get(<<"mode">>, Fissures)),
              watching => maps:get(watch_ref, State) =/= undefined,
              gui_clients => map_size(maps:get(gui_clients, State))}, State};
handle_call({update, Patch}, _From, State) ->
    case merge_settings(maps:get(settings, State), Patch) of
        {ok, Settings} ->
            case persist_settings(maps:get(path, State), Settings) of
                ok ->
                    State1 = reconcile_watch(State#{settings => Settings}),
                    {reply, {ok, Settings}, State1};
                {error, Reason} ->
                    {reply, {error, {notification_settings_write_failed, Reason}}, State}
            end;
        {error, _Reason} = Error ->
            {reply, Error, State}
    end;
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast({gui_connected, Pid}, State) ->
    {noreply, reconcile_watch(add_gui(Pid, State))};
handle_cast({gui_disconnected, Pid}, State) ->
    {noreply, reconcile_watch(remove_gui(Pid, State))};
handle_cast(_Message, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info({retinue_daemon, Ref, {ok, Update}}, State = #{watch_ref := Ref}) ->
    {noreply, handle_worldstate(Update, State)};
handle_info({retinue_daemon, Ref, {error, Reason}}, State = #{watch_ref := Ref}) ->
    logger:warning("fissure notification watch failed: ~p", [Reason]),
    {noreply, State};
handle_info(retry_watch, State) ->
    {noreply, reconcile_watch(State#{retry_timer => undefined})};
handle_info({'DOWN', Monitor, process, _Pid, _Reason},
            State = #{worldstate_monitor := Monitor}) ->
    State1 = State#{watch_ref => undefined, worldstate_monitor => undefined,
                    initialized => false},
    {noreply, schedule_retry(State1)};
handle_info({'DOWN', Monitor, process, _Pid, _Reason}, State) ->
    case maps:take(Monitor, maps:get(gui_monitors, State)) of
        error -> {noreply, State};
        {Gui, Monitors} ->
            Clients = maps:remove(Gui, maps:get(gui_clients, State)),
            {noreply, reconcile_watch(
                        State#{gui_clients => Clients, gui_monitors => Monitors})}
    end;
handle_info(_Message, State) ->
    {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, State) ->
    _ = stop_watch(State),
    ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    Defaults = #{path => settings_path(), settings => default_settings(),
                 watch_ref => undefined, worldstate_monitor => undefined,
                 retry_timer => undefined, initialized => false, seen => #{},
                 gui_clients => #{}, gui_monitors => #{}},
    {ok, reconcile_watch(maps:merge(Defaults, State))}.

reconcile_watch(State) ->
    case should_watch(State) of
        true -> ensure_watch(State);
        false -> stop_watch(State)
    end.

should_watch(State) ->
    Fissures = maps:get(<<"fissures">>, maps:get(settings, State)),
    case mode_atom(maps:get(<<"mode">>, Fissures)) of
        persistent -> true;
        session -> map_size(maps:get(gui_clients, State)) > 0;
        off -> false
    end.

ensure_watch(State = #{watch_ref := Ref}) when Ref =/= undefined -> State;
ensure_watch(State) ->
    case whereis(wfcli_worldstate_service) of
        undefined -> schedule_retry(State);
        Service ->
            Monitor = erlang:monitor(process, Service),
            Request = #{source => worldstate, mode => list, interval => 60,
                        specs => [#{label => fissures, type_filter => fissure}],
                        opts => #{ttl => 60, resolve_items => true}},
            case subscribe_watch(Request) of
                {ok, Ref} ->
                    cancel_timer(maps:get(retry_timer, State)),
                    State#{watch_ref => Ref, worldstate_monitor => Monitor,
                           retry_timer => undefined, initialized => false, seen => #{}};
                Error ->
                    erlang:demonitor(Monitor, [flush]),
                    logger:warning("could not start fissure notification watch: ~p", [Error]),
                    schedule_retry(State)
            end
    end.

stop_watch(State) ->
    case maps:get(watch_ref, State, undefined) of
        undefined -> ok;
        Ref -> safe_unsubscribe(Ref)
    end,
    demonitor_ref(maps:get(worldstate_monitor, State, undefined)),
    cancel_timer(maps:get(retry_timer, State, undefined)),
    State#{watch_ref => undefined, worldstate_monitor => undefined,
           retry_timer => undefined, initialized => false, seen => #{}}.

schedule_retry(State = #{retry_timer := Timer}) when Timer =/= undefined -> State;
schedule_retry(State) ->
    case should_watch(State) of
        true -> State#{retry_timer => erlang:send_after(?RETRY_MS, self(), retry_watch)};
        false -> State
    end.

handle_worldstate(#{specs := [Spec | _]}, State) ->
    Entries = maps:get(entries, Spec, []),
    Opts = maps:get(opts, Spec, #{}),
    Fissures = [wfcli_activity_view:project_fissure(Entry, Opts)
                || Entry <- Entries, is_map(Entry)],
    Now = erlang:system_time(millisecond),
    {New, Seen} = select_new(Fissures, maps:get(seen, State),
                             not maps:get(initialized, State), Now),
    Filters = maps:get(<<"filters">>,
                       maps:get(<<"fissures">>, maps:get(settings, State))),
    notify([Fissure || Fissure <- New, matches_any(Fissure, Filters)]),
    State#{seen => Seen, initialized => true};
handle_worldstate(_Update, State) -> State.

select_new(Fissures, Seen0, Initial, Now) ->
    Seen = maps:filter(fun(_Id, At) -> Now - At < ?SEEN_TTL_MS end, Seen0),
    {New, Next} = lists:foldl(
      fun(Fissure, {Fresh, Acc}) ->
          Id = fissure_id(Fissure),
          IsNew = not Initial andalso not maps:is_key(Id, Acc),
          Fresh1 = case IsNew of true -> [Fissure | Fresh]; false -> Fresh end,
          {Fresh1, Acc#{Id => Now}}
      end,
      {[], Seen}, Fissures),
    {lists:reverse(New), Next}.

fissure_id(Fissure) ->
    case maps:get(<<"id">>, Fissure, <<>>) of
        <<>> -> {maps:get(<<"tier">>, Fissure, <<>>),
                 maps:get(<<"mission">>, Fissure, <<>>),
                 maps:get(<<"node">>, Fissure, <<>>),
                 maps:get(<<"expiry">>, Fissure, <<>>)};
        Id -> Id
    end.

matches_any(_Fissure, []) -> false;
matches_any(Fissure, Filters) ->
    lists:any(fun(Filter) -> matches_filter(Fissure, Filter) end, Filters).

matches_filter(Fissure, Filter) ->
    field_matches(maps:get(<<"type">>, Filter, <<"all">>),
                  maps:get(<<"tier">>, Fissure, <<>>)) andalso
    field_matches(maps:get(<<"mode">>, Filter, <<"all">>),
                  maps:get(<<"mission">>, Fissure, <<>>)) andalso
    location_matches(maps:get(<<"location">>, Filter, <<"all">>),
                     maps:get(<<"node">>, Fissure, <<>>)) andalso
    steel_matches(maps:get(<<"steelPath">>, Filter, <<"all">>),
                  maps:get(<<"hard">>, Fissure, false)).

field_matches(Value, Actual) ->
    Normalized = lower(Value),
    Normalized =:= <<"all">> orelse Normalized =:= lower(Actual).

location_matches(Value, Node) ->
    Normalized = lower(Value),
    Normalized =:= <<"all">> orelse
        binary:match(lower(Node), Normalized) =/= nomatch.

steel_matches(Value, Hard) ->
    case lower(Value) of
        <<"all">> -> true;
        <<"steelpath">> -> Hard =:= true;
        <<"steel">> -> Hard =:= true;
        <<"hard">> -> Hard =:= true;
        <<"normal">> -> Hard =/= true;
        _ -> false
    end.

notify([]) -> ok;
notify(Fissures) ->
    _ = spawn(fun() -> lists:foreach(fun deliver/1, Fissures) end),
    ok.

deliver(Fissure) ->
    Result = case application:get_env(retinue_daemon, notification_fun) of
        {ok, Fun} when is_function(Fun, 1) -> Fun(Fissure);
        _ -> desktop_notification(Fissure)
    end,
    case Result of
        ok -> ok;
        {error, Reason} -> logger:warning("desktop notification failed: ~p", [Reason]);
        _ -> ok
    end.

desktop_notification(Fissure) ->
    case os:find_executable("notify-send") of
        false -> {error, notify_send_not_found};
        Executable ->
            Args = notification_args(Fissure),
            try open_port({spawn_executable, Executable},
                          [binary, exit_status, stderr_to_stdout, {args, Args}]) of
                Port -> await_port(Port, <<>>)
            catch
                error:Reason -> {error, Reason}
            end
    end.

notification_args(Fissure) ->
    ["--app-name=wfgui", "--icon=wfgui",
     "--hint=string:desktop-entry:wfgui",
     "Warframe fissure", binary_to_list(notification_text(Fissure))].

await_port(Port, Output) ->
    receive
        {Port, {data, Data}} -> await_port(Port, <<Output/binary, Data/binary>>);
        {Port, {exit_status, 0}} -> ok;
        {Port, {exit_status, Status}} -> {error, {notify_send_exit, Status, Output}}
    after 5000 ->
        safe_port_close(Port),
        {error, notify_send_timeout}
    end.

notification_text(Fissure) ->
    Steel = case maps:get(<<"hard">>, Fissure, false) of
        true -> <<"Steel Path ">>;
        false -> <<>>
    end,
    Tier = value(maps:get(<<"tier">>, Fissure, <<"Unknown">>)),
    Mission = value(maps:get(<<"mission">>, Fissure, <<"Unknown mission">>)),
    Node = value(maps:get(<<"node">>, Fissure, <<"Unknown node">>)),
    Expiry = value(maps:get(<<"expiry">>, Fissure, <<>>)),
    Suffix = case Expiry of <<>> -> <<>>; _ -> <<". Until ", Expiry/binary>> end,
    <<"New ", Steel/binary, Tier/binary, " fissure: ", Mission/binary,
      " at ", Node/binary, Suffix/binary>>.

add_gui(Pid, State) ->
    case maps:is_key(Pid, maps:get(gui_clients, State)) of
        true -> State;
        false ->
            Monitor = erlang:monitor(process, Pid),
            State#{gui_clients => (maps:get(gui_clients, State))#{Pid => Monitor},
                   gui_monitors => (maps:get(gui_monitors, State))#{Monitor => Pid}}
    end.

remove_gui(Pid, State) ->
    case maps:take(Pid, maps:get(gui_clients, State)) of
        error -> State;
        {Monitor, Clients} ->
            erlang:demonitor(Monitor, [flush]),
            State#{gui_clients => Clients,
                   gui_monitors => maps:remove(Monitor, maps:get(gui_monitors, State))}
    end.

merge_settings(Current, Patch) ->
    CurrentFissures = maps:get(<<"fissures">>, Current),
    FissurePatch = maps:get(<<"fissures">>, Patch, #{}),
    case is_map(FissurePatch) of
        false -> {error, invalid_notification_settings};
        true ->
            Mode = maps:get(<<"mode">>, FissurePatch,
                            maps:get(<<"mode">>, CurrentFissures)),
            Filters = maps:get(<<"filters">>, FissurePatch,
                               maps:get(<<"filters">>, CurrentFissures)),
            case {valid_mode(Mode), normalize_filters(Filters)} of
                {true, {ok, Normalized}} ->
                    {ok, #{<<"fissures">> =>
                               #{<<"mode">> => mode_binary(mode_atom(Mode)),
                                 <<"filters">> => Normalized}}};
                {false, _} -> {error, invalid_notification_mode};
                {_, {error, _Reason} = Error} -> Error
            end
    end.

normalize_filters(Filters) when is_list(Filters) ->
    Valid = [normalize_filter(Filter) || Filter <- Filters, is_map(Filter)],
    case {Filters, Valid} of
        {[], []} -> {ok, [default_filter()]};
        {_, Normalized} when length(Normalized) =:= length(Filters) -> {ok, Normalized};
        _ -> {error, invalid_notification_filters}
    end;
normalize_filters(_Filters) -> {error, invalid_notification_filters}.

normalize_filter(Filter) ->
    #{<<"type">> => lower(maps:get(<<"type">>, Filter, <<"all">>)),
      <<"mode">> => lower(maps:get(<<"mode">>, Filter, <<"all">>)),
      <<"location">> => lower(maps:get(<<"location">>, Filter, <<"all">>)),
      <<"steelPath">> => lower(maps:get(
                                   <<"steelPath">>, Filter,
                                   maps:get(<<"steel_path">>, Filter, <<"all">>)))}.

default_settings() ->
    #{<<"fissures">> => #{<<"mode">> => <<"off">>,
                             <<"filters">> => [default_filter()]}}.

default_filter() ->
    #{<<"type">> => <<"all">>, <<"mode">> => <<"all">>,
      <<"location">> => <<"all">>, <<"steelPath">> => <<"all">>}.

load_settings(Path) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            try jsone:decode(Binary, [{object_format, map}]) of
                Settings when is_map(Settings) ->
                    case merge_settings(default_settings(), Settings) of
                        {ok, Normalized} -> Normalized;
                        {error, Reason} ->
                            logger:warning("invalid notification settings: ~p", [Reason]),
                            default_settings()
                    end
            catch
                error:Reason ->
                    logger:warning("could not decode notification settings: ~p", [Reason]),
                    default_settings()
            end;
        {error, enoent} -> default_settings();
        {error, Reason} ->
            logger:warning("could not read notification settings: ~p", [Reason]),
            default_settings()
    end.

persist_settings(Path, Settings) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            case file:write_file(Temp, jsone:encode(Settings), [binary]) of
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

settings_path() ->
    case application:get_env(retinue_daemon, notification_settings_file) of
        {ok, Path} -> Path;
        undefined -> wfcli_paths:config_file("notifications.json")
    end.

-spec valid_mode(term()) -> boolean().
valid_mode(Value) ->
    lists:member(mode_atom(Value), [off, session, persistent]).

-spec mode_atom(term()) -> mode() | invalid.
mode_atom(off) -> off;
mode_atom(session) -> session;
mode_atom(persistent) -> persistent;
mode_atom(<<"off">>) -> off;
mode_atom(<<"session">>) -> session;
mode_atom(<<"persistent">>) -> persistent;
mode_atom(_) -> invalid.

mode_binary(off) -> <<"off">>;
mode_binary(session) -> <<"session">>;
mode_binary(persistent) -> <<"persistent">>.

lower(Value) ->
    unicode:characters_to_binary(string:casefold(binary_to_list(value(Value)))).

value(Value) when is_binary(Value) -> Value;
value(Value) when is_atom(Value) -> atom_to_binary(Value);
value(Value) when is_list(Value) -> unicode:characters_to_binary(Value);
value(Value) -> iolist_to_binary(io_lib:format("~p", [Value])).

subscribe_watch(Request) ->
    try wfcli_worldstate_service:subscribe(self(), Request)
    catch Class:Reason -> {error, {Class, Reason}}
    end.

safe_unsubscribe(Ref) ->
    try wfcli_worldstate_service:unsubscribe(Ref)
    catch _Class:_Reason -> ok
    end.

safe_port_close(Port) ->
    try port_close(Port)
    catch error:badarg -> ok
    end.

demonitor_ref(undefined) -> ok;
demonitor_ref(Monitor) -> erlang:demonitor(Monitor, [flush]), ok.

cancel_timer(undefined) -> ok;
cancel_timer(Timer) -> _ = erlang:cancel_timer(Timer), ok.

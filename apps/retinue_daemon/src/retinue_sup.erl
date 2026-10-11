-module(retinue_sup).
-behaviour(supervisor).

-export([start_link/0, start_subtree/1, init/1, child_specs/0, worker_specs/0, owner/1,
         restart_child/1,
         ensure_children/0, ensure_children/2]).

-define(CHILD_SHUTDOWN_TIMEOUT_MS, 5000).

start_link() -> supervisor:start_link({local, ?MODULE}, ?MODULE, []).

start_subtree(Module) ->
    case whereis(Module) of
        undefined ->
            case await_departing_children(Module) of
                ok -> supervisor:start_link({local, Module}, Module, []);
                Error -> Error
            end;
        Pid -> {error, {already_started, Pid}}
    end.

%% A killed supervisor can exit before its trapping children unregister.
await_departing_children(Module) ->
    Monitors = lists:filtermap(fun(#{id := Name}) ->
        case whereis(Name) of
            undefined -> false;
            Pid ->
                case process_info(Pid, {dictionary, '$ancestors'}) of
                    {{dictionary, '$ancestors'}, [Module | _]} ->
                        {true, {Name, Pid, monitor(process, Pid)}};
                    _ -> false
                end
        end
    end, Module:child_specs()),
    Deadline = erlang:monotonic_time(millisecond) + ?CHILD_SHUTDOWN_TIMEOUT_MS,
    try await_departing_children(Monitors, Deadline)
    after
        lists:foreach(fun({_, _, Ref}) -> demonitor(Ref, [flush]) end, Monitors)
    end.

await_departing_children([], _Deadline) -> ok;
await_departing_children([{Name, Pid, Ref} | Rest], Deadline) ->
    Timeout = max(0, Deadline - erlang:monotonic_time(millisecond)),
    receive
        {'DOWN', Ref, process, Pid, _} -> await_departing_children(Rest, Deadline)
    after Timeout ->
        {error, {child_shutdown_timeout, Name, Pid}}
    end.

init([]) ->
    Children = case application:get_env(retinue_daemon, daemon_enabled, false) of
        true -> child_specs();
        _ -> []
    end,
    {ok, {#{strategy => rest_for_one, intensity => 3, period => 10}, Children}}.

child_specs() ->
    [worker(retinue_activity), subtree(retinue_games_sup),
     worker(retinue_local_api), worker(retinue_daemon)].

worker_specs() ->
    wfcli_warframe_sup:child_specs() ++ retinue_poe2_sup:child_specs() ++
        [worker(retinue_activity), worker(retinue_local_api), worker(retinue_daemon)].

owner(Child) ->
    case lists:keyfind(Child, 1, [{maps:get(id, Spec), wfcli_warframe_sup}
                                || Spec <- wfcli_warframe_sup:child_specs()] ++
                               [{maps:get(id, Spec), retinue_poe2_sup}
                                || Spec <- retinue_poe2_sup:child_specs()]) of
        {_, Supervisor} -> Supervisor;
        false -> ?MODULE
    end.

restart_child(Child) ->
    Supervisor = owner(Child),
    %% Only activity recovery invalidates every held lifetime. Socket hot updates
    %% must preserve the daemon that owns the pending caller reply.
    Children = case Child of
        retinue_activity -> [maps:get(id, Spec) || Spec <- child_specs()];
        _ -> [Child]
    end,
    case terminate_for_restart(Supervisor, lists:reverse(Children)) of
        ok -> restart_children(Supervisor, Children);
        Error -> Error
    end.

terminate_for_restart(_Supervisor, []) -> ok;
terminate_for_restart(Supervisor, [Child | Rest]) ->
    case supervisor:terminate_child(Supervisor, Child) of
        ok -> terminate_for_restart(Supervisor, Rest);
        {error, Reason} -> {error, {terminate_failed, Reason}}
    end.

restart_children(_Supervisor, []) -> ok;
restart_children(Supervisor, [Child | Rest]) ->
    case supervisor:restart_child(Supervisor, Child) of
        {ok, _Pid} -> restart_children(Supervisor, Rest);
        {ok, _Pid, _Info} -> restart_children(Supervisor, Rest);
        {error, Reason} -> {error, {restart_failed, Reason}}
    end.

ensure_children() ->
    case application:get_env(retinue_daemon, daemon_enabled, false) of
        true ->
            {ok, _} = application:ensure_all_started(gun),
            case ensure_children(?MODULE, child_specs()) of
                ok ->
                    ok = ensure_children(retinue_games_sup, retinue_games_sup:child_specs()),
                    ok = wfcli_warframe_sup:ensure_children(),
                    ensure_children(retinue_poe2_sup, retinue_poe2_sup:child_specs());
                Error -> Error
            end;
        _ -> ok
    end.

ensure_children(_, []) -> ok;
ensure_children(Supervisor, [Spec | Rest]) ->
    case supervisor:start_child(Supervisor, Spec) of
        {ok, _} -> ensure_children(Supervisor, Rest);
        {ok, _, _} -> ensure_children(Supervisor, Rest);
        {error, {already_started, _}} -> ensure_children(Supervisor, Rest);
        {error, already_present} -> ensure_children(Supervisor, Rest);
        {error, Reason} -> {error, {maps:get(id, Spec), Reason}}
    end.

worker(Module) ->
    #{id => Module, start => {Module, start_link, []}, restart => permanent,
      shutdown => ?CHILD_SHUTDOWN_TIMEOUT_MS, type => worker, modules => [Module]}.
subtree(Module) -> (worker(Module))#{type => supervisor, shutdown => infinity}.

%%%-------------------------------------------------------------------
%% EUnit coverage for daemon-owned player persistence and subscriptions.
%%%-------------------------------------------------------------------
-module(wf_player_service_eunit).

-include_lib("eunit/include/eunit.hrl").

player_service_test_() ->
    {setup,
     fun setup/0,
     fun cleanup/1,
     fun(_Path) -> [
         fun merges_source_owned_namespaces/0,
         fun notifies_and_releases_dead_subscribers/0,
         fun reloads_persisted_snapshot/0,
         fun publish_invalidates_derived_views/0,
         fun unchanged_publish_keeps_revision_and_views/0,
         fun unchanged_game_publish_refreshes_transient_state/0,
         fun game_stop_does_not_erase_cached_inventory/0,
         fun native_scopes_publish_one_canonical_revision/0,
         fun projects_subscriptions_before_delivery/0,
         fun status_includes_diagnostics_without_inventory/0,
         fun clear_advances_revision/0
     ] end}.

setup() ->
    Path = filename:join(
             "/tmp", "wfcli-player-" ++ integer_to_list(erlang:unique_integer([positive]))),
    application:set_env(retinue_daemon, player_cache, Path),
    {ok, _Pid} = wf_player_service:start_link(),
    Path.

cleanup(Path) ->
    case whereis(wf_player_service) of
        undefined -> ok;
        _Pid -> gen_server:stop(wf_player_service)
    end,
    application:unset_env(retinue_daemon, player_cache),
    _ = file:delete(Path ++ ".tmp"),
    _ = file:delete(Path),
    ok.

merges_source_owned_namespaces() ->
    {ok, _} = wf_player_service:publish(
                <<"inventory">>, #{<<"weapons">> => [<<"Braton">>]}),
    {ok, _} = wf_player_service:publish(
                <<"profile">>, #{<<"rank">> => 12}),
    Data = maps:get(data, wf_player_service:snapshot()),
    ?assert(maps:is_key(<<"inventory">>, Data)),
    ?assert(maps:is_key(<<"profile">>, Data)).

notifies_and_releases_dead_subscribers() ->
    Parent = self(),
    Client = spawn(fun() -> receive stop -> Parent ! stopped end end),
    {ok, _Ref, _} = wf_player_service:subscribe(Client),
    ?assertEqual(1, maps:get(subscribers, wf_player_service:status())),
    exit(Client, kill),
    await_subscribers(0, 50).

reloads_persisted_snapshot() ->
    Before = wf_player_service:snapshot(),
    ok = gen_server:stop(wf_player_service),
    {ok, _Pid} = wf_player_service:start_link(),
    After = wf_player_service:snapshot(),
    ?assertEqual(maps:get(data, Before), maps:get(data, After)).

publish_invalidates_derived_views() ->
    true = ets:insert(wf_player_view_cache, {test_view, cached}),
    {ok, _} = wf_player_service:publish(<<"profile">>, #{<<"rank">> => 13}),
    ?assertEqual([], ets:lookup(wf_player_view_cache, test_view)).

unchanged_publish_keeps_revision_and_views() ->
    Data = #{<<"value">> => 1},
    {ok, First} = wf_player_service:publish(<<"stable">>, Data),
    await_cache_clean(50),
    {ok, Ref, _} = wf_player_service:subscribe(self()),
    true = ets:insert(wf_player_view_cache, {stable_view, cached}),
    {ok, Second} = wf_player_service:publish(<<"stable">>, Data),
    ?assertEqual(maps:get(revision, First), maps:get(revision, Second)),
    ?assertEqual([{stable_view, cached}],
                 ets:lookup(wf_player_view_cache, stable_view)),
    ?assertMatch(#{cache_dirty := false, persist_timer := undefined},
                 sys:get_state(wf_player_service)),
    receive
        {wf_player, Ref, _Source, _Snapshot} -> error(unchanged_publish_notified)
    after 25 -> ok
    end,
    ok = wf_player_service:unsubscribe(Ref).

unchanged_game_publish_refreshes_transient_state() ->
    Game = #{<<"running">> => true},
    {ok, First} = wf_player_service:publish(<<"game">>, Game),
    _ = sys:replace_state(
          wf_player_service,
          fun(State) -> State#{game_active => false} end),
    {ok, Second} = wf_player_service:publish(<<"game">>, Game),
    ?assertEqual(maps:get(revision, First), maps:get(revision, Second)),
    ?assertEqual(true, maps:get(game_active, wf_player_service:status())).

game_stop_does_not_erase_cached_inventory() ->
    Inventory = #{<<"schema">> => 1, <<"sync">> => <<"abc">>},
    {ok, _} = wf_player_service:publish(<<"inventory">>, Inventory),
    {ok, _} = wf_player_service:publish(<<"game">>, #{<<"running">> => false}),
    Data = maps:get(data, wf_player_service:snapshot()),
    ?assertEqual(Inventory, maps:get(<<"inventory">>, Data)).

native_scopes_publish_one_canonical_revision() ->
    ok = wf_player_service:clear(),
    Full = wf_player_inventory_eunit:full(1, 12),
    Native = wf_player_inventory_eunit:native(2, 1, 17),
    {ok, Base} = wf_player_service:publish(<<"inventory_http">>, Full),
    {ok, Ref, _} = wf_player_service:subscribe(self()),
    {ok, Changed} = wf_player_service:publish(<<"inventory_native">>, Native),
    ?assertEqual(maps:get(revision, Base) + 1, maps:get(revision, Changed)),
    receive {wf_player, Ref, <<"inventory">>, Changed} -> ok
    after 1000 -> error(no_inventory_notification) end,
    true = ets:insert(wf_player_view_cache, {scope_view, cached}),
    {ok, Changed} = wf_player_service:publish(<<"inventory_http">>, Full),
    {ok, Changed} = wf_player_service:publish(<<"inventory_native">>, Native),
    ?assertEqual([{scope_view, cached}], ets:lookup(wf_player_view_cache, scope_view)),
    receive {wf_player, Ref, _, _} -> error(replay_notified) after 25 -> ok end,
    ok = gen_server:stop(wf_player_service),
    {ok, _} = wf_player_service:start_link(),
    {ok, Changed} = wf_player_service:publish(<<"inventory_http">>, Full),
    {ok, Changed} = wf_player_service:publish(<<"inventory_native">>, Native),
    ?assertEqual(Changed, wf_player_service:snapshot()).

projects_subscriptions_before_delivery() ->
    Game = #{<<"phase">> => <<"orbiter">>, <<"pid">> => 123, <<"private">> => true},
    {ok, Full} = wf_player_service:publish(<<"game">>, Game),
    {ok, HudRef, Hud} = wf_player_service:subscribe(self(), hud),
    ?assertEqual(#{<<"phase">> => <<"orbiter">>, <<"pid">> => 123},
                 maps:get(<<"game">>, maps:get(data, Hud))),
    ?assertNot(maps:is_key(<<"inventory">>, maps:get(data, Hud))),
    {ok, MetaRef, Meta} = wf_player_service:subscribe(self(), metadata),
    ?assertEqual(maps:remove(data, Full), Meta),
    {ok, Receipt} = wf_player_service:publish(<<"profile">>, #{<<"rank">> => 15}, metadata),
    Updated = wf_player_service:snapshot(),
    ?assertEqual(maps:remove(data, Updated), Receipt),
    receive {wf_player, MetaRef, <<"profile">>, Projected} ->
        ?assertEqual(maps:remove(data, Updated), Projected)
    after 1000 -> error(no_metadata) end,
    receive {wf_player, HudRef, _, _} -> error(unrelated_hud_update) after 25 -> ok end,
    ok = wf_player_service:unsubscribe(MetaRef),
    {ok, _} = wf_player_service:publish(<<"game">>, #{<<"phase">> => <<"stopped">>}),
    receive {wf_player, HudRef, <<"game">>, Stopped} ->
        ?assertEqual(#{<<"phase">> => <<"stopped">>}, maps:get(<<"game">>, maps:get(data, Stopped)))
    after 1000 -> error(no_hud_stop) end,
    ok = wf_player_service:clear(),
    receive {wf_player, HudRef, clear, Cleared} ->
        ?assertEqual(#{<<"game">> => #{}, <<"collector">> => #{}}, maps:get(data, Cleared))
    after 1000 -> error(no_hud_clear) end,
    ok = wf_player_service:unsubscribe(HudRef).

clear_advances_revision() ->
    Before = wf_player_service:snapshot(),
    {ok, Ref, _} = wf_player_service:subscribe(self()),
    ok = wf_player_service:clear(),
    receive
        {wf_player, Ref, clear, Cleared} ->
            ?assertEqual(maps:get(revision, Before) + 1,
                         maps:get(revision, Cleared)),
            ?assertEqual(#{}, maps:get(data, Cleared)),
            ?assert(is_integer(maps:get(updated_at, Cleared)))
    after 1000 ->
        error(clear_notification_missing)
    end,
    ok = wf_player_service:unsubscribe(Ref).

status_includes_diagnostics_without_inventory() ->
    Collector = #{<<"inventory_received_at">> => 123},
    Capture = #{<<"state">> => <<"armed">>, <<"directory">> => <<"next">>},
    Result = #{<<"state">> => <<"saved">>, <<"directory">> => <<"previous">>},
    {ok, _} = wf_player_service:publish(<<"collector">>, Collector),
    {ok, _} = wf_player_service:publish(<<"capture">>, Capture),
    {ok, _} = wf_player_service:publish(<<"capture_result">>, Result),
    Status = wf_player_service:status(),
    ?assertEqual(Collector, maps:get(collector, Status)),
    ?assertEqual(Capture, maps:get(capture, Status)),
    ?assertEqual(Result, maps:get(capture_result, Status)),
    ?assertNot(maps:is_key(inventory, Status)).

await_subscribers(Expected, 0) ->
    ?assertEqual(Expected, maps:get(subscribers, wf_player_service:status()));
await_subscribers(Expected, Attempts) ->
    case maps:get(subscribers, wf_player_service:status()) of
        Expected -> ok;
        _ -> timer:sleep(10), await_subscribers(Expected, Attempts - 1)
    end.

await_cache_clean(0) ->
    ?assertMatch(#{cache_dirty := false, persist_timer := undefined},
                 sys:get_state(wf_player_service));
await_cache_clean(Attempts) ->
    case sys:get_state(wf_player_service) of
        #{cache_dirty := false, persist_timer := undefined} -> ok;
        _ -> timer:sleep(10), await_cache_clean(Attempts - 1)
    end.

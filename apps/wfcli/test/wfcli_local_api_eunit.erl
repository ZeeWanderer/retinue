%%%-------------------------------------------------------------------
%% EUnit integration coverage for owner-only Unix socket handshake.
%%%-------------------------------------------------------------------
-module(wfcli_local_api_eunit).

-include_lib("eunit/include/eunit.hrl").

unix_socket_lifecycle_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(State) -> fun() -> lifecycle(State) end end}.

inventory_scopes_socket_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         Companion = connect_client(Path, <<"wfcompanion">>, #{<<"pid">> => 123}),
         Publish = #{<<"op">> => <<"publish">>, <<"dataset">> => <<"player">>},
         Full = wfcli_player_inventory_eunit:full(1, 12),
         ok = socket:send(Companion, wfcli_local_protocol:encode(Publish#{<<"id">> => 2,
                           <<"source">> => <<"inventory_http">>, <<"data">> => Full})),
         ?assertMatch(#{<<"ok">> := true}, receive_response(Companion, 2, <<>>)),
         Subscriber = connect_client(Path, <<"test">>, #{}),
         ok = socket:send(Subscriber, wfcli_local_protocol:encode(#{<<"id">> => 2,
             <<"op">> => <<"subscribe">>, <<"dataset">> => <<"player">>, <<"include_data">> => false})),
         ?assertMatch(#{<<"ok">> := true}, receive_response(Subscriber, 2, <<>>)),
         Native = wfcli_player_inventory_eunit:native(2, 1, 17),
         ok = socket:send(Companion, wfcli_local_protocol:encode(Publish#{<<"id">> => 3,
                           <<"source">> => <<"inventory_native">>, <<"data">> => Native})),
         ?assertMatch(#{<<"ok">> := true}, receive_response(Companion, 3, <<>>)),
         {ok, Line} = socket:recv(Subscriber, 0, 1000),
         ?assertMatch({ok, #{<<"event">> := <<"dataset">>, <<"source">> := <<"inventory">>}},
                      wfcli_local_protocol:decode(string:trim(Line))),
         Before = wfcli_player_service:snapshot(),
         ok = socket:send(Companion, wfcli_local_protocol:encode(Publish#{<<"id">> => 4,
                           <<"source">> => <<"inventory_http">>, <<"data">> => Full})),
         ?assertMatch(#{<<"ok">> := true}, receive_response(Companion, 4, <<>>)),
         ?assertEqual(Before, wfcli_player_service:snapshot()),
         ?assertEqual({error, timeout}, socket:recv(Subscriber, 0, 25)),
         ok = socket:send(Companion, wfcli_local_protocol:encode(Publish#{<<"id">> => 5,
                           <<"source">> => <<"inventory">>, <<"data">> => Full})),
         ?assertMatch(#{<<"ok">> := false}, receive_response(Companion, 5, <<>>)),
         ok = socket:close(Subscriber),
         ok = socket:close(Companion)
     end end}.

player_hud_subscription_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         Game = #{<<"pid">> => 123, <<"phase">> => <<"orbiter">>, <<"running">> => true},
         Collector = #{<<"debug_output_active">> => true, <<"debug_output_lines_observed">> => 42,
                       <<"inventory_updates">> => 9},
         {ok, _} = wfcli_player_service:publish(<<"game">>, Game),
         {ok, _} = wfcli_player_service:publish(<<"collector">>, Collector),
         Socket = connect_client(Path, <<"wfcompanion">>, #{<<"pid">> => 123}),
         Subscribe = #{<<"op">> => <<"subscribe">>, <<"id">> => 2,
                       <<"dataset">> => <<"player">>, <<"view">> => <<"hud">>},
         lists:foreach(fun(Invalid) ->
             ok = socket:send(Socket, wfcli_local_protocol:encode(Invalid)),
             ?assertMatch(#{<<"ok">> := false}, receive_response(Socket, 2, <<>>))
         end, [Subscribe#{<<"view">> => <<"unknown">>}, Subscribe#{<<"include_data">> => false}]),
         ok = socket:send(Socket, wfcli_local_protocol:encode(Subscribe)),
         Initial = receive_response(Socket, 2, <<>>),
         ?assertEqual(#{<<"game">> => maps:without([<<"running">>], Game),
                        <<"collector">> => maps:without([<<"inventory_updates">>], Collector)},
                      maps:get(<<"data">>, maps:get(<<"data">>, Initial))),
         Full = wfcli_player_inventory_eunit:full(1, 12),
         {ok, _} = wfcli_player_service:publish(<<"inventory_http">>, Full),
         ?assertEqual({error, timeout}, socket:recv(Socket, 0, 25)),
         ok = socket:send(Socket, wfcli_local_protocol:encode(
             #{<<"op">> => <<"get">>, <<"id">> => 3, <<"dataset">> => <<"player">>})),
         Reply = receive_response(Socket, 3, <<>>),
         ?assertEqual(Full, maps:get(<<"inventory_http">>,
                            maps:get(<<"data">>, maps:get(<<"data">>, Reply)))),
         {ok, _} = wfcli_player_service:publish(<<"collector">>, #{<<"debug_output_active">> => false}),
         Event = diagnostic_command(Socket),
         ?assertEqual(<<"collector">>, maps:get(<<"source">>, Event)),
         EventData = maps:get(<<"data">>, maps:get(<<"data">>, Event)),
         ?assertEqual(#{<<"debug_output_active">> => false}, maps:get(<<"collector">>, EventData)),
         ?assertNot(maps:is_key(<<"inventory">>, EventData)),
         ok = socket:close(Socket)
     end end}.

companion_diagnostic_socket_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         Socket = connect_client(Path, <<"wfcompanion">>, #{<<"pid">> => 123}),
         await_external_activity(1, 20),
         Snapshot = wfcli_player_service:snapshot(),
         {ok, Ref} = wfcli_local_api:diagnostics_submit(self(),
                         #{action => watch, topic => inventory, seconds => 60}),
         #{<<"event">> := <<"companion_diagnostics">>, <<"request_id">> := Id,
           <<"request">> := #{<<"action">> := <<"watch">>}} = diagnostic_command(Socket),
         Data = #{<<"state">> => <<"running">>, <<"job">> => Id, <<"samples">> => 1},
         ok = socket:send(Socket, wfcli_local_protocol:encode(
             #{<<"op">> => <<"companion_diagnostics">>, <<"request_id">> => Id, <<"data">> => Data})),
         receive {wfcli_daemon, Ref, {ok, Data}} -> ok after 1000 -> error(no_reply) end,
         wfcli_local_api:diagnostics_consume(self(), Ref),
         ?assertMatch(#{<<"request">> := #{<<"action">> := <<"credit">>}}, diagnostic_command(Socket)),
         ?assertEqual(Snapshot, wfcli_player_service:snapshot()),
         ok = socket:close(Socket),
         receive {wfcli_daemon, Ref, {error, companion_disconnected}} -> ok
         after 1000 -> error(no_disconnect) end,
         ?assertEqual({error, no_companion_connected},
                      wfcli_local_api:diagnostics_submit(self(), #{action => status}))
     end end}.

diagnostic_command(Socket) ->
    {ok, Line} = socket:recv(Socket, 0, 2000),
    {ok, Message} = wfcli_local_protocol:decode(string:trim(Line)),
    Message.

build_stats_socket_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         application:set_env(wfdaemon, build_source_fun,
           fun(#{action := stats, options := Options}, undefined) ->
               wfcli_build_stats:calculate(Options, wfcli_build_stats_eunit:equipment(),
                   [wfcli_build_stats_eunit:weapon() | wfcli_stat_frames_eunit:exports()])
           end),
         Socket = connect_client(Path, <<"test">>, #{}),
         ok = socket:send(Socket, wfcli_local_protocol:encode(
             #{<<"op">> => <<"build_stats">>, <<"id">> => 2,
               <<"options">> => (wfcli_build_stats_eunit:options())#{<<"armor">> => 2700,
                                                                   <<"bounds">> => true}})),
         Reply = receive_response(Socket, 2, <<>>),
         ?assertEqual(true, maps:get(<<"ok">>, Reply)),
         Data = maps:get(<<"data">>, Reply),
         ?assertMatch(#{<<"schema">> := 1, <<"stats">> := #{}}, Data),
         #{<<"stats">> := #{<<"projectile_outcomes">> := Outcomes,
                             <<"source_packet">> := #{<<"channels">> := Channels}}} = Data,
         ?assert(is_map(Channels)),
         ?assertMatch([#{<<"tier">> := _, <<"probability">> := _,
                         <<"raw_damage">> := _, <<"health_damage">> := _} | _], Outcomes),
         ?assert(lists:all(fun(#{<<"health_damage">> := D}) -> is_integer(D) end, Outcomes)),
         ?assertEqual(101, maps:get(<<"player_revision">>, Data)),
         ?assertMatch(#{<<"lower">> := #{}, <<"upper">> := #{}}, maps:get(<<"bounds">>, Data)),
         ?assert(is_list(maps:get(<<"effect_graph">>, Data))),
         ok = socket:send(Socket, wfcli_local_protocol:encode(
             #{<<"op">> => <<"build_stats">>, <<"id">> => 3,
               <<"options">> => (wfcli_build_stats_eunit:options())#{<<"armor">> => -1}})),
         ?assertEqual(false, maps:get(<<"ok">>, receive_response(Socket, 3, <<>>))),
         Nourish = <<"/Lotus/Powersuits/Devourer/Abilities/DevourerConsumeAbility">>,
         Frame = #{<<"definition_id">> => <<"/Lotus/Powersuits/Cowgirl/MesaPrime">>,
                   <<"class">> => <<"warframe">>,
                   <<"config">> => #{<<"upgrade_slots">> => [],
                       <<"ability_override">> => #{<<"Ability">> => Nourish, <<"Index">> => 0},
                       <<"ability_ranks">> => #{Nourish => 3}}},
         Options = wfcli_build_stats_eunit:options(),
         Builds = maps:get(<<"buildset">>, Options),
         ok = socket:send(Socket, wfcli_local_protocol:encode(
             #{<<"op">> => <<"build_stats">>, <<"id">> => 4,
               <<"options">> => Options#{<<"buildset">> => Builds#{
                   <<"warframe">> => #{<<"build">> => Frame}},
                   <<"effects">> => #{<<"warframe/nourish">> => 1}}})),
         AbilityReply = receive_response(Socket, 4, <<>>),
         ?assertEqual(true, maps:get(<<"ok">>, AbilityReply)),
         [Ability] = [N || N = #{<<"kind">> := <<"ability">>} <-
             maps:get(<<"effect_graph">>, maps:get(<<"data">>, AbilityReply))],
         ?assertEqual(<<"explicit">>, maps:get(<<"rank_source">>, Ability)),
         ?assertMatch(#{<<"active_stacks">> := 1,
                        <<"effects">> := [#{<<"element">> := <<"viral">>,
                                           <<"target">> := <<"weapon">>}]},
                      maps:get(<<"selected">>, maps:get(<<"states">>, Ability))),
         ok = socket:send(Socket, wfcli_local_protocol:encode(
             #{<<"op">> => <<"build_stats">>, <<"id">> => 5,
               <<"options">> => (wfcli_stat_frames_eunit:request(<<"Nova Prime">>, 30, []))#{
                   <<"bounds">> => true}})),
         FrameReply = receive_response(Socket, 5, <<>>),
         ?assertEqual(true, maps:get(<<"ok">>, FrameReply)),
         FrameData = maps:get(<<"data">>, FrameReply),
         ?assertEqual(<<"warframe">>, maps:get(<<"kind">>, FrameData)),
         ?assertEqual(370.0, maps:get(<<"health">>, maps:get(<<"stats">>, FrameData))),
         [Prime] = maps:get(<<"selected">>, maps:get(<<"abilities">>, FrameData)),
         ?assertMatch(#{<<"crowd_control">> := #{<<"variants">> := #{<<"slow">> := 0.5, <<"speed">> := 1.5}}},
                      maps:get(<<"outputs">>, Prime)),
         ok = socket:close(Socket)
     end end}.

backend_restart_disconnects_and_restores_events_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         Socket = connect_client(Path, <<"test">>, #{}),
         Request = #{<<"op">> => <<"subscribe">>, <<"id">> => 2,
                     <<"dataset">> => <<"player">>, <<"include_data">> => false},
         ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
         _ = receive_response(Socket, 2, <<>>),
         ok = gen_server:stop(wfcli_player_service),
         {ok, _} = wfcli_player_service:start_link(),
         ?assertEqual({error, closed}, socket:recv(Socket, 0, 2000)),
         ok = socket:close(Socket),
         request_player_metadata_subscription(Path)
     end end}.

backend_restart_closes_pending_requests_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         Test = self(),
         application:set_env(wfdaemon, build_source_fun,
                             fun(_, _) ->
                                 Test ! build_started,
                                 receive finish -> {error, unused} end
                             end),
         Socket = connect_client(Path, <<"test">>, #{}),
         ok = socket:send(Socket, wfcli_local_protocol:encode(
                           #{<<"op">> => <<"build_detail">>, <<"id">> => 2,
                             <<"build_id">> => 300})),
         receive build_started -> ok after 2000 -> error(build_not_started) end,
         ok = gen_server:stop(wfcli_build_service),
         {ok, _} = wfcli_build_service:start_link(),
         ?assertEqual({error, closed}, socket:recv(Socket, 0, 2000)),
         ok = socket:close(Socket)
     end end}.

api_owner_death_closes_accepted_sockets_test_() ->
    {setup, fun() ->
                State = setup(),
                unlink(whereis(wfcli_local_api)),
                State
            end, fun cleanup/1,
     fun(#{socket := Path}) -> fun() ->
         Socket = connect_client(Path, <<"test">>, #{}),
         Pid = whereis(wfcli_local_api),
         Ref = monitor(process, Pid),
         exit(Pid, kill),
         receive {'DOWN', Ref, process, Pid, killed} -> ok
         after 2000 -> error(api_not_stopped)
         end,
         {ok, _} = wfcli_local_api:start_link(),
         ?assertEqual({error, closed}, socket:recv(Socket, 0, 2000)),
         ok = socket:close(Socket),
         Fresh = connect_client(Path, <<"test">>, #{}),
         ok = socket:close(Fresh)
     end end}.

contract_change_disconnects_native_clients_test() ->
    State = #{contract => #{}, connections => #{self() => #{}},
              worker_holders => #{}, worker_waiters => queue:new(),
              worker_count => 0, worker_limit => 1},
    {ok, Updated} = wfcli_local_api:code_change(undefined, State, undefined),
    receive shutdown -> ok after 1000 -> error(client_not_disconnected) end,
    ?assertEqual(
       maps:with([<<"envelope">>, <<"interfaces">>],
                 wfcli_local_protocol:contract()),
       maps:get(contract, Updated)).

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-local-api-" ++ os:getpid() ++ "-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    SocketPath = filename:join(Root, "wfdaemon.sock"),
    CachePath = filename:join(Root, "player.term"),
    GameMetadataCache = filename:join(Root, "game-metadata.term"),
    MarketCache = filename:join(Root, "market.term"),
    AssetCache = filename:join(Root, "assets"),
    NotificationSettings = filename:join(Root, "notifications.json"),
    MarketToken = filename:join(Root, "market-token"),
    MarketPresence = filename:join(Root, "market-presence.json"),
    OverframeSession = filename:join(Root, "overframe-session.json"),
    BuildStore = filename:join(Root, "builds.term"),
    ResolutionIssues = filename:join(Root, "resolution-issues.json"),
    ItemCatalog = filename:join(Root, "WFCDItems.json"),
    ItemRecipes = filename:join(Root, "ExportRecipes_en.json"),
    StarChart = filename:join(Root, "StarChart.json"),
    ok = filelib:ensure_dir(filename:join(Root, "placeholder")),
    ok = file:write_file(
           ItemCatalog,
           jsone:encode(#{<<"version">> => <<"test">>, <<"fetchedAt">> => 1,
                          <<"entries">> => []})),
    ok = file:write_file(ItemRecipes,
                         jsone:encode(#{<<"ExportRecipes">> => []})),
    ok = file:write_file(
           StarChart,
           jsone:encode(#{<<"version">> => <<"test">>, <<"fetchedAt">> => 1,
                          <<"nodes">> => #{<<"EarthNode">> => 3000},
                          <<"junctions">> => []})),
    application:set_env(wfdaemon, local_socket, SocketPath),
    application:set_env(wfdaemon, player_cache, CachePath),
    application:set_env(wfdaemon, game_metadata_cache, GameMetadataCache),
    application:set_env(wfdaemon, market_cache, MarketCache),
    application:set_env(wfdaemon, market_request_interval_ms, 0),
    application:set_env(wfdaemon, market_http_fun, fun market_http/2),
    application:set_env(wfdaemon, asset_cache_dir, AssetCache),
    application:set_env(wfdaemon, notification_settings_file, NotificationSettings),
    application:set_env(wfdaemon, market_account_file, MarketToken),
    application:set_env(wfdaemon, market_presence_file, MarketPresence),
    application:set_env(wfdaemon, overframe_account_file, OverframeSession),
    application:set_env(wfdaemon, overframe_http_fun, fun overframe_http/2),
    application:set_env(wfdaemon, build_store_file, BuildStore),
    application:set_env(wfdaemon, build_cache_file, BuildStore ++ ".cache"),
    application:set_env(wfdaemon, resolution_issues_file, ResolutionIssues),
    application:set_env(wfdaemon, build_catalog_fun, fun build_catalog/0),
    application:set_env(wfdaemon, build_source_fun, fun build_source/2),
    application:set_env(wfdaemon, item_catalog_file, ItemCatalog),
    application:set_env(wfdaemon, item_recipe_file, ItemRecipes),
    application:set_env(wfdaemon, star_chart_file, StarChart),
    application:set_env(wfdaemon, local_request_workers, 2),
    application:set_env(wfdaemon, local_request_global_workers, 2),
    application:set_env(wfdaemon, daemon_idle_shutdown, false),
    {ok, _Worldstate} = wfcli_worldstate_service:start_link(),
    {ok, _Forma} = wfcli_forma_service:start_link(),
    {ok, _Player} = wfcli_player_service:start_link(),
    {ok, _GameMetadata} = wfcli_game_metadata_service:start_link(),
    {ok, _ResolutionIssues} = wfcli_resolution_issues:start_link(),
    {ok, _Builds} = wfcli_build_service:start_link(),
    {ok, _MarketLimiter} = wfcli_market_limiter:start_link(),
    {ok, _Market} = wfcli_market_service:start_link(),
    {ok, _MarketAccount} = wfcli_market_account_service:start_link(),
    {ok, _MarketPresence} = wfcli_market_presence_service:start_link(),
    {ok, _Assets} = wfcli_asset_service:start_link(),
    {ok, _Notifications} = wfcli_notification_service:start_link(),
    {ok, _Api} = wfcli_local_api:start_link(),
    #{root => Root, socket => SocketPath, cache => CachePath,
      game_metadata_cache => GameMetadataCache,
      market_cache => MarketCache, market_token => MarketToken,
      market_presence => MarketPresence,
      build_store => BuildStore,
      resolution_issues => ResolutionIssues,
      overframe_session => OverframeSession,
      notification_settings => NotificationSettings}.

cleanup(#{root := Root, socket := SocketPath, cache := CachePath,
          game_metadata_cache := GameMetadataCache,
          market_cache := MarketCache, market_token := MarketToken,
          market_presence := MarketPresence,
          build_store := BuildStore,
          resolution_issues := ResolutionIssues,
          overframe_session := OverframeSession,
          notification_settings := NotificationSettings}) ->
    gen_server:stop(wfcli_local_api),
    gen_server:stop(wfcli_notification_service),
    gen_server:stop(wfcli_asset_service),
    gen_server:stop(wfcli_market_presence_service),
    gen_server:stop(wfcli_market_account_service),
    gen_server:stop(wfcli_market_service),
    gen_server:stop(wfcli_market_limiter),
    gen_server:stop(wfcli_build_service),
    gen_server:stop(wfcli_resolution_issues),
    gen_server:stop(wfcli_game_metadata_service),
    gen_server:stop(wfcli_player_service),
    gen_server:stop(wfcli_forma_service),
    gen_server:stop(wfcli_worldstate_service),
    application:unset_env(wfdaemon, local_socket),
    application:unset_env(wfdaemon, player_cache),
    application:unset_env(wfdaemon, game_metadata_cache),
    application:unset_env(wfdaemon, market_cache),
    application:unset_env(wfdaemon, market_request_interval_ms),
    application:unset_env(wfdaemon, market_http_fun),
    application:unset_env(wfdaemon, asset_cache_dir),
    application:unset_env(wfdaemon, notification_settings_file),
    application:unset_env(wfdaemon, market_account_file),
    application:unset_env(wfdaemon, market_presence_file),
    application:unset_env(wfdaemon, overframe_account_file),
    application:unset_env(wfdaemon, overframe_http_fun),
    application:unset_env(wfdaemon, build_store_file),
    application:unset_env(wfdaemon, build_cache_file),
    application:unset_env(wfdaemon, resolution_issues_file),
    application:unset_env(wfdaemon, build_catalog_fun),
    application:unset_env(wfdaemon, build_source_fun),
    application:unset_env(wfdaemon, item_catalog_file),
    application:unset_env(wfdaemon, item_recipe_file),
    application:unset_env(wfdaemon, star_chart_file),
    application:unset_env(wfdaemon, local_request_workers),
    application:unset_env(wfdaemon, local_request_global_workers),
    application:unset_env(wfdaemon, asset_http_fun),
    application:unset_env(wfdaemon, asset_fresh_ms),
    application:unset_env(wfdaemon, daemon_idle_shutdown),
    _ = file:delete(SocketPath),
    _ = file:delete(CachePath),
    _ = file:delete(CachePath ++ ".tmp"),
    _ = file:delete(GameMetadataCache),
    _ = file:delete(GameMetadataCache ++ ".tmp"),
    _ = file:delete(MarketCache),
    _ = file:delete(MarketCache ++ ".tmp"),
    _ = file:delete(NotificationSettings),
    _ = file:delete(NotificationSettings ++ ".tmp"),
    _ = file:delete(MarketToken),
    _ = file:delete(MarketToken ++ ".tmp"),
    _ = file:delete(MarketPresence),
    _ = file:delete(MarketPresence ++ ".tmp"),
    _ = file:delete(OverframeSession),
    _ = file:delete(OverframeSession ++ ".tmp"),
    _ = file:delete(BuildStore),
    _ = file:delete(BuildStore ++ ".tmp"),
    _ = file:delete(ResolutionIssues),
    _ = file:delete(ResolutionIssues ++ ".tmp"),
    _ = file:del_dir_r(Root),
    ok.

lifecycle(#{socket := SocketPath}) ->
    request_player_metadata_subscription(SocketPath),
    TestSocket = connect_client(SocketPath, <<"test">>, #{}),
    ?assertMatch(#{external_activity := 0}, wfcli_worldstate_service:status()),
    request_game_metadata_publish(TestSocket),
    request_mastery_view_from_raw_publish(TestSocket),
    request_build_equipment_view(TestSocket),
    request_build_source_views(TestSocket),
    request_build_groups(TestSocket),
    request_market_resolve(TestSocket),
    request_market_describe(TestSocket),
    reject_invalid_market_resolve(TestSocket),
    request_market_account(TestSocket),
    request_overframe_account(TestSocket),
    reject_invalid_overframe_session(TestSocket),
    request_market_presence(TestSocket),
    reject_invalid_market_order(TestSocket),
    route_market_order_mutations(TestSocket),
    reject_invalid_relic_limit(TestSocket),
    request_market_quote(TestSocket),
    request_market_variant_quote(TestSocket),
    request_cached_market_quote(TestSocket),
    request_notification_settings(TestSocket),
    request_diagnostics_report(TestSocket),
    reject_unnegotiated_diagnostics(SocketPath),
    slow_asset_does_not_block_dataset(TestSocket),
    streams_stale_asset_refresh(TestSocket),
    local_request_limit_queues_excess_work(TestSocket),
    global_local_request_limit_queues_other_clients(TestSocket, SocketPath),
    request_asset_cache_controls(TestSocket),
    ok = socket:close(TestSocket),

    GuiSocket = connect_client(
                  SocketPath, <<"wfgui">>,
                  #{<<"pid">> => 1000, <<"mode">> => <<"desktop">>}),
    await_external_activity(1, 20),
    ok = socket:close(GuiSocket),
    await_external_activity(0, 20),

    CompanionSocket1 = connect_client(
                         SocketPath, <<"wfcompanion">>,
                         #{<<"pid">> => 1001, <<"mode">> => <<"standalone">>}),
    await_external_activity(1, 20),
    publish_game_running(CompanionSocket1),
    ?assertMatch(#{game_active := true}, wfcli_player_service:status()),
    ?assertMatch(#{external_activity := 1}, wfcli_worldstate_service:status()),
    CompanionSocket2 = connect_client(
                         SocketPath, <<"wfcompanion">>,
                         #{<<"pid">> => 1002, <<"mode">> => <<"launch">>}),
    await_external_activity(2, 20),
    #{companions := 2, companion_details := Details} = wfcli_local_api:status(),
    ?assertEqual([<<"launch">>, <<"standalone">>],
                 lists:sort([maps:get(mode, Detail) || Detail <- Details])),
    ?assertEqual([1001, 1002], lists:sort([maps:get(os_pid, Detail) || Detail <- Details])),
    ok = socket:close(CompanionSocket1),
    await_external_activity(1, 20),
    ok = socket:close(CompanionSocket2),
    await_external_activity(0, 20).

request_mastery_view_from_raw_publish(Socket) ->
    Observation = #{
        <<"schema">> => 2,
        <<"profile">> => #{<<"player_name">> => <<"TestTenno">>},
        <<"raw">> => #{
            <<"PlayerLevel">> => 1,
            <<"Missions">> => [#{<<"Tag">> => <<"EarthNode">>,
                                 <<"Completes">> => 1, <<"Tier">> => 0}]
        }
    },
    Publish = #{<<"op">> => <<"publish">>, <<"id">> => 3,
                <<"dataset">> => <<"player">>, <<"source">> => <<"inventory_http">>,
                <<"data">> => inventory_observation(Observation)},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Publish)),
    {ok, PublishLine} = socket:recv(Socket, 0, 5000),
    {ok, PublishReply} = wfcli_local_protocol:decode(string:trim(PublishLine)),
    ?assertEqual(true, maps:get(<<"ok">>, PublishReply)),

    Request = #{<<"op">> => <<"mastery_view">>, <<"id">> => 4},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    Summary = maps:get(<<"summary">>, maps:get(<<"data">>, Reply)),
    ?assertEqual(true,
                 maps:get(<<"available">>, maps:get(<<"rank_progress">>, Summary))),
    ?assertEqual(1, maps:get(<<"current">>,
                             maps:get(<<"normal">>, maps:get(<<"star_chart">>, Summary)))),
    ?assertEqual(3000,
                 maps:get(<<"total_xp">>, maps:get(<<"rank_progress">>, Summary))),
    ok = wfcli_player_service:clear().

request_game_metadata_publish(Socket) ->
    Metadata = #{<<"schema">> => 2,
                 <<"executable">> =>
                     #{<<"sha256">> => binary:copy(<<"a">>, 64)},
                 <<"archimedea">> => #{<<"catalog">> => #{}}},
    Publish = #{<<"op">> => <<"publish">>, <<"id">> => 81,
                <<"dataset">> => <<"game_metadata">>, <<"data">> => Metadata},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Publish)),
    {ok, PublishLine} = socket:recv(Socket, 0, 5000),
    {ok, PublishReply} = wfcli_local_protocol:decode(string:trim(PublishLine)),
    ?assertEqual(true, maps:get(<<"ok">>, PublishReply)),

    Get = #{<<"op">> => <<"get">>, <<"id">> => 82,
            <<"dataset">> => <<"game_metadata">>},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Get)),
    {ok, GetLine} = socket:recv(Socket, 0, 5000),
    {ok, GetReply} = wfcli_local_protocol:decode(string:trim(GetLine)),
    ?assertEqual(Metadata,
                 maps:get(<<"data">>, maps:get(<<"data">>, GetReply))).

request_asset_cache_controls(Socket) ->
    Status = #{<<"op">> => <<"asset_cache_status">>, <<"id">> => 40},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Status)),
    {ok, StatusLine} = socket:recv(Socket, 0, 1000),
    {ok, StatusReply} = wfcli_local_protocol:decode(string:trim(StatusLine)),
    ?assertEqual(true, maps:get(<<"ok">>, StatusReply)),
    StatusData = maps:get(<<"data">>, StatusReply),
    ?assert(is_integer(maps:get(<<"bytes">>, StatusData))),
    ?assert(is_binary(maps:get(<<"cache_root">>, StatusData))),

    Clear = #{<<"op">> => <<"asset_cache_clear">>, <<"id">> => 41},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Clear)),
    {ok, ClearLine} = socket:recv(Socket, 0, 1000),
    {ok, ClearReply} = wfcli_local_protocol:decode(string:trim(ClearLine)),
    ?assertEqual(true, maps:get(<<"ok">>, ClearReply)),
    ClearData = maps:get(<<"data">>, ClearReply),
    ?assertEqual(0, maps:get(<<"objects">>, ClearData)),
    ?assertEqual(0, maps:get(<<"bytes">>, ClearData)).

slow_asset_does_not_block_dataset(Socket) ->
    Test = self(),
    application:set_env(
      wfdaemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {asset_fetch_started, self()},
          receive continue ->
              {ok, 200, [{"content-type", "image/png"}],
               <<16#89, "PNG", 13, 10, 26, 10, 0>>}
          end
      end),
    AssetRequest = #{<<"op">> => <<"asset_resolve">>, <<"id">> => 10,
                     <<"assets">> => [#{<<"id">> => <<"test">>,
                                         <<"image_name">> => <<"test.png">>}]},
    ok = socket:send(Socket, wfcli_local_protocol:encode(AssetRequest)),
    AssetWorker = receive
        {asset_fetch_started, Pid} -> Pid
    after 1000 ->
        error(asset_fetch_not_started)
    end,
    GetRequest = #{<<"op">> => <<"get">>, <<"id">> => 11,
                   <<"dataset">> => <<"daemon">>},
    ok = socket:send(Socket, wfcli_local_protocol:encode(GetRequest)),
    {ok, GetLine} = socket:recv(Socket, 0, 1000),
    {ok, GetReply} = wfcli_local_protocol:decode(string:trim(GetLine)),
    ?assertEqual(11, maps:get(<<"id">>, GetReply)),
    ?assertEqual(true, maps:get(<<"ok">>, GetReply)),
    AssetWorker ! continue,
    {ok, AssetLine} = socket:recv(Socket, 0, 1000),
    {ok, AssetReply} = wfcli_local_protocol:decode(string:trim(AssetLine)),
    ?assertEqual(10, maps:get(<<"id">>, AssetReply)),
    ?assertEqual(true, maps:get(<<"ok">>, AssetReply)),
    application:unset_env(wfdaemon, asset_http_fun).

streams_stale_asset_refresh(Socket) ->
    Png = <<16#89, "PNG", 13, 10, 26, 10, 0>>,
    application:set_env(
      wfdaemon, asset_http_fun,
      fun(_Url, _Headers) -> {ok, 200, [], Png} end),
    send_asset_request(Socket, 12, <<"refresh.png">>),
    {ok, InitialLine} = socket:recv(Socket, 0, 1000),
    {ok, Initial} = wfcli_local_protocol:decode(string:trim(InitialLine)),
    [InitialAsset] = maps:get(<<"assets">>, maps:get(<<"data">>, Initial)),
    InitialDigest = maps:get(<<"digest">>, InitialAsset),

    Test = self(),
    application:set_env(wfdaemon, asset_fresh_ms, 0),
    application:set_env(
      wfdaemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {socket_asset_refresh_started, self()},
          receive continue -> {ok, 200, [], <<Png/binary, 1>>} end
      end),
    send_asset_request(Socket, 13, <<"refresh.png">>),
    Worker = receive
        {socket_asset_refresh_started, Pid} -> Pid
    after 1000 -> error(socket_asset_refresh_not_started)
    end,
    {ok, StaleLine} = socket:recv(Socket, 0, 1000),
    {ok, Stale} = wfcli_local_protocol:decode(string:trim(StaleLine)),
    [StaleAsset] = maps:get(<<"assets">>, maps:get(<<"data">>, Stale)),
    ?assertEqual(true, maps:get(<<"stale">>, StaleAsset)),
    ?assertEqual(InitialDigest, maps:get(<<"digest">>, StaleAsset)),

    Worker ! continue,
    {ok, EventLine} = socket:recv(Socket, 0, 1000),
    {ok, Event} = wfcli_local_protocol:decode(string:trim(EventLine)),
    ?assertEqual(<<"asset">>, maps:get(<<"event">>, Event)),
    Refreshed = maps:get(<<"data">>, Event),
    ?assertNotEqual(InitialDigest, maps:get(<<"digest">>, Refreshed)),
    ?assertEqual(<<"refresh.png">>, maps:get(<<"image_name">>, Refreshed)),
    application:unset_env(wfdaemon, asset_fresh_ms),
    application:unset_env(wfdaemon, asset_http_fun).

local_request_limit_queues_excess_work(Socket) ->
    Test = self(),
    application:set_env(
      wfdaemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {local_asset_started, self()},
          receive continue ->
              {ok, 200, [{"content-type", "image/png"}],
               <<16#89, "PNG", 13, 10, 26, 10, 0>>}
          end
      end),
    lists:foreach(
      fun(Id) ->
          Name = <<"local-", (integer_to_binary(Id))/binary, ".png">>,
          Request = #{<<"op">> => <<"asset_resolve">>, <<"id">> => Id,
                      <<"assets">> => [#{<<"id">> => integer_to_binary(Id),
                                          <<"image_name">> => Name}]},
          ok = socket:send(Socket, wfcli_local_protocol:encode(Request))
      end,
      [20, 21, 22]),
    First = receive {local_asset_started, Pid1} -> Pid1 after 1000 -> timeout end,
    Second = receive {local_asset_started, Pid2} -> Pid2 after 1000 -> timeout end,
    ?assert(is_pid(First)),
    ?assert(is_pid(Second)),
    receive {local_asset_started, _Pid} -> error(local_request_limit_exceeded)
    after 100 -> ok
    end,
    First ! continue,
    Third = receive {local_asset_started, Pid3} -> Pid3 after 1000 -> timeout end,
    ?assert(is_pid(Third)),
    Second ! continue,
    Third ! continue,
    ?assertEqual([20, 21, 22], lists:sort(recv_reply_ids(Socket, 3, <<>>, []))),
    application:unset_env(wfdaemon, asset_http_fun).

global_local_request_limit_queues_other_clients(Socket, SocketPath) ->
    Other = connect_client(SocketPath, <<"test">>, #{}),
    Test = self(),
    application:set_env(
      wfdaemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {global_asset_started, self()},
          receive continue ->
              {ok, 200, [{"content-type", "image/png"}],
               <<16#89, "PNG", 13, 10, 26, 10, 0>>}
          end
      end),
    send_asset_request(Socket, 30, <<"global-30.png">>),
    send_asset_request(Socket, 31, <<"global-31.png">>),
    First = receive {global_asset_started, Pid1} -> Pid1 after 1000 -> timeout end,
    Second = receive {global_asset_started, Pid2} -> Pid2 after 1000 -> timeout end,
    ?assert(is_pid(First)),
    ?assert(is_pid(Second)),
    send_asset_request(Other, 32, <<"global-32.png">>),
    receive {global_asset_started, _Pid} -> error(global_local_request_limit_exceeded)
    after 100 -> ok
    end,
    First ! continue,
    Third = receive {global_asset_started, Pid3} -> Pid3 after 1000 -> timeout end,
    ?assert(is_pid(Third)),
    Second ! continue,
    Third ! continue,
    ?assertEqual([30, 31], lists:sort(recv_reply_ids(Socket, 2, <<>>, []))),
    ?assertEqual([32], recv_reply_ids(Other, 1, <<>>, [])),
    ok = socket:close(Other),
    application:unset_env(wfdaemon, asset_http_fun).

send_asset_request(Socket, Id, Name) ->
    Request = #{<<"op">> => <<"asset_resolve">>, <<"id">> => Id,
                <<"assets">> => [#{<<"id">> => integer_to_binary(Id),
                                    <<"image_name">> => Name}]},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)).

recv_reply_ids(_Socket, Count, _Buffer, Ids) when length(Ids) >= Count -> Ids;
recv_reply_ids(Socket, Count, Buffer, Ids) ->
    {ok, Data} = socket:recv(Socket, 0, 1000),
    Parts = binary:split(<<Buffer/binary, Data/binary>>, <<"\n">>, [global]),
    [Rest | ReversedLines] = lists:reverse(Parts),
    Lines = lists:reverse(ReversedLines),
    NewIds = [maps:get(<<"id">>, Reply)
              || Line <- Lines, Line =/= <<>>,
                 {ok, Reply} <- [wfcli_local_protocol:decode(Line)]],
    recv_reply_ids(Socket, Count, Rest, NewIds ++ Ids).

connect_client(SocketPath, Client, Extra) ->
    {ok, Socket} = socket:open(local, stream, default),
    ok = socket:connect(Socket, #{family => local, path => SocketPath}),
    Hello = maps:merge(
              (wfcli_local_protocol:contract())#{
                 <<"op">> => <<"hello">>, <<"id">> => 1,
                 <<"client">> => Client, <<"version">> => <<"test">>},
              Extra),
    ok = socket:send(Socket, wfcli_local_protocol:encode(Hello)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(1, maps:get(<<"id">>, Reply)),
    ?assertEqual(wfcli_local_protocol:envelope_version(),
                 maps:get(<<"envelope">>, Reply)),
    ?assertEqual(wfcli_local_protocol:interfaces(),
                 maps:get(<<"interfaces">>, Reply)),
    Requested = maps:get(<<"features">>, Hello, []),
    ExpectedFeatures = [Feature || Feature <- wfcli_local_protocol:features(),
                                   lists:member(Feature, Requested)],
    ?assertEqual(ExpectedFeatures, maps:get(<<"features">>, Reply)),
    Socket.

inventory_observation(Observation) ->
    Full = wfcli_player_inventory_eunit:full(1, 0),
    Raw = (maps:get(<<"raw">>, Observation))#{<<"LastInventorySync">> => maps:get(<<"sync">>, Full)},
    (maps:merge(Full, Observation))#{<<"raw">> => Raw}.

request_build_equipment_view(Socket) ->
    Observation = #{
        <<"schema">> => 2,
        <<"raw">> => #{
            <<"SpecialItems">> => [
                #{<<"ItemId">> => #{<<"$oid">> => <<"exalted-1">>},
                  <<"ItemType">> => <<"/Lotus/Powersuits/Excalibur/DoomSword">>,
                  <<"Features">> => 547,
                  <<"Polarized">> => 2,
                  <<"Configs">> => [#{<<"Name">> => <<"Config A">>,
                                      <<"Upgrades">> => []}]}
            ]
        }
    },
    Publish = #{<<"op">> => <<"publish">>, <<"id">> => 42,
                <<"dataset">> => <<"player">>, <<"source">> => <<"inventory_http">>,
                <<"data">> => inventory_observation(Observation)},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Publish)),
    {ok, PublishLine} = socket:recv(Socket, 0, 5000),
    {ok, PublishReply} = wfcli_local_protocol:decode(string:trim(PublishLine)),
    ?assertEqual(true, maps:get(<<"ok">>, PublishReply)),

    Request = #{<<"op">> => <<"build_equipment">>, <<"id">> => 43},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    Data = maps:get(<<"data">>, Reply),
    [Instance] = maps:get(<<"instances">>, Data),
    ?assertEqual(<<"exalted">>, maps:get(<<"class">>, Instance)),
    ?assertEqual(2, maps:get(<<"forma_count">>, Instance)).

request_build_source_views(Socket) ->
    Requests = [
        {44, #{<<"op">> => <<"build_search">>, <<"query">> => <<"revenant">>},
         <<"build_items">>},
        {45, #{<<"op">> => <<"build_list">>,
               <<"item">> => <<"/Lotus/Powersuits/Revenant/RevenantPrime">>},
         <<"builds">>},
        {46, #{<<"op">> => <<"build_detail">>, <<"build_id">> => 374539},
         <<"build_revision">>}
    ],
    lists:foreach(
      fun({Id, Request, Dataset}) ->
          ok = socket:send(Socket,
                           wfcli_local_protocol:encode(Request#{<<"id">> => Id})),
          {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
          {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
          ?assertEqual(true, maps:get(<<"ok">>, Reply)),
          ?assertEqual(Dataset, maps:get(<<"dataset">>, Reply))
      end, Requests).

request_build_groups(Socket) ->
    Create = #{<<"op">> => <<"build_group_create">>, <<"id">> => 80,
               <<"group">> =>
                   #{<<"definition_id">> =>
                         <<"/Lotus/Powersuits/Excalibur/DoomSword">>,
                     <<"instance_id">> => <<"exalted-1">>,
                     <<"name">> => <<"Exalted plans">>}},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Create)),
    {CreateReply, CreateEvent} = receive_group_mutation(Socket, 80),
    ?assertEqual(true, maps:get(<<"ok">>, CreateReply)),
    ?assertEqual(<<"created">>, maps:get(<<"action">>, CreateEvent)),
    Group0 = maps:get(<<"data">>, CreateReply),
    GroupId = maps:get(<<"id">>, Group0),

    AddConfig = #{<<"op">> => <<"build_group_add_config">>, <<"id">> => 81,
                  <<"group_id">> => GroupId, <<"revision">> => 1,
                  <<"instance_id">> => <<"exalted-1">>, <<"config_index">> => 0},
    ok = socket:send(Socket, wfcli_local_protocol:encode(AddConfig)),
    {ConfigReply, ConfigEvent} = receive_group_mutation(Socket, 81),
    ?assertEqual(<<"member_added">>, maps:get(<<"action">>, ConfigEvent)),
    Group1 = maps:get(<<"data">>, ConfigReply),
    ?assertEqual(1, maps:get(<<"config_member_count">>, Group1)),

    AddSource = #{<<"op">> => <<"build_group_add_source">>, <<"id">> => 82,
                  <<"group_id">> => GroupId, <<"revision">> => 2,
                  <<"source">> => <<"overframe">>, <<"external_id">> => 374539,
                  <<"fingerprint">> => <<"test-revision">>},
    ok = socket:send(Socket, wfcli_local_protocol:encode(AddSource)),
    {SourceReply, _SourceEvent} = receive_group_mutation(Socket, 82),
    Group2 = maps:get(<<"data">>, SourceReply),
    ?assertEqual(2, maps:get(<<"member_count">>, Group2)),

    Plan = #{<<"op">> => <<"build_group_plan">>, <<"id">> => 84,
             <<"group_id">> => GroupId, <<"revision">> => 3},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Plan)),
    {PlanReply, PlanEvent} = receive_group_mutation(Socket, 84),
    ?assertEqual(<<"planned">>, maps:get(<<"action">>, PlanEvent)),
    ?assertEqual(<<"build_plan">>, maps:get(<<"dataset">>, PlanReply)),
    ?assertEqual(<<"ready">>,
                 maps:get(<<"status">>, maps:get(<<"data">>, PlanReply))),

    List = #{<<"op">> => <<"build_group_list">>, <<"id">> => 83},
    ok = socket:send(Socket, wfcli_local_protocol:encode(List)),
    ListReply = receive_response(Socket, 83, <<>>),
    [Summary] = maps:get(<<"groups">>, maps:get(<<"data">>, ListReply)),
    ?assertEqual(GroupId, maps:get(<<"id">>, Summary)),
    ?assertEqual(2, maps:get(<<"member_count">>, Summary)),
    ?assertEqual(<<"ready">>,
                 maps:get(<<"status">>, maps:get(<<"plan_result">>, Summary))),
    ok = wfcli_player_service:clear().

receive_group_mutation(Socket, Id) ->
    receive_group_mutation(Socket, Id, <<>>, undefined, undefined).

receive_group_mutation(_Socket, _Id, _Buffer, Reply, Event)
  when Reply =/= undefined, Event =/= undefined -> {Reply, Event};
receive_group_mutation(Socket, Id, Buffer, Reply, Event) ->
    {ok, Data} = socket:recv(Socket, 0, 5000),
    Parts = binary:split(<<Buffer/binary, Data/binary>>, <<"\n">>, [global]),
    [Rest | Reversed] = lists:reverse(Parts),
    {NextReply, NextEvent} = lists:foldl(
      fun(Line, {ReplyAcc, EventAcc}) when Line =/= <<>> ->
              {ok, Message} = wfcli_local_protocol:decode(Line),
              case Message of
                  #{<<"id">> := Id} -> {Message, EventAcc};
                  #{<<"event">> := <<"build_group">>} -> {ReplyAcc, Message};
                  _ -> {ReplyAcc, EventAcc}
              end;
         (_Line, Acc) -> Acc
      end, {Reply, Event}, lists:reverse(Reversed)),
    receive_group_mutation(Socket, Id, Rest, NextReply, NextEvent).

receive_response(Socket, Id, Buffer) ->
    {ok, Data} = socket:recv(Socket, 0, 5000),
    Parts = binary:split(<<Buffer/binary, Data/binary>>, <<"\n">>, [global]),
    [Rest | Reversed] = lists:reverse(Parts),
    case [Message || Line <- lists:reverse(Reversed), Line =/= <<>>,
                     {ok, Message = #{<<"id">> := MessageId}} <-
                         [wfcli_local_protocol:decode(Line)],
                     MessageId =:= Id] of
        [Reply | _] -> Reply;
        [] -> receive_response(Socket, Id, Rest)
    end.

request_player_metadata_subscription(SocketPath) ->
    Socket = connect_client(SocketPath, <<"test">>, #{}),
    Request = #{<<"op">> => <<"subscribe">>, <<"id">> => 2,
                <<"dataset">> => <<"player">>, <<"include_data">> => false},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, InitialLine} = socket:recv(Socket, 0, 5000),
    {ok, Initial} = wfcli_local_protocol:decode(string:trim(InitialLine)),
    InitialData = maps:get(<<"data">>, Initial),
    ?assertEqual(true, maps:get(<<"ok">>, Initial)),
    ?assert(maps:is_key(<<"revision">>, InitialData)),
    ?assertNot(maps:is_key(<<"data">>, InitialData)),

    {ok, _} = wfcli_player_service:publish(
                <<"collector">>, #{<<"inventory_active">> => true}),
    {ok, CollectorLine} = socket:recv(Socket, 0, 5000),
    {ok, CollectorEvent} = wfcli_local_protocol:decode(string:trim(CollectorLine)),
    ?assertEqual(<<"collector">>, maps:get(<<"source">>, CollectorEvent)),
    {ok, Snapshot} = wfcli_player_service:publish(
                       <<"inventory">>, #{<<"schema">> => 1}),
    {ok, EventLine} = socket:recv(Socket, 0, 5000),
    {ok, Event} = wfcli_local_protocol:decode(string:trim(EventLine)),
    EventData = maps:get(<<"data">>, Event),
    ?assertEqual(<<"dataset">>, maps:get(<<"event">>, Event)),
    ?assertEqual(<<"inventory">>, maps:get(<<"source">>, Event)),
    ?assertEqual(maps:get(revision, Snapshot), maps:get(<<"revision">>, EventData)),
    ?assertNot(maps:is_key(<<"data">>, EventData)),
    ok = socket:close(Socket),
    ok = wfcli_player_service:clear().

request_notification_settings(Socket) ->
    Get = #{<<"op">> => <<"notification_settings">>, <<"id">> => 15},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Get)),
    {ok, GetLine} = socket:recv(Socket, 0, 5000),
    {ok, GetReply} = wfcli_local_protocol:decode(string:trim(GetLine)),
    ?assertEqual(<<"off">>, maps:get(
                              <<"mode">>,
                              maps:get(<<"fissures">>, maps:get(<<"data">>, GetReply)))),
    Set = #{<<"op">> => <<"notification_settings_set">>, <<"id">> => 16,
            <<"fissures">> => #{<<"mode">> => <<"session">>}},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Set)),
    {ok, SetLine} = socket:recv(Socket, 0, 5000),
    {ok, SetReply} = wfcli_local_protocol:decode(string:trim(SetLine)),
    ?assertEqual(true, maps:get(<<"ok">>, SetReply)),
    ?assertEqual(<<"session">>, maps:get(
                                  <<"mode">>,
                                  maps:get(<<"fissures">>, maps:get(<<"data">>, SetReply)))).

request_diagnostics_report(Socket) ->
    Scope = <<"client:test">>,
    Issue = #{<<"kind">> => <<"asset_decode">>,
              <<"identity">> => <<"wfcd:test.png">>,
              <<"reason">> => <<"invalid image">>,
              <<"fallback">> => <<"test.png">>},
    Report = #{<<"op">> => <<"diagnostics_report">>, <<"id">> => 45,
               <<"issues">> => [Issue]},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Report)),
    {ok, ReportLine} = socket:recv(Socket, 0, 5000),
    {ok, ReportReply} = wfcli_local_protocol:decode(string:trim(ReportLine)),
    ?assertEqual(true, maps:get(<<"ok">>, ReportReply)),
    [Stored] = [Entry || Entry <- wfcli_resolution_issues:list(),
                         maps:get(<<"scope">>, Entry, undefined) =:= Scope,
                         maps:get(<<"identity">>, Entry, undefined) =:=
                             <<"wfcd:test.png">>],
    ?assertEqual(<<"asset_decode">>, maps:get(<<"kind">>, Stored)),
    Clear = #{<<"op">> => <<"diagnostics_report">>, <<"id">> => 46,
              <<"issues">> => []},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Clear)),
    {ok, ClearLine} = socket:recv(Socket, 0, 5000),
    {ok, ClearReply} = wfcli_local_protocol:decode(string:trim(ClearLine)),
    ?assertEqual(true, maps:get(<<"ok">>, ClearReply)),
    ?assertEqual([], [Entry || Entry <- wfcli_resolution_issues:list(),
                              maps:get(<<"scope">>, Entry, undefined) =:= Scope]).

reject_unnegotiated_diagnostics(SocketPath) ->
    Socket = connect_client(SocketPath, <<"limited">>, #{<<"features">> => []}),
    Request = #{<<"op">> => <<"diagnostics_report">>, <<"id">> => 47,
                <<"issues">> => []},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(false, maps:get(<<"ok">>, Reply)),
    ?assertEqual(<<"feature_not_negotiated">>, maps:get(<<"error">>, Reply)),
    ok = socket:close(Socket).

publish_game_running(Socket) ->
    Request = #{<<"op">> => <<"publish">>, <<"id">> => 2,
                <<"dataset">> => <<"player">>, <<"source">> => <<"game">>,
                <<"data">> => #{<<"running">> => true}},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(2, maps:get(<<"id">>, Reply)).

request_market_quote(Socket) ->
    Request = #{<<"op">> => <<"market_quote">>, <<"id">> => 9,
                <<"items">> => [<<"saryn_prime_set">>]},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(9, maps:get(<<"id">>, Reply)),
    ?assertEqual(<<"market">>, maps:get(<<"dataset">>, Reply)),
    Data = maps:get(<<"data">>, Reply),
    [QuoteRow] = maps:get(<<"quotes">>, Data),
    ?assertEqual(<<"saryn_prime_set">>, maps:get(<<"item">>, QuoteRow)),
    Quote = maps:get(<<"quote">>, QuoteRow),
    ?assertEqual(42, maps:get(<<"lowest_sell">>, Quote)).

request_market_variant_quote(Socket) ->
    Request = #{<<"op">> => <<"market_quote_variant">>, <<"id">> => 21,
                <<"item">> => <<"saryn_prime_set">>,
                <<"filters">> => #{<<"rank">> => 5,
                                     <<"subtype">> => <<"blueprint">>}},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(21, maps:get(<<"id">>, Reply)),
    Data = maps:get(<<"data">>, Reply),
    ?assertEqual(<<"saryn_prime_set">>, maps:get(<<"slug">>, Data)),
    ?assertEqual(42, maps:get(<<"lowest_sell">>, maps:get(<<"quote">>, Data))).

request_cached_market_quote(Socket) ->
    Request = #{<<"op">> => <<"market_quote">>, <<"id">> => 14,
                <<"items">> => [<<"saryn_prime_set">>],
                <<"cache_only">> => true},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(14, maps:get(<<"id">>, Reply)),
    [QuoteRow] = maps:get(<<"quotes">>, maps:get(<<"data">>, Reply)),
    ?assertEqual(<<"saryn_prime_set">>, maps:get(<<"item">>, QuoteRow)).

request_market_resolve(Socket) ->
    Request = #{<<"op">> => <<"market_resolve">>, <<"id">> => 8,
                <<"labels">> => [<<"Saryn Prme Set">>], <<"limit">> => 1},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    Reply = receive_response(Socket, 8, <<>>),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(8, maps:get(<<"id">>, Reply)),
    ?assertEqual(<<"market">>, maps:get(<<"dataset">>, Reply)),
    [Resolution] = maps:get(<<"resolutions">>, maps:get(<<"data">>, Reply)),
    ?assertEqual(<<"Saryn Prme Set">>, maps:get(<<"label">>, Resolution)),
    [Best] = maps:get(<<"matches">>, Resolution),
    ?assertEqual(<<"Saryn Prime Set">>, maps:get(<<"name">>, Best)),
    ?assertEqual(<<"saryn_prime_set">>, maps:get(<<"slug">>, Best)),
    ?assertEqual(1, maps:get(<<"distance">>, Best)).

request_market_describe(Socket) ->
    Request = #{<<"op">> => <<"market_describe">>, <<"id">> => 17,
                <<"items">> => [<<"saryn">>, <<"Saryn Prime Set">>]},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    [ById, ByName] = maps:get(<<"items">>, maps:get(<<"data">>, Reply)),
    ?assertEqual(ById, ByName),
    ?assertEqual(<<"saryn">>, maps:get(<<"id">>, ById)).

request_market_account(Socket) ->
    Request = #{<<"op">> => <<"market_account">>, <<"id">> => 18},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    Data = maps:get(<<"data">>, Reply),
    ?assertEqual(false, maps:get(<<"authenticated">>, Data)),
    ?assertMatch(#{<<"mode">> := <<"invisible">>}, maps:get(<<"presence">>, Data)).

request_overframe_account(Socket) ->
    Request = #{<<"op">> => <<"overframe_account">>, <<"id">> => 25},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(<<"overframe_account">>, maps:get(<<"dataset">>, Reply)),
    Data = maps:get(<<"data">>, Reply),
    ?assertEqual(false, maps:get(<<"authenticated">>, Data)),
    ?assertEqual(false, maps:get(<<"stale">>, Data)).

reject_invalid_overframe_session(Socket) ->
    Request = #{<<"op">> => <<"overframe_session_import">>, <<"id">> => 26},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(false, maps:get(<<"ok">>, Reply)),
    ?assertEqual(<<"invalid_overframe_session">>, maps:get(<<"error">>, Reply)).

request_market_presence(Socket) ->
    Request = #{<<"op">> => <<"market_presence_set">>, <<"id">> => 20,
                <<"mode">> => <<"auto">>},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    Reply = receive_market_presence(Socket, undefined, false),
    ?assertEqual(true, maps:get(<<"ok">>, Reply)),
    ?assertEqual(<<"auto">>,
                 maps:get(<<"mode">>, maps:get(<<"data">>, Reply))).

receive_market_presence(_Socket, Reply, true) when Reply =/= undefined ->
    Reply;
receive_market_presence(Socket, Reply, SawEvent) ->
    {ok, Chunk} = socket:recv(Socket, 0, 5000),
    {NextReply, NextSawEvent} = lists:foldl(
      fun(Line, Acc) ->
          {ok, Message} = wfcli_local_protocol:decode(Line),
          classify_market_presence(Message, Acc)
      end,
      {Reply, SawEvent},
      binary:split(Chunk, <<"\n">>, [global, trim_all])),
    receive_market_presence(Socket, NextReply, NextSawEvent).

classify_market_presence(#{<<"id">> := 20} = Reply, {_OldReply, SawEvent}) ->
    {Reply, SawEvent};
classify_market_presence(#{<<"event">> := <<"market_presence">>}, {Reply, _}) ->
    {Reply, true}.

reject_invalid_market_order(Socket) ->
    Request = #{<<"op">> => <<"market_order_create">>, <<"id">> => 19,
                <<"order">> => #{<<"type">> => <<"sell">>}},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
    {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
    {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
    ?assertEqual(false, maps:get(<<"ok">>, Reply)),
    ?assertEqual(<<"invalid_market_order">>, maps:get(<<"error">>, Reply)).

route_market_order_mutations(Socket) ->
    Requests = [
        #{<<"op">> => <<"market_order_update">>, <<"id">> => 22,
          <<"order_id">> => <<"order-1">>,
          <<"patch">> => #{<<"visible">> => true}},
        #{<<"op">> => <<"market_order_delete">>, <<"id">> => 23,
          <<"order_id">> => <<"order-1">>},
        #{<<"op">> => <<"market_order_close">>, <<"id">> => 24,
          <<"order_id">> => <<"order-1">>, <<"quantity">> => 1}
    ],
    lists:foreach(
      fun(Request) ->
          ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
          {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
          {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
          ?assertEqual(maps:get(<<"id">>, Request), maps:get(<<"id">>, Reply)),
          ?assertEqual(false, maps:get(<<"ok">>, Reply)),
          ?assertEqual(<<"Sign in to Warframe.market first">>,
                       maps:get(<<"error">>, Reply))
      end,
      Requests).

reject_invalid_market_resolve(Socket) ->
    TooMany = [<<"label">> || _ <- lists:seq(1, 21)],
    Request1 = #{<<"op">> => <<"market_resolve">>, <<"id">> => 7,
                 <<"labels">> => TooMany, <<"limit">> => 1},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request1)),
    {ok, ReplyLine1} = socket:recv(Socket, 0, 5000),
    {ok, Reply1} = wfcli_local_protocol:decode(string:trim(ReplyLine1)),
    ?assertEqual(false, maps:get(<<"ok">>, Reply1)),
    ?assertEqual(<<"invalid_market_labels">>, maps:get(<<"error">>, Reply1)),

    Request2 = #{<<"op">> => <<"market_resolve">>, <<"id">> => 6,
                 <<"labels">> => [<<"Saryn">>], <<"limit">> => 0},
    ok = socket:send(Socket, wfcli_local_protocol:encode(Request2)),
    {ok, ReplyLine2} = socket:recv(Socket, 0, 5000),
    {ok, Reply2} = wfcli_local_protocol:decode(string:trim(ReplyLine2)),
    ?assertEqual(false, maps:get(<<"ok">>, Reply2)),
    ?assertEqual(<<"invalid_market_limit">>, maps:get(<<"error">>, Reply2)).

reject_invalid_relic_limit(Socket) ->
    lists:foreach(
      fun({Id, Operation}) ->
          Request = #{<<"op">> => Operation, <<"id">> => Id,
                      <<"era">> => <<"axi">>, <<"limit">> => 0},
          ok = socket:send(Socket, wfcli_local_protocol:encode(Request)),
          {ok, ReplyLine} = socket:recv(Socket, 0, 5000),
          {ok, Reply} = wfcli_local_protocol:decode(string:trim(ReplyLine)),
          ?assertEqual(false, maps:get(<<"ok">>, Reply)),
          ?assertEqual(<<"invalid_relic_limit">>, maps:get(<<"error">>, Reply))
      end,
      [{12, <<"relic_recommendations">>}, {13, <<"relic_planner">>}]).

market_http(Url, _Headers) ->
    Body = case lists:suffix("/v2/items", Url) of
        true ->
            #{<<"error">> => null,
              <<"data">> => [#{<<"id">> => <<"saryn">>,
                               <<"slug">> => <<"saryn_prime_set">>,
                               <<"i18n">> => #{<<"en">> =>
                                                   #{<<"name">> => <<"Saryn Prime Set">>}}},
                             #{<<"id">> => <<"soma">>,
                               <<"slug">> => <<"soma_prime_set">>,
                               <<"i18n">> => #{<<"en">> =>
                                                   #{<<"name">> => <<"Soma Prime Set">>}}}]};
        false ->
            #{<<"error">> => null,
              <<"data">> => #{<<"sell">> => [#{<<"platinum">> => 42}],
                                <<"buy">> => [#{<<"platinum">> => 35}]}}
    end,
    {ok, 200, jsone:encode(Body)}.

overframe_http(_Url, _Headers) ->
    {ok, 200, jsone:encode(#{<<"user">> => null})}.

build_catalog() ->
    {ok, #{fetched_at => erlang:system_time(millisecond),
           items_by_path => #{}, items_by_id => #{},
           mods_by_id => #{}, rivens_by_id => #{}}}.

build_source(#{action := search}, _Catalog) ->
    {ok, #{<<"source">> => <<"overframe">>, <<"count">> => 1,
           <<"items">> => [#{<<"canonical_id">> =>
                                  <<"/Lotus/Powersuits/Revenant/RevenantPrime">>,
                              <<"name">> => <<"Revenant Prime">>}]}};
build_source(#{action := list}, _Catalog) ->
    {ok, #{<<"source">> => <<"overframe">>, <<"count">> => 1,
           <<"builds">> => [#{<<"identity">> =>
                                   #{<<"source">> => <<"overframe">>,
                                     <<"external_id">> => 374539},
                               <<"title">> => <<"Test Build">>}]}};
build_source(#{action := detail, id := Id}, _Catalog) ->
    {ok, #{<<"schema">> => 1,
           <<"identity">> => #{<<"source">> => <<"overframe">>,
                                <<"external_id">> => Id},
           <<"fingerprint">> => <<"test-revision">>,
           <<"content">> =>
               #{<<"item">> => <<"/Lotus/Powersuits/Excalibur/DoomSword">>,
                 <<"slots">> => []},
           <<"metadata">> => #{}, <<"raw">> => #{},
           <<"fetched_at">> => 1}}.

await_external_activity(_Expected, 0) ->
    ?assert(false);
await_external_activity(Expected, Attempts) ->
    case maps:get(external_activity, wfcli_worldstate_service:status()) of
        Expected -> ok;
        _ ->
            timer:sleep(10),
            await_external_activity(Expected, Attempts - 1)
    end.

-module(wfcli_rpc).
-export([request/1, submit/2, subscribe/2, unsubscribe/1, status/0, paths/1]).

request(player_snapshot) ->
    Snapshot = wfcli_player_service:snapshot(),
    Snapshot#{projection_summary => wfcli_player_projection:summary(Snapshot)};
request(player_clear) ->
    wfcli_player_service:clear();
request(resolution_issues) ->
    Reply = case wfcli_resolution_issues:refresh() of
        ok -> wfcli_resolution_issues:list();
        {error, _Reason} = Error -> Error
    end,
    Reply;
request(notification_settings) ->
    safe_worker_call(wfcli_notification_service,
                             fun wfcli_notification_service:settings/0);
request({notification_settings, Patch}) when is_map(Patch) ->
    Reply = case safe_worker_call(
                   wfcli_notification_service,
                   fun() -> wfcli_notification_service:update(Patch) end) of
        {ok, Settings} -> Settings;
        {error, _Reason} = Error -> Error
    end,
    Reply;
request({companion_command, Command}) when is_map(Command) ->
    retinue_local_api:companion_command(Command);
request(Request) -> {error, {unknown_request, Request}}.

submit(Client, Request) ->
    case maps:get(source, Request, worldstate) of
        Source when Source =:= worldstate; Source =:= trader; Source =:= teshin ->
            safe_worker_call(wfcli_worldstate_service,
                             fun() -> wfcli_worldstate_service:submit(Client, Request) end);
        exports -> safe_worker_call(wfcli_exports_store,
                                    fun() -> wfcli_exports_store:submit(Client, Request) end);
        query -> safe_worker_call(wfcli_query_service,
                                  fun() -> wfcli_query_service:submit(Client, Request) end);
        metadata -> safe_worker_call(wfcli_source_manager,
                                     fun() -> wfcli_source_manager:submit(Client, Request) end);
        forma -> safe_worker_call(wfcli_forma_service,
                                  fun() -> wfcli_forma_service:submit(Client, Request) end);
        build_stats -> safe_worker_call(wfcli_build_service,
                                         fun() -> wfcli_build_service:submit_cli(Client, Request) end);
        market -> safe_worker_call(wfcli_market_service,
                                   fun() -> wfcli_market_service:submit(Client, Request) end);
        companion_diagnostics -> safe_worker_call(retinue_local_api,
                                   fun() -> retinue_local_api:diagnostics_submit(Client, Request) end);
        Source -> {error, {unsupported_source, Source}}
    end.

safe_worker_call(Worker, Fun) ->
    try Fun()
    catch exit:Reason -> {error, {worker_unavailable, Worker, Reason}}
    end.

subscribe(Client, #{source := companion_diagnostics} = Request) -> submit(Client, Request);
subscribe(Client, Request) ->
    safe_worker_call(wfcli_worldstate_service,
                     fun() -> wfcli_worldstate_service:subscribe(Client, Request) end).

unsubscribe({companion_diagnostics, _} = Ref) -> retinue_local_api:diagnostics_cancel(Ref);
unsubscribe(Ref) -> wfcli_worldstate_service:unsubscribe(Ref).

status() ->
    maps:from_list([{Key, safe_status(Module)} || {Key, Module} <- [
        {worldstate, wfcli_worldstate_service}, {exports, wfcli_exports_store},
        {sources, wfcli_source_manager}, {forma, wfcli_forma_service},
        {query, wfcli_query_service}, {player, wfcli_player_service},
        {game_metadata, wfcli_game_metadata_service}, {resolution_issues, wfcli_resolution_issues},
        {market, wfcli_market_service}, {assets, wfcli_asset_service},
        {market_account, wfcli_market_account_service}, {market_presence, wfcli_market_presence_service}]]).

safe_status(Module) -> try Module:status() catch _:_ -> unavailable end.

paths(Status) ->
    Root = case maps:get(assets, Status, unavailable) of
        #{cache_root := Cache} -> Cache;
        _ -> wfcli_paths:cache_file("assets")
    end,
    wfcli_paths:directories(wfcli) ++ [{assets, Root}].

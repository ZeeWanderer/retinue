-module(wfcli_warframe_sup).
-behaviour(supervisor).
-export([start_link/0, init/1, child_specs/0, ensure_children/0]).

start_link() -> retinue_sup:start_subtree(?MODULE).
init([]) -> {ok, {#{strategy => one_for_one, intensity => 3, period => 10}, child_specs()}}.
ensure_children() -> retinue_sup:ensure_children(?MODULE, child_specs()).

child_specs() ->
    [#{
        id => wfcli_worldstate_service,
        start => {wfcli_worldstate_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_worldstate_service]
    }, #{
        id => wfcli_exports_store,
        start => {wfcli_exports_store, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_exports_store]
    }, #{
        id => wfcli_source_manager,
        start => {wfcli_source_manager, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_source_manager]
    }, #{
        id => wfcli_query_service,
        start => {wfcli_query_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_query_service]
    }, #{
        id => wfcli_forma_service,
        start => {wfcli_forma_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_forma_service]
    }, #{
        id => wfcli_player_service,
        start => {wfcli_player_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_player_service]
    }, #{
        id => wfcli_game_metadata_service,
        start => {wfcli_game_metadata_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_game_metadata_service]
    }, #{
        id => wfcli_resolution_issues,
        start => {wfcli_resolution_issues, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_resolution_issues]
    }, #{
        id => wfcli_build_service,
        start => {wfcli_build_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_build_service]
    }, #{
        id => wfcli_market_limiter,
        start => {wfcli_market_limiter, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_market_limiter]
    }, #{
        id => wfcli_market_service,
        start => {wfcli_market_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_market_service]
    }, #{
        id => wfcli_market_account_service,
        start => {wfcli_market_account_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_market_account_service]
    }, #{
        id => wfcli_market_presence_service,
        start => {wfcli_market_presence_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_market_presence_service]
    }, #{
        id => wfcli_asset_service,
        start => {wfcli_asset_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_asset_service]
    }, #{
        id => wfcli_notification_service,
        start => {wfcli_notification_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wfcli_notification_service]
    }].

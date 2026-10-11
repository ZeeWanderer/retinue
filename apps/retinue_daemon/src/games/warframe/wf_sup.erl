-module(wf_sup).
-behaviour(supervisor).
-export([start_link/0, init/1, child_specs/0, ensure_children/0]).

start_link() -> retinue_sup:start_subtree(?MODULE).
init([]) -> {ok, {#{strategy => one_for_one, intensity => 3, period => 10}, child_specs()}}.
ensure_children() -> retinue_sup:ensure_children(?MODULE, child_specs()).

child_specs() ->
    [#{
        id => wf_worldstate_service,
        start => {wf_worldstate_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_worldstate_service]
    }, #{
        id => wf_exports_store,
        start => {wf_exports_store, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_exports_store]
    }, #{
        id => wf_source_manager,
        start => {wf_source_manager, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_source_manager]
    }, #{
        id => wf_query_service,
        start => {wf_query_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_query_service]
    }, #{
        id => wf_forma_service,
        start => {wf_forma_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_forma_service]
    }, #{
        id => wf_player_service,
        start => {wf_player_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_player_service]
    }, #{
        id => wf_game_metadata_service,
        start => {wf_game_metadata_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_game_metadata_service]
    }, #{
        id => wf_resolution_issues,
        start => {wf_resolution_issues, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_resolution_issues]
    }, #{
        id => wf_build_service,
        start => {wf_build_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_build_service]
    }, #{
        id => wf_market_limiter,
        start => {wf_market_limiter, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_market_limiter]
    }, #{
        id => wf_market_service,
        start => {wf_market_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_market_service]
    }, #{
        id => wf_market_account_service,
        start => {wf_market_account_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_market_account_service]
    }, #{
        id => wf_market_presence_service,
        start => {wf_market_presence_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_market_presence_service]
    }, #{
        id => wf_asset_service,
        start => {wf_asset_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_asset_service]
    }, #{
        id => wf_notification_service,
        start => {wf_notification_service, start_link, []},
        restart => permanent,
        shutdown => 5000,
        type => worker,
        modules => [wf_notification_service]
    }].

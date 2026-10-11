-module(wf_protocol).
-export([default_datasets/0, all_datasets/0, owner/1]).

-doc "Persistent public datasets searched by a query with no dataset selector.".
-spec default_datasets() -> [atom()].
default_datasets() -> [worldstate, mods, items, codex, enemies, drops].

-doc "Every supported query dataset, including local player and market state.".
-spec all_datasets() -> [atom()].
all_datasets() -> default_datasets() ++ [player, market, diagnostics, incidents, captures].

-doc "Return daemon process that owns one request source.".
-spec owner(map()) -> atom() | undefined.
owner(#{source := Source}) when Source =:= worldstate; Source =:= trader; Source =:= teshin ->
    wf_worldstate_service;
owner(#{source := exports}) -> wf_exports_store;
owner(#{source := query}) -> wf_query_service;
owner(#{source := metadata}) -> wf_source_manager;
owner(#{source := forma}) -> wf_forma_service;
owner(#{source := build_stats}) -> wf_build_service;
owner(#{source := market}) -> wf_market_service;
owner(#{source := companion_diagnostics}) -> retinue_local_api;
owner(_) -> undefined.

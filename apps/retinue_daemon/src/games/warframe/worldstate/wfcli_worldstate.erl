%%%-------------------------------------------------------------------
%% Fetch, cache, index, and search Warframe worldstate.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate).

-export([load/1, default_cache/0, default_trader_cache/0, search/2, search_entries/2,
         index/1, raw_entry/1, load_trader_inventory/1,
         inventory_entries/2, inventory_entries/3,
         update_nodes/0, update_languages/0, update_manifest/0,
         update_export/1, update_exports/1, update_all_exports/0, update_all/0, export_files/0,
         resolver_export_files/0, codex_export_files/0, write_metadata_file/2,
         metadata_paths/1, opts/1, reindex/2]).

-define(DEFAULT_URL, "https://api.warframe.com/cdn/worldState.php").
-define(TRADER_URL, "https://content.warframe.com/dynamic/traderInventory.php").
-include_lib("retinue_daemon/include/wfcli_worldstate.hrl").

-type opts() :: map().
-type ws() :: #ws{}.
-type entry() :: map().
-type query() :: string() | binary().
-type cache_source() :: cached | cached_stale | fetched.

%% Public API
-spec load(opts()) -> {ok, ws(), cache_source()} | {error, term()}.
load(Opts) ->
    Resolve = maps:get(resolve_items, Opts, true),
    Opts1 = Opts#{resolve_items => Resolve},
    Cache = maps:get(cache, Opts1, default_cache()),
    Ttl = maps:get(ttl, Opts1, 60),
    Force = maps:get(refresh, Opts1, false),
    FetchFun = maps:get(fetch_fun, Opts1, fun fetch_remote/0),
    NowFun = maps:get(now_fun, Opts1, fun now_seconds/0),
    case retinue_data_cache:load(#{
        cache => Cache,
        ttl => Ttl,
        refresh => Force,
        fetch_fun => FetchFun,
        decode_fun => fun decode/1,
        now_fun => NowFun
    }) of
        {ok, Ws, Source} -> {ok, build_index(Ws, Opts1), Source};
        Error -> maybe_load_cached(Cache, Opts1, Error)
    end.

maybe_load_cached(Cache, Opts, Error) ->
    case file:read_file(Cache) of
        {ok, Bin} ->
            case decode(Bin) of
                {ok, Ws} -> {ok, build_index(Ws, Opts), cached_stale};
                _ -> Error
            end;
        _ -> Error
    end.

-spec default_cache() -> file:filename_all().
default_cache() ->
    wfcli_paths:cache_file("worldstate.json").

-spec default_trader_cache() -> file:filename_all().
default_trader_cache() ->
    wfcli_paths:cache_file("trader_inventory.json").

-spec search(ws(), query()) -> [entry()].
search(#ws{index = Index}, Query) ->
    Q = string:lowercase(Query),
    [E || E <- Index, matches(E, Q)].

-spec search_entries([entry()], query()) -> [entry()].
search_entries(Entries, Query) ->
    Q = string:lowercase(Query),
    [E || E <- Entries, matches(E, Q)].

-spec index(ws()) -> [entry()].
index(#ws{index = Index, raw = Raw, opts = Opts}) ->
    case Index of
        [] -> index_raw(Raw, Opts);
        _ -> Index
    end.

-doc "Expose the immutable source tree as one query-only entity for absolute raw paths.".
-spec raw_entry(ws()) -> entry().
raw_entry(#ws{raw = Raw}) ->
    #{type => raw_worldstate,
      id => "worldstate",
      name => "Raw worldstate",
      data => Raw,
      row_map => #{type => "Raw worldstate", summary => "Raw worldstate",
                   details => "Use data.<path> filters and extract=data.<path>"},
      extra_fields => #{}, fields => [], haystack => ""}.

-spec opts(ws() | term()) -> opts().
opts(#ws{opts = Opts}) -> Opts;
opts(_) -> #{}.

-doc "Rebuild option-dependent names, rows, and search fields from already-decoded worldstate.".
-spec reindex(ws(), opts()) -> ws().
reindex(#ws{} = Ws, Opts) ->
    build_index(Ws, Opts).

-spec inventory_entries(ws(), baro | prime_vault | teshin | atom()) -> [entry()].
inventory_entries(Ws, Type) ->
    inventory_entries(Ws, Type, opts(Ws)).

-spec inventory_entries(ws(), baro | prime_vault | teshin | atom(), opts()) -> [entry()].
inventory_entries(#ws{raw = Raw}, baro, Opts) ->
    VoidTraders = maps:get(<<"VoidTraders">>, Raw, []),
    lists:flatmap(fun(Map) -> void_trader_manifest_entries(Map, Opts) end, VoidTraders);
inventory_entries(#ws{raw = Raw}, prime_vault, Opts) ->
    VaultTraders = maps:get(<<"PrimeVaultTraders">>, Raw, []),
    lists:flatmap(fun(Map) -> prime_vault_manifest_entries(Map, Opts) end, VaultTraders);
inventory_entries(#ws{}, teshin, Opts) ->
    wfcli_teshin:inventory(Opts);
inventory_entries(_, _, _) ->
    [].

-spec load_trader_inventory(opts()) -> {ok, [entry()], cache_source()} | {error, term()}.
load_trader_inventory(Opts) ->
    Cache = maps:get(cache, Opts, default_trader_cache()),
    Ttl = maps:get(ttl, Opts, 60),
    Force = maps:get(refresh, Opts, false),
    FetchFun = maps:get(fetch_fun, Opts, fun fetch_trader_inventory/0),
    NowFun = maps:get(now_fun, Opts, fun now_seconds/0),
    retinue_data_cache:load(#{
        cache => Cache,
        ttl => Ttl,
        refresh => Force,
        fetch_fun => FetchFun,
        decode_fun => fun decode_trader_inventory/1,
        now_fun => NowFun
    }).

%% Internal

decode(Bin) when is_binary(Bin) ->
    try jsone:decode(Bin, [{object_format, map}]) of
        Raw when is_map(Raw) ->
            {ok, #ws{raw = Raw}}
    catch
        Class:Reason -> {error, {decode_failed, Class, Reason}}
    end;
decode(_) -> {error, bad_data}.

decode_trader_inventory(Bin) when is_binary(Bin) ->
    try jsone:decode(Bin, [{object_format, map}]) of
        Raw when is_map(Raw) ->
            List = maps:get(<<"Manifest">>, Raw, []),
            Entries = index_trader_inventory(List),
            {ok, Entries}
    catch
        Class:Reason -> {error, {decode_failed, Class, Reason}}
    end;
decode_trader_inventory(_) -> {error, bad_data}.

fetch_remote() ->
    application:ensure_all_started(inets),
    application:ensure_all_started(ssl),
    case httpc:request(get, {?DEFAULT_URL, []}, [{timeout, 10000}], [{body_format, binary}]) of
        {ok, {{_, 200, _}, _Headers, Body}} -> {ok, Body};
        {ok, {{_, Code, _}, _H, Body}} -> {error, {http_error, Code, Body}};
        {error, Reason} -> {error, Reason}
    end.

fetch_trader_inventory() ->
    application:ensure_all_started(inets),
    application:ensure_all_started(ssl),
    case httpc:request(get, {?TRADER_URL, []}, [{timeout, 10000}], [{body_format, binary}]) of
        {ok, {{_, 200, _}, _Headers, Body}} -> {ok, Body};
        {ok, {{_, Code, _}, _H, Body}} -> {error, {http_error, Code, Body}};
        {error, Reason} -> {error, Reason}
    end.

%%--------------------------------------------------------------------
%% Indexing helpers
%%--------------------------------------------------------------------
-doc "Main worldstate extension point: add new raw sections here, then build entries with stable type atoms.".
index_raw(Raw, Opts) ->
    Fissures = fissure_list(Raw),
    lists:flatten([
        index_fissures(Fissures, Opts),
        index_sorties(maps:get(<<"Sorties">>, Raw, []), Opts),
        index_lite_sorties(maps:get(<<"LiteSorties">>, Raw, []), Opts),
        index_alerts(maps:get(<<"Alerts">>, Raw, []), Opts),
        index_invasions(maps:get(<<"Invasions">>, Raw, []), Opts),
        index_events(maps:get(<<"Events">>, Raw, []), Opts),
        index_flash_sales(maps:get(<<"FlashSales">>, Raw, []), Opts),
        index_global_upgrades(maps:get(<<"GlobalUpgrades">>, Raw, []), Opts),
        index_goals(maps:get(<<"Goals">>, Raw, []), Opts),
        index_conquests(maps:get(<<"Conquests">>, Raw, []), Opts),
        index_construction_projects(maps:get(<<"ConstructionProjects">>, Raw, []), Opts),
        index_descents(maps:get(<<"Descents">>, Raw, []), Opts),
        index_circuit(maps:get(<<"EndlessXpSchedule">>, Raw, []), Opts),
        index_experiment_recommended(maps:get(<<"ExperimentRecommended">>, Raw, []), Opts),
        index_featured_guilds(maps:get(<<"FeaturedGuilds">>, Raw, []), Opts),
        index_hub_events(maps:get(<<"HubEvents">>, Raw, []), Opts),
        index_pvp_active_tournaments(maps:get(<<"PVPActiveTournaments">>, Raw, []), Opts),
        index_pvp_alternative_modes(maps:get(<<"PVPAlternativeModes">>, Raw, []), Opts),
        index_pvp_challenges(maps:get(<<"PVPChallengeInstances">>, Raw, []), Opts),
        index_persistent_enemies(maps:get(<<"PersistentEnemies">>, Raw, []), Opts),
        index_project_pct(maps:get(<<"ProjectPct">>, Raw, []), Opts),
        index_prime_vault_availabilities(maps:get(<<"PrimeVaultAvailabilities">>, Raw, []), Opts),
        index_syndicate_missions(maps:get(<<"SyndicateMissions">>, Raw, []), Opts),
        index_daily_deals(maps:get(<<"DailyDeals">>, Raw, []), Opts),
        index_prime_vault(maps:get(<<"PrimeVaultTraders">>, Raw, []), Opts),
        index_arbitration(maps:get(<<"Arbitration">>, Raw, undefined), Opts),
        index_baro(maps:get(<<"VoidTraders">>, Raw, []), Opts),
        index_void_storms(maps:get(<<"VoidStorms">>, Raw, []), Opts),
        index_calendar(maps:get(<<"KnownCalendarSeasons">>, Raw, []), Opts),
        index_season_info(maps:get(<<"SeasonInfo">>, Raw, undefined), Opts),
        index_in_game_market(maps:get(<<"InGameMarket">>, Raw, undefined), Opts),
        index_library_info(maps:get(<<"LibraryInfo">>, Raw, undefined), Opts),
        index_node_overrides(maps:get(<<"NodeOverrides">>, Raw, []), Opts),
        index_prime_access(maps:get(<<"PrimeAccessAvailability">>, Raw, undefined), Opts),
        index_prime_token(maps:get(<<"PrimeTokenAvailability">>, Raw, undefined), Opts),
        index_sku_sales(maps:get(<<"SkuSales">>, Raw, []), Opts),
        index_twitch_promos(maps:get(<<"TwitchPromos">>, Raw, []), Opts),
        index_meta(Raw, Opts)
    ]).

build_index(#ws{raw = Raw} = Ws, Opts) ->
    Ws#ws{opts = Opts, index = index_raw(Raw, Opts)}.

fissure_list(Raw) ->
    case maps:get(<<"VoidFissures">>, Raw, undefined) of
        List when is_list(List), List =/= [] -> List;
        _ -> maps:get(<<"ActiveMissions">>, Raw, [])
    end.

index_fissures(List, Opts) ->
    [build_entry(fissure, Map, Opts, oid(Map), fissure_name(Map, Opts))
     || Map <- List, fissure_active(Map)].

fissure_active(#{<<"Active">> := true}) -> true;
fissure_active(_) -> true.

fissure_name(Map, Opts) ->
    Mod = maps:get(<<"Modifier">>, Map, <<"Unknown">>),
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, Map, <<"Unknown">>), Opts),
    Tier = wfcli_worldstate_projector:to_list(Mod),
    lists:flatten(io_lib:format("~s - ~s", [Tier, Node])).

-doc "Convert one raw worldstate map into the normalized entity shape used by query/render code.".
build_entry(Type, Map, Opts, Id, Name) ->
    wfcli_entity_worldstate:build(Type, Id, Name, Map, Opts).

index_sorties(List, Opts) ->
    [build_entry(sortie, Map, Opts, oid(Map),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Boss">>, Map, <<"Sortie">>)))
     || Map <- List].

index_lite_sorties(List, Opts) ->
    [build_entry(lite_sortie, Map, Opts, oid_or_index(Map, "lite_sortie", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Boss">>, Map, <<"Lite Sortie">>)))
     || {Idx, Map} <- with_index(List)].

index_alerts(List, Opts) ->
    [build_entry(alert, Map, Opts, oid(Map),
                 wfcli_resolve:resolve("node", maps:get(<<"Node">>, Map, <<"Alert">>), Opts))
     || Map <- List].

index_invasions(List, Opts) ->
    [build_entry(invasion, Map, Opts, oid(Map),
                 wfcli_resolve:resolve("node", maps:get(<<"Node">>, Map, <<"Invasion">>), Opts))
     || Map <- List].

index_events(List, Opts) ->
    [build_entry(event, Map, Opts, oid(Map), event_name(Map, Opts)) || Map <- List].

index_flash_sales(List, Opts) ->
    [build_entry(flash_sale, Map, Opts, oid_or_index(Map, "flash_sale", Idx),
                 wfcli_resolve:resolve("item", maps:get(<<"TypeName">>, Map, <<"Unknown">>), Opts))
     || {Idx, Map} <- with_index(List)].

event_name(Map, Opts) ->
    wfcli_worldstate_projector:event_name(Map, Opts).

index_global_upgrades(List, Opts) ->
    [build_entry(global_upgrade, Map, Opts, oid(Map), global_upgrade_name(Map, Opts))
     || Map <- List].

index_goals(List, Opts) ->
    [build_entry(goal, Map, Opts, oid_or_index(Map, "goal", Idx),
                 wfcli_worldstate_projector:goal_name(Map, Opts))
     || {Idx, Map} <- with_index(List)].

index_conquests(List, Opts) ->
    [build_entry(archimedea, Map, Opts, oid_or_index(Map, "archimedea", Idx),
                 wfcli_archimedea:name(Map))
     || {Idx, Map} <- with_index(List)].

index_construction_projects(List, Opts) ->
    [build_entry(construction_project, Map, Opts, oid_or_index(Map, "construction_project", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Node">>, Map, <<"Construction Project">>)))
     || {Idx, Map} <- with_index(List)].

-doc "Index Duviri/undercroft descent state when the official worldstate exposes it.".
index_descents(List, Opts) ->
    [build_entry(descent, Map, Opts, oid_or_index(Map, "descent", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"RandSeed">>, Map, <<"Descent">>)))
     || {Idx, Map} <- with_index(List)].

index_circuit(List, Opts) ->
    [build_entry(circuit,
                 maps:merge(maps:with([<<"Activation">>, <<"Expiry">>], Schedule), Choice),
                 Opts, oid_or_index(Schedule, "circuit", Idx) ++ ":" ++
                           retinue_text:to_list(Category),
                 wfcli_worldstate_projector:circuit_category(Category))
     || {Idx, #{<<"CategoryChoices">> := Choices} = Schedule} <- with_index(List),
        is_list(Choices),
        #{<<"Category">> := Category, <<"Choices">> := Rewards} = Choice <- Choices,
        is_binary(Category), is_list(Rewards)].

index_experiment_recommended(List, Opts) ->
    [build_entry(experiment_recommended, Map, Opts, oid_or_index(Map, "experiment_recommended", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Tag">>, Map, <<"Experiment">>)))
     || {Idx, Map} <- with_index(List)].

index_featured_guilds(List, Opts) ->
    [build_entry(featured_guild, Map, Opts, oid_or_index(Map, "featured_guild", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Name">>, Map, <<"Featured Guild">>)))
     || {Idx, Map} <- with_index(List)].

index_hub_events(List, Opts) ->
    [build_entry(hub_event, Map, Opts, oid_or_index(Map, "hub_event", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Tag">>, Map, <<"Hub Event">>)))
     || {Idx, Map} <- with_index(List)].

index_pvp_active_tournaments(List, Opts) ->
    [build_entry(pvp_active_tournament, Map, Opts, oid_or_index(Map, "pvp_active_tournament", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"PVPMode">>, Map, <<"PVP Tournament">>)))
     || {Idx, Map} <- with_index(List)].

index_pvp_alternative_modes(List, Opts) ->
    [build_entry(pvp_alternative_mode, Map, Opts, oid_or_index(Map, "pvp_alternative_mode", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Name">>, Map, <<"PVP Mode">>)))
     || {Idx, Map} <- with_index(List)].

index_pvp_challenges(List, Opts) ->
    [build_entry(pvp_challenge, Map, Opts, oid_or_index(Map, "pvp_challenge", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Category">>, Map, <<"PVP Challenge">>)))
     || {Idx, Map} <- with_index(List)].

index_persistent_enemies(List, Opts) ->
    [build_entry(persistent_enemy, Map, Opts, oid_or_index(Map, "persistent_enemy", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"AgentType">>, Map, <<"Persistent Enemy">>)))
     || {Idx, Map} <- with_index(List)].

global_upgrade_name(Map, Opts) ->
    wfcli_worldstate_projector:global_upgrade_name(Map, Opts).

index_syndicate_missions(List, Opts) ->
    [build_entry(syndicate_mission, Map, Opts, oid(Map), syndicate_name(Map, Opts))
     || Map <- List].

syndicate_name(Map, Opts) ->
    wfcli_worldstate_projector:syndicate_name(Map, Opts).

index_daily_deals(List, Opts) ->
    [build_entry(daily_deal, Map, Opts, daily_deal_id(Map), daily_deal_name(Map, Opts))
     || Map <- List].

daily_deal_id(Map) ->
    case maps:get(<<"StoreItem">>, Map, undefined) of
        undefined -> "daily_deal";
        Item -> wfcli_worldstate_projector:to_list(Item)
    end.

daily_deal_name(Map, Opts) ->
    wfcli_worldstate_projector:daily_deal_name(Map, Opts).

index_trader_inventory(List) ->
    [build_entry(void_trader_item, Map, #{resolve_items => true}, trader_item_id(Map), trader_item_name(Map))
     || Map <- List].

trader_item_id(Map) ->
    case maps:get(<<"ItemType">>, Map, undefined) of
        undefined -> "unknown";
        Item -> wfcli_worldstate_projector:to_list(Item)
    end.

trader_item_name(Map) ->
    wfcli_resolve:resolve("item", maps:get(<<"ItemType">>, Map, <<"Unknown">>),
                                             #{resolve_items => true}).

index_prime_vault(List, Opts) ->
    [build_entry(prime_vault, Map, Opts, oid(Map), prime_vault_name(Map)) || Map <- List].

prime_vault_name(_Map) ->
    "Prime Vault".

index_arbitration(undefined, _Opts) -> [];
index_arbitration(Map, Opts) when is_map(Map) ->
    [build_entry(arbitration, Map, Opts, oid(Map),
                 wfcli_resolve:resolve("node", maps:get(<<"Node">>, Map, <<"Arbitration">>), Opts))];
index_arbitration(_, _Opts) -> [].

index_baro(List, Opts) ->
    [build_entry(baro, Map, Opts, oid(Map), baro_name(Map)) || Map <- List].

baro_name(Map) ->
    wfcli_worldstate_projector:to_list(maps:get(<<"Character">>, Map, <<"Baro Ki'Teer">>)).

index_void_storms(List, Opts) ->
    [build_entry(void_storm, Map, Opts, oid(Map), void_storm_name(Map, Opts))
     || Map <- List].

void_storm_name(Map, Opts) ->
    Node = wfcli_resolve:resolve("node", maps:get(<<"Node">>, Map, <<"Unknown">>), Opts),
    Tier = wfcli_worldstate_projector:to_list(maps:get(<<"ActiveMissionTier">>, Map, <<"Unknown">>)),
    lists:flatten(io_lib:format("~s - ~s", [Tier, Node])).

-doc "Flatten KnownCalendarSeasons into one searchable entry per season day.".
index_calendar(List, Opts) ->
    lists:flatten([index_calendar_season(Map, Opts) || Map <- List]).

-doc "Index global season metadata; keep separate from calendar day entries.".
index_season_info(SeasonInfo, Opts) when is_map(SeasonInfo) ->
    [build_entry(season_info, SeasonInfo, Opts, "season_info",
                 wfcli_worldstate_projector:season_info_name(SeasonInfo, Opts))];
index_season_info(_, _Opts) -> [].

index_in_game_market(Map, Opts) when is_map(Map) ->
    [build_entry(in_game_market, Map, Opts, "in_game_market",
                 wfcli_worldstate_projector:in_game_market_name(Map, Opts))];
index_in_game_market(_, _Opts) -> [].

index_library_info(Map, Opts) when is_map(Map) ->
    [build_entry(library_info, Map, Opts, "library_info",
                 wfcli_worldstate_projector:library_info_name(Map, Opts))];
index_library_info(_, _Opts) -> [].

index_node_overrides(List, Opts) ->
    [build_entry(node_override, Map, Opts, oid_or_index(Map, "node_override", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Node">>, Map, <<"Node Override">>)))
     || {Idx, Map} <- with_index(List)].

index_prime_access(Map, Opts) when is_map(Map) ->
    [build_entry(prime_access, Map, Opts, "prime_access",
                 wfcli_worldstate_projector:to_list(maps:get(<<"State">>, Map, <<"Prime Access">>)))];
index_prime_access(_, _Opts) -> [].

index_prime_token(Val, Opts) when is_boolean(Val) ->
    [build_entry(prime_token, #{<<"PrimeTokenAvailability">> => Val}, Opts,
                 "prime_token", wfcli_worldstate_projector:to_list(Val))];
index_prime_token(_, _Opts) -> [].

index_project_pct(List, Opts) ->
    [build_entry(project_pct, #{<<"Index">> => Idx, <<"Pct">> => Val}, Opts,
                 indexed_id("project_pct", Idx), wfcli_worldstate_projector:to_list(Idx))
     || {Idx, Val} <- with_index(List)].

index_prime_vault_availabilities(List, Opts) ->
    [build_entry(prime_vault_availability, #{<<"Index">> => Idx, <<"Available">> => Val}, Opts,
                 indexed_id("prime_vault_availability", Idx), wfcli_worldstate_projector:to_list(Idx))
     || {Idx, Val} <- with_index(List)].

index_sku_sales(List, Opts) ->
    [build_entry(sku_sale, Map, Opts, oid_or_index(Map, "sku_sale", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"SaleType">>, Map, <<"SKU Sale">>)))
     || {Idx, Map} <- with_index(List)].

index_twitch_promos(List, Opts) ->
    [build_entry(twitch_promo, Map, Opts, oid_or_index(Map, "twitch_promo", Idx),
                 wfcli_worldstate_projector:to_list(maps:get(<<"Name">>, Map, <<"Twitch Promo">>)))
     || {Idx, Map} <- with_index(List)].

-doc "Expose build/version/time seed metadata for debugging stale or surprising worldstate data.".
index_meta(Raw, Opts) ->
    Meta = [
        {<<"BuildLabel">>, maps:get(<<"BuildLabel">>, Raw, undefined)},
        {<<"Version">>, maps:get(<<"Version">>, Raw, undefined)},
        {<<"MobileVersion">>, maps:get(<<"MobileVersion">>, Raw, undefined)},
        {<<"ForceLogoutVersion">>, maps:get(<<"ForceLogoutVersion">>, Raw, undefined)},
        {<<"Time">>, maps:get(<<"Time">>, Raw, undefined)},
        {<<"WorldSeed">>, maps:get(<<"WorldSeed">>, Raw, undefined)},
        {<<"Tmp">>, maps:get(<<"Tmp">>, Raw, undefined)}
    ],
    [build_entry(meta, #{Key => Val}, Opts,
                 wfcli_worldstate_projector:to_list(Key),
                 wfcli_worldstate_projector:to_list(Key))
     || {Key, Val} <- Meta, Val =/= undefined].

index_calendar_season(Map, Opts) when is_map(Map) ->
    Season = maps:get(<<"Season">>, Map, <<"Unknown">>),
    Activation = maps:get(<<"Activation">>, Map, undefined),
    Expiry = maps:get(<<"Expiry">>, Map, undefined),
    Days = maps:get(<<"Days">>, Map, []),
    [calendar_entry(Season, Activation, Expiry, DayMap, Opts)
     || DayMap <- Days, is_map(DayMap)];
index_calendar_season(_, _Opts) ->
    [].

calendar_entry(Season, Activation, Expiry, DayMap, Opts) ->
    Day = maps:get(<<"day">>, DayMap, <<"Unknown">>),
    Events = maps:get(<<"events">>, DayMap, []),
    Data = #{
        <<"Season">> => Season,
        <<"Day">> => Day,
        <<"Events">> => Events,
        <<"Activation">> => Activation,
        <<"Expiry">> => Expiry
    },
    Name = calendar_name(Season, Day),
    build_entry(calendar, Data, Opts, calendar_id(Season, Day), Name).

calendar_name(Season, Day) ->
    SeasonStr = wfcli_worldstate_projector:to_list(Season),
    DayStr = wfcli_worldstate_projector:to_list(Day),
    lists:flatten(io_lib:format("Calendar ~s day ~s", [SeasonStr, DayStr])).

calendar_id(Season, Day) ->
    SeasonStr = wfcli_worldstate_projector:to_list(Season),
    DayStr = wfcli_worldstate_projector:to_list(Day),
    lists:flatten(io_lib:format("calendar_~s_~s", [SeasonStr, DayStr])).

%%--------------------------------------------------------------------
%% Inventory extraction helpers
%%--------------------------------------------------------------------
void_trader_manifest_entries(Map, Opts) when is_map(Map) ->
    Entries = manifest_list([<<"Manifest">>, <<"Inventory">>], Map),
    [make_inventory_entry(void_trader_item, Entry, Opts) || Entry <- Entries];
void_trader_manifest_entries(_, _) ->
    [].

prime_vault_manifest_entries(Map, Opts) when is_map(Map) ->
    Manifest = manifest_list([<<"Manifest">>], Map),
    Evergreen = [E#{<<"Evergreen">> => true} || E <- manifest_list([<<"EvergreenManifest">>], Map)],
    Entries = Manifest ++ Evergreen,
    [make_inventory_entry(prime_vault_item, Entry, Opts) || Entry <- Entries];
prime_vault_manifest_entries(_, _) ->
    [].

manifest_list([], _) -> [];
manifest_list([Key | Rest], Map) ->
    case maps:get(Key, Map, undefined) of
        List when is_list(List) -> List;
        _ -> manifest_list(Rest, Map)
    end.

-doc "Normalize Baro/Prime Vault manifest entries and resolve ItemType when metadata is available.".
make_inventory_entry(Type, Entry, Opts) when is_map(Entry) ->
    ItemType = maps:get(<<"ItemType">>, Entry, undefined),
    Id = case ItemType of
        undefined -> "unknown";
        _ -> wfcli_worldstate_projector:to_list(ItemType)
    end,
    Name = wfcli_resolve:resolve("item", maps:get(<<"ItemType">>, Entry, <<"Unknown">>), Opts),
    build_entry(Type, Entry, Opts, Id, Name);
make_inventory_entry(Type, Entry, _Opts) ->
    #{type => Type, id => "unknown", name => "Unknown", data => Entry}.

oid(Map) ->
    case maps:get(<<"_id">>, Map, undefined) of
        #{<<"$oid">> := Oid} -> wfcli_worldstate_projector:to_list(Oid);
        Oid when is_binary(Oid); is_list(Oid) -> wfcli_worldstate_projector:to_list(Oid);
        _ -> "unknown"
    end.

with_index(List) ->
    lists:zip(lists:seq(1, length(List)), List).

indexed_id(Prefix, Index) ->
    lists:flatten(io_lib:format("~s_~p", [Prefix, Index])).

oid_or_index(Map, Prefix, Index) ->
    case oid(Map) of
        "unknown" -> indexed_id(Prefix, Index);
        Id -> Id
    end.

now_seconds() ->
    calendar:datetime_to_gregorian_seconds(calendar:universal_time()).

update_nodes() ->
    wfcli_metadata_update:update_nodes().

update_languages() ->
    wfcli_metadata_update:update_languages().

update_manifest() ->
    wfcli_metadata_update:update_manifest().

update_all_exports() ->
    wfcli_metadata_update:update_all_exports().

update_exports(Files) ->
    wfcli_metadata_update:update_exports(Files).

update_export(File) ->
    wfcli_metadata_update:update_export(File).

update_all() ->
    wfcli_metadata_update:update_all().

write_metadata_file(Name, Body) when is_list(Name), is_binary(Body) ->
    wfcli_metadata_update:write_file(Name, Body).

metadata_paths(Name) ->
    wfcli_metadata_update:paths(Name).

export_files() ->
    wfcli_metadata_update:export_files().

resolver_export_files() ->
    wfcli_metadata_update:resolver_export_files().

codex_export_files() ->
    wfcli_metadata_update:codex_export_files().

matches(#{name := Name, type := Type} = E, Q) ->
    NameMatch = string:find(string:lowercase(Name), Q) =/= nomatch,
    TypeMatch = string:find(atom_to_list(Type), Q) =/= nomatch,
    HayMatch =
        case maps:get(haystack, E, undefined) of
            undefined -> false;
            H when is_list(H) -> string:find(H, Q) =/= nomatch;
            _ -> false
        end,
    NameMatch orelse TypeMatch orelse HayMatch.

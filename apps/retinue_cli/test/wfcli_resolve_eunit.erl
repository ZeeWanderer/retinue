%%%-------------------------------------------------------------------
%% EUnit tests for unified resolver.
%%%-------------------------------------------------------------------
-module(wfcli_resolve_eunit).

-include_lib("eunit/include/eunit.hrl").

resolve_mission_type_test() ->
    ?assertEqual("Defense", wfcli_resolve:resolve("MissionType", "MT_DEFENSE", #{resolve_items => true})),
    ?assertEqual("Interception", wfcli_resolve:resolve("MissionType", "MT_TERRITORY", #{resolve_items => true})),
    ?assertEqual("Hive Sabotage", wfcli_resolve:resolve("MissionType", "MT_HIVE", #{resolve_items => true})),
    ?assertEqual("Legacyte Harvest", wfcli_resolve:resolve("MissionType", "MT_ENDLESS_CAPTURE", #{resolve_items => true})),
    ?assertEqual("Disruption", wfcli_resolve:resolve("MissionType", "MT_ARTIFACT", #{resolve_items => true})),
    ?assertEqual("Defection", wfcli_resolve:resolve("MissionType", "MT_EVACUATION", #{resolve_items => true})),
    ?assertEqual("MT_FUTURE_MODE", wfcli_resolve:resolve("MissionType", "MT_FUTURE_MODE", #{resolve_items => true})).

resolve_modifier_test() ->
    ?assertEqual("Meso", wfcli_resolve:resolve("Modifier", "VoidT2", #{resolve_items => true})).

resolve_faction_test() ->
    ?assertEqual("Grineer", wfcli_resolve:resolve("Faction", "FC_GRINEER", #{resolve_items => true})).

resolve_case_insensitive_tokens_test() ->
    ?assertEqual("Defense", wfcli_resolve:resolve("missiontype", "mt_defense", #{resolve_items => true})),
    ?assertEqual("Meso", wfcli_resolve:resolve("modifier", "voidt2", #{resolve_items => true})),
    ?assertEqual("Grineer", wfcli_resolve:resolve("faction", "fc_grineer", #{resolve_items => true})),
    ?assertEqual("Slash", wfcli_resolve:resolve("dt", "dt_slash", #{resolve_items => true, raw => false})).

resolve_node_case_insensitive_test() ->
    Prev = persistent_term:get({wfcli, node_map}, undefined),
    persistent_term:put({wfcli, node_map}, #{<<"solnode">> => <<"Earth (Test)">>}),
    try
        ?assertEqual("Earth (Test)", wfcli_resolve:resolve("node", "solnode", #{resolve_items => true}))
    after
        restore_pt({wfcli, node_map}, Prev)
    end.

resolve_node_loads_daemon_priv_map_test() ->
    Prev = persistent_term:get({wfcli, node_map}, undefined),
    persistent_term:erase({wfcli, node_map}),
    try
        ?assertEqual("Stribog (Void)",
                     wfcli_resolve:resolve("node", "SolNode404", #{resolve_items => true})),
        ?assertEqual("Vesper Relay (Venus)",
                     wfcli_resolve:resolve("node", "SolNode239", #{resolve_items => true}))
    after
        restore_pt({wfcli, node_map}, Prev)
    end.

resolve_hidden_sanctuary_nodes_test() ->
    Prev = persistent_term:get({wfcli, node_map}, undefined),
    persistent_term:put({wfcli, node_map}, #{}),
    try
        ?assertEqual("Sanctuary Onslaught",
                     wfcli_resolve:resolve("node", "SolNode801", #{resolve_items => true})),
        ?assertEqual("Elite Sanctuary Onslaught",
                     wfcli_resolve:resolve("node", "SolNode802", #{resolve_items => true}))
    after
        restore_pt({wfcli, node_map}, Prev)
    end.

resolve_any_raw_test() ->
    ?assertEqual("MT_DEFENSE", wfcli_resolve:resolve("any", "MT_DEFENSE", #{resolve_items => false})).

resolve_dt_humanize_test() ->
    ?assertEqual("Slash", wfcli_resolve:resolve("dt", "DT_SLASH", #{resolve_items => true, raw => false})).

resolve_dt_raw_test() ->
    ?assertEqual("DT_SLASH", wfcli_resolve:resolve("dt", "DT_SLASH", #{resolve_items => true, raw => true})).

resolve_language_map_overrides_test() ->
    Prev = persistent_term:get({wfcli, lang_map}, undefined),
    persistent_term:put({wfcli, lang_map}, #{<<"mt_defense">> => #{<<"value">> => <<"Defensa">>}}),
    try
        ?assertEqual("Defensa", wfcli_resolve:resolve("missiontype", "MT_DEFENSE", #{resolve_items => true}))
    after
        restore_pt({wfcli, lang_map}, Prev)
    end.

resolve_item_name_fallback_rewrites_test() ->
    Prev = persistent_term:get({wfcli, item_map}, undefined),
    ItemKey = <<"/lotus/types/test/item">>,
    persistent_term:put({wfcli, item_map}, #{ItemKey => <<"Test Item">>}),
    try
        StorePath = "/Lotus/StoreItems/Types/Test/Item",
        ?assertEqual("Test Item", wfcli_resolve:resolve("item", StorePath, #{resolve_items => true}))
    after
        restore_pt({wfcli, item_map}, Prev)
    end.

resolve_item_name_case_insensitive_test() ->
    Prev = persistent_term:get({wfcli, item_map}, undefined),
    persistent_term:put({wfcli, item_map}, #{<<"/lotus/types/test/item">> => <<"Case Item">>}),
    try
        ?assertEqual("Case Item", wfcli_resolve:resolve("item", "/lotus/types/test/item", #{resolve_items => true}))
    after
        restore_pt({wfcli, item_map}, Prev)
    end.

resolve_item_name_language_lowercase_test() ->
    PrevLang = persistent_term:get({wfcli, lang_map}, undefined),
    persistent_term:put({wfcli, lang_map}, #{
        <<"/lotus/storeitems/upgrades/skins/meleedangles/firemeleedangle">> =>
            #{<<"value">> => <<"Fire Melee Dangle">>}
    }),
    PrevItem = persistent_term:get({wfcli, item_map}, undefined),
    persistent_term:put({wfcli, item_map}, #{}),
    try
        Name = wfcli_resolve:resolve(
            "item",
            "/Lotus/StoreItems/Upgrades/Skins/MeleeDangles/FireMeleeDangle",
            #{resolve_items => true}
        ),
        ?assertEqual("Fire Melee Dangle", Name)
    after
        restore_pt({wfcli, lang_map}, PrevLang),
        restore_pt({wfcli, item_map}, PrevItem)
    end.

resolve_unicode_list_item_test() ->
    Word = [69,112,105,99,32,71,97,109,101,115,32,72,101,115,97,112,32,66,97,351,108,97,109,97],
    ?assertEqual(Word, wfcli_resolve:resolve("item", Word, #{resolve_items => true})).

invalidate_mod_cache_clears_current_and_legacy_keys_test() ->
    Keys = [{wfcli, mod_map, 2}, {wfcli, mod_name_index, 2},
            {wfcli, mod_map}, {wfcli, mod_name_index}],
    Previous = [{Key, persistent_term:get(Key, undefined)} || Key <- Keys],
    lists:foreach(fun(Key) -> persistent_term:put(Key, #{test => true}) end, Keys),
    try
        ok = wfcli_resolve_registry:invalidate_mod_cache(),
        lists:foreach(
          fun(Key) -> ?assertEqual(undefined,
                                   persistent_term:get(Key, undefined)) end,
          Keys)
    after
        lists:foreach(fun({Key, Value}) -> restore_pt(Key, Value) end, Previous)
    end.

restore_pt(Key, undefined) ->
    persistent_term:erase(Key);
restore_pt(Key, Val) ->
    persistent_term:put(Key, Val).

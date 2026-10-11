%%%-------------------------------------------------------------------
%% EUnit tests for export entity builders.
%%%-------------------------------------------------------------------
-module(wfcli_entity_exports_eunit).

-include_lib("eunit/include/eunit.hrl").

mod_effects_row_map_test() ->
    Mod = #{
        name => "Test Mod",
        uniqueName => "/Lotus/Upgrades/Mods/Test",
        effects => ["+10% Damage", "+20% Heat"],
        polarity => "AP_ATTACK"
    },
    Entry = wfcli_entity_exports:build_mod(Mod, #{}),
    RowMap = maps:get(row_map, Entry, #{}),
    ?assertEqual("+10% Damage; +20% Heat", maps:get(effects, RowMap)),
    ?assertEqual("Test Mod", maps:get(name, RowMap)),
    ?assertEqual("AP_ATTACK", maps:get(polarity, RowMap)),
    Haystack = maps:get(haystack, Entry, ""),
    ?assert(string:find(Haystack, "heat") =/= nomatch).

item_row_map_fields_test() ->
    Item = #{
        name => "Test Item",
        file => "ExportWeapons_en.json",
        masteryReq => 5,
        abilities => ["One", "Two"],
        uniqueName => "/Lotus/Weapons/Test"
    },
    Entry = wfcli_entity_exports:build_item(Item, #{}),
    RowMap = maps:get(row_map, Entry, #{}),
    ?assertEqual("Test Item", maps:get(name, RowMap)),
    ?assertEqual("One, Two", maps:get(abilities, RowMap)),
    ?assertEqual(5, maps:get(masteryReq, RowMap)),
    ?assertEqual("/Lotus/Weapons/Test", maps:get(uniqueName, RowMap)).

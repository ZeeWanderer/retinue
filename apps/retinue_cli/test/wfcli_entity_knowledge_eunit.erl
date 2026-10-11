%%%-------------------------------------------------------------------
%% EUnit tests for static knowledge entity schemas.
%%%-------------------------------------------------------------------
-module(wfcli_entity_knowledge_eunit).

-include_lib("eunit/include/eunit.hrl").

enemy_entity_is_searchable_test() ->
    Enemy = #{name => "Test Lancer", uniqueName => "/Enemy/Test",
              faction => "Grineer", health => 100, shield => 0, armor => 50,
              resistances => "Slash, Heat", dropCount => 1,
              description => "Fixture unit"},
    Entry = wfcli_entity_knowledge:build_enemy(Enemy, #{search_raw => true}),
    Row = maps:get(row_map, Entry),
    ?assertEqual("Grineer", maps:get(faction, Row)),
    ?assertEqual(100, maps:get(health, Row)),
    ?assert(string:find(maps:get(haystack, Entry), "fixture unit") =/= nomatch).

drop_entity_preserves_numeric_chance_test() ->
    Drop = #{item => "Test Mod", enemy => "Test Lancer", enemyUniqueName => "/Enemy/Test",
             chance => 0.125, rarity => "Rare", table => "Mod Locations", index => 1},
    Entry = wfcli_entity_knowledge:build_drop(Drop, #{search_raw => true}),
    ?assertEqual(0.125, maps:get(chance, maps:get(row_map, Entry))),
    ?assert(string:find(maps:get(haystack, Entry), "test lancer") =/= nomatch),
    ?assertEqual(location, maps:get(role, wfcli_entity_knowledge:column_spec(enemy))).

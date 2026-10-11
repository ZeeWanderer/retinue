%%%-------------------------------------------------------------------
%% EUnit tests for official and optional knowledge loaders.
%%%-------------------------------------------------------------------
-module(wfcli_knowledge_eunit).

-include_lib("eunit/include/eunit.hrl").

loads_codex_flags_and_provenance_test() ->
    {ok, Entries, Meta} = wfcli_knowledge:load_codex(fixture_exports()),
    Visible = find_name("Visible Codex Entry", Entries),
    Hidden = find_name("Excluded Codex Entry", Entries),
    ?assertEqual(true, maps:get(codexSecret, Visible)),
    ?assertEqual(true, maps:get(excludeFromCodex, Hidden)),
    ?assertEqual("official PublicExport", maps:get(source, Meta)),
    ?assertEqual(64, length(maps:get(version, Meta))).

loads_versioned_enemy_and_reverse_drops_test() ->
    {ok, Enemies, Meta} = wfcli_knowledge:load_enemies(fixture_knowledge()),
    Lancer = find_name("Test Lancer", Enemies),
    ?assertEqual("Grineer", maps:get(faction, Lancer)),
    ?assertEqual("fixture-sha256", maps:get(version, Meta)),
    {ok, Drops, Meta} = wfcli_knowledge:load_drops(fixture_knowledge()),
    ?assertEqual(2, length(Drops)),
    TestMod = find_item("Test Mod", Drops),
    ?assertEqual("Test Lancer", maps:get(enemy, TestMod)),
    ?assertEqual(0.125, maps:get(chance, TestMod)).

find_name(Name, Entries) ->
    hd([E || E <- Entries, maps:get(name, E) =:= Name]).

find_item(Name, Entries) ->
    hd([E || E <- Entries, maps:get(item, E) =:= Name]).

fixture_exports() -> filename:join(["apps", "retinue_cli", "test", "fixtures", "exports"]).
fixture_knowledge() -> filename:join(["apps", "retinue_cli", "test", "fixtures", "knowledge"]).

%%%-------------------------------------------------------------------
%% EUnit tests for worldstate formatting helpers.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_format_eunit).

-include_lib("eunit/include/eunit.hrl").

mission_type_map_test() ->
    Entry = #{type => fissure,
              data => #{<<"Modifier">> => <<"VoidT1">>,
                       <<"MissionType">> => <<"MT_EXCAVATE">>,
                       <<"Node">> => <<"SolNode144">>}},
    Row = wfcli_worldstate_projector:table_row_map(Entry, #{resolve_items => true}),
    ?assertEqual("Excavation", maps:get(mission, Row)).

sortie_resolution_test() ->
    Entry = #{type => sortie,
              name => <<"Sortie">>,
              data => #{<<"Boss">> => <<"SORTIE_BOSS_PHORID">>,
                        <<"Variants">> => [#{<<"missionType">> => <<"MT_EXCAVATE">>,
                                            <<"node">> => <<"SolNode144">>,
                                            <<"modifierType">> => <<"SORTIE_MODIFIER_EXIMUS">>}]}},
    Row = wfcli_worldstate_projector:table_row_map(Entry, #{resolve_items => true}),
    ?assertEqual("Phorid", maps:get(boss, Row)),
    ?assert(string:find(maps:get(modifiers, Row), "Eximus") =/= nomatch),
    ?assert(string:find(maps:get(stages, Row), "Excavation") =/= nomatch).

sortie_modifier_fallback_test() ->
    Entry = #{type => sortie,
              name => <<"Sortie">>,
              data => #{<<"Boss">> => <<"SORTIE_BOSS_PHORID">>,
                        <<"Variants">> => [#{<<"modifierType">> => <<"SORTIE_MODIFIER_UNKNOWN">>}]}},
    Row = wfcli_worldstate_projector:table_row_map(Entry, #{resolve_items => true}),
    ?assert(string:find(maps:get(modifiers, Row), "SORTIE_MODIFIER_UNKNOWN") =/= nomatch).

archimedea_projection_and_semantic_query_test() ->
    Data = #{
        <<"Type">> => <<"CT_LAB">>,
        <<"RandomSeed">> => 157125,
        <<"Missions">> => [#{
            <<"faction">> => <<"FC_MITW">>,
            <<"missionType">> => <<"MT_EXTERMINATION">>,
            <<"difficulties">> => [
                #{<<"type">> => <<"CD_NORMAL">>, <<"deviation">> => <<"FortifiedFoes">>,
                  <<"risks">> => [<<"ShieldedFoes">>]},
                #{<<"type">> => <<"CD_HARD">>, <<"deviation">> => <<"FortifiedFoes">>,
                  <<"risks">> => [<<"ShieldedFoes">>, <<"AntiMaterialWeapons">>]}
            ]
        }],
        <<"Variables">> => [<<"Knifestep">>]
    },
    Entry = wfcli_entity_worldstate:build(
              archimedea, "deep", "Deep Archimedea", Data,
              #{resolve_items => true, archimedea_loadout => loadout_context()}),
    Row = maps:get(row_map, Entry),
    ?assertEqual("Deep", maps:get(archimedea, Row)),
    ?assertEqual(157125, maps:get(seed, Row)),
    ?assert(string:find(maps:get(deviations, Row), "Sealed Armor") =/= nomatch),
    ?assert(string:find(maps:get(elite_risks, Row), "Commanding Culverins") =/= nomatch),
    ?assert(string:find(maps:get(modifier_details, Row), "Lose 2 Health") =/= nomatch),
    ?assertEqual("ready", maps:get(loadout_status, Row)),
    ?assertEqual("", maps:get(loadout_warning, Row)),
    ?assert(string:find(maps:get(loadouts, Row), "Owned Frame [owned]") =/= nomatch),
    Parsed = wfcli_worldstate_query:parse(
               "archimedea=deep deviation~sealed elite-risk~culverin seed=157125 "
               "warframe~owned loadout-status=ready"),
    ?assertEqual([], maps:get(errors, Parsed)),
    ?assert(wfcli_worldstate_query:match(Entry, Parsed)).

archimedea_unverified_loadouts_include_warning_test() ->
    Data = #{<<"Type">> => <<"CT_LAB">>, <<"RandomSeed">> => 157125},
    Context = (loadout_context())#{warning => unverified_game_metadata},
    Entry = wfcli_entity_worldstate:build(
              archimedea, "deep", "Deep Archimedea", Data,
              #{resolve_items => true, archimedea_loadout => Context}),
    Row = maps:get(row_map, Entry),
    ?assertEqual("ready", maps:get(loadout_status, Row)),
    ?assert(string:find(maps:get(loadout_warning, Row), "unverified") =/= nomatch),
    ?assert(string:find(maps:get(loadouts, Row), "Owned Frame [owned]") =/= nomatch),
    #{fields := Fields} = wfcli_worldstate_presentation:block_spec(archimedea),
    ?assert(lists:member({"Warning", loadout_warning}, Fields)).

archimedea_owned_marker_uses_ownership_flag_test() ->
    Owned = <<"catalog-owned">>,
    OtherA = <<"forced-a">>,
    OtherB = <<"forced-b">>,
    Pool = #{owned => [Owned], catalog => [<<"a">>, <<"b">>, <<"c">>]},
    Context = #{status => ready, account_seed => 7,
                names => #{Owned => "Catalog Owned", OtherA => "Forced A",
                           OtherB => "Forced B", <<"a">> => "A",
                           <<"b">> => "B", <<"c">> => "C"},
                pools => #{suits => Pool, primaries => Pool,
                           secondaries => Pool, melees => Pool}},
    Data = #{<<"Type">> => <<"CT_LAB">>, <<"RandomSeed">> => 157125,
             <<"ForcedLoadouts">> =>
                 #{<<"suits">> => [Owned, OtherA, OtherB]}},
    Projection = wfcli_archimedea:project(
                   Data, #{resolve_items => true, archimedea_loadout => Context}),
    ?assertEqual("Catalog Owned [owned], Forced A, Forced B",
                 maps:get(suits, Projection)).

loadout_context() ->
    Pool = #{owned => [<<"owned">>], catalog => [<<"a">>, <<"b">>, <<"c">>]},
    #{status => ready, account_seed => 7,
      names => #{<<"owned">> => "Owned Frame", <<"a">> => "A",
                 <<"b">> => "B", <<"c">> => "C"},
      pools => #{suits => Pool, primaries => Pool,
                 secondaries => Pool, melees => Pool}}.

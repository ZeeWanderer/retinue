%%%-------------------------------------------------------------------
%% EUnit coverage for the Overframe source adapter.
%%%-------------------------------------------------------------------
-module(wfcli_overframe_source_eunit).

-include_lib("eunit/include/eunit.hrl").

database_chunk_decode_test() ->
    Database = #{<<"/Lotus/Test">> =>
                     #{<<"id">> => 42,
                       <<"name">> => <<"Tenno's \"test\" \\ item"/utf8>>,
                       <<"unicode">> =>
                           unicode:characters_to_binary([84, 97, 117, 32, 16#3C4])}},
    {ok, Decoded} = wfcli_overframe_source:decode_database(chunk(Database)),
    ?assertEqual(Database, Decoded).

catalog_refresh_and_search_test_() ->
    {setup,
     fun() ->
         application:set_env(retinue_daemon, overframe_http_fun, catalog_http_fun())
     end,
     fun(_) -> application:unset_env(retinue_daemon, overframe_http_fun) end,
     fun(_) -> fun() ->
         {ok, Catalog} = wfcli_overframe_source:refresh_catalog(),
         ?assertEqual(wfcli_overframe_source:catalog_schema(),
                      maps:get(schema, Catalog)),
         ?assertEqual(5, map_size(maps:get(items_by_path, Catalog))),
         ?assertEqual(1, map_size(maps:get(mods_by_id, Catalog))),
         Mod = maps:get(200, maps:get(mods_by_id, Catalog)),
         ?assertEqual(2, maps:get(<<"base_drain">>, Mod)),
         ?assertEqual(<<"galvanized">>, maps:get(<<"mod_variant">>, Mod)),
         ?assertEqual([<<"cold">>, <<"toxin">>], maps:get(<<"elemental_types">>, Mod)),
         ?assertEqual([<<"cold">>, <<"toxin">>],
                      wfcli_overframe_source:mod_elements(Catalog, <<"/Lotus/Mods/Test">>)),
         ?assertEqual(null, wfcli_overframe_source:mod_elements(Catalog, <<"/unknown">>)),
         {ok, Search} = wfcli_overframe_source:search(
                          Catalog, <<"test">>, <<"primary">>, 10),
         [Item] = maps:get(<<"items">>, Search),
         ?assertEqual(<<"Test Rifle">>, maps:get(<<"name">>, Item)),
         ?assertEqual(100, maps:get(<<"external_id">>, Item)),
         {ok, All} = wfcli_overframe_source:search(
                       Catalog, <<>>, <<"all">>, 10),
         ?assertEqual([<<"Agkuza">>, <<"Test Rifle">>],
                      [maps:get(<<"name">>, Match)
                       || Match <- maps:get(<<"items">>, All)]),
         [Archmelee] = [Match || Match <- maps:get(<<"items">>, All),
                                 maps:get(<<"name">>, Match) =:= <<"Agkuza">>],
         ?assertEqual(<<"archmelee">>, maps:get(<<"class">>, Archmelee))
     end end}.

revision_fingerprint_ignores_metadata_test() ->
    Catalog = test_catalog(),
    Raw = raw_build(),
    First = wfcli_overframe_source:normalize_detail(Raw, Catalog),
    Second = wfcli_overframe_source:normalize_detail(
               Raw#{<<"score">> => 999, <<"updated">> => <<"later">>}, Catalog),
    ChangedSlots = [#{<<"slot_id">> => 1, <<"mod">> => 200,
                      <<"rank">> => 9, <<"polarity">> => 1, <<"drain">> => 7}],
    Changed = wfcli_overframe_source:normalize_detail(
                Raw#{<<"slots">> => ChangedSlots}, Catalog),
    ?assertEqual(maps:get(<<"fingerprint">>, First),
                 maps:get(<<"fingerprint">>, Second)),
    ?assertNotEqual(maps:get(<<"fingerprint">>, First),
                    maps:get(<<"fingerprint">>, Changed)),
    [Slot] = maps:get(<<"slots">>, maps:get(<<"content">>, First)),
    ?assertEqual(<<"/Lotus/Mods/Test">>, maps:get(<<"canonical_id">>, Slot)),
    ?assertEqual(<<"madurai">>, maps:get(<<"mod_polarity">>, Slot)),
    ?assertEqual(<<"madurai">>, maps:get(<<"slot_polarity">>, Slot)),
    ?assertEqual(<<"matched">>, maps:get(<<"polarity_state">>, Slot)),
    ?assertEqual(<<"galvanized">>, maps:get(<<"mod_variant">>, Slot)),
    ?assertEqual(<<"rare">>, maps:get(<<"rarity">>, Slot)),
    ?assertEqual(12, maps:get(<<"cost">>, Slot)),
    Presentation = maps:get(<<"presentation">>, First),
    ?assertEqual(<<"primary">>,
                 maps:get(<<"layout_class">>,
                          maps:get(<<"topology">>, Presentation))),
    [Upgrade] = maps:get(<<"upgrades">>, Presentation),
    ?assertEqual(<<"mod-8">>, maps:get(<<"topology_slot">>, Upgrade)),
    ?assertEqual(#{<<"id">> => <<"/Lotus/Mods/Test">>,
                   <<"source">> => <<"wfcd">>,
                   <<"image_name">> => <<"Test.png">>},
                 maps:get(<<"asset">>, Upgrade)),
    ?assertEqual(<<"Test notes">>,
                 maps:get(<<"description">>, maps:get(<<"metadata">>, First))),
    ChangedMod = (maps:get(200, maps:get(mods_by_id, Catalog)))#{
                   <<"texture">> => <<"/Lotus/Mods/Test-v2.png">>},
    ChangedCatalog = Catalog#{mods_by_id => #{200 => ChangedMod}},
    PresentedAgain = wfcli_overframe_source:normalize_detail(Raw, ChangedCatalog),
    ?assertEqual(maps:get(<<"fingerprint">>, First),
                 maps:get(<<"fingerprint">>, PresentedAgain)),
    [ChangedUpgrade] = maps:get(
                         <<"upgrades">>,
                         maps:get(<<"presentation">>, PresentedAgain)),
    ?assertEqual(<<"Test-v2.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, ChangedUpgrade))).

pinned_revision_refreshes_derived_metadata_test() ->
    Catalog = test_catalog(),
    Original = wfcli_overframe_source:normalize_detail(raw_build(), Catalog),
    Content = maps:get(<<"content">>, Original),
    [Slot] = maps:get(<<"slots">>, Content),
    Stale = Original#{<<"content">> => Content#{<<"slots">> =>
        [Slot#{<<"base_drain">> => null, <<"cost">> => null,
               <<"mod_variant">> => <<"standard">>}]}},
    Updated = wfcli_overframe_source:present_revision(Stale, Catalog),
    ?assertEqual(maps:get(<<"content">>, Stale), maps:get(<<"content">>, Updated)),
    ?assertEqual(maps:get(<<"fingerprint">>, Stale), maps:get(<<"fingerprint">>, Updated)),
    [Presented] = maps:get(<<"upgrades">>, maps:get(<<"presentation">>, Updated)),
    ?assertEqual(12, maps:get(<<"cost">>, Presented)),
    ?assertEqual(<<"galvanized">>, maps:get(<<"mod_variant">>, Presented)).

catalog_http_fun() ->
    Items = #{<<"/Lotus/Weapons/TestRifle">> =>
                  #{<<"id">> => 100, <<"name">> => <<"Test Rifle">>,
                    <<"categories">> => [<<"weapon">>, <<"primary">>],
                    <<"data">> => #{<<"ArtifactSlots">> => [<<"AP_UNIVERSAL">>]},
                    <<"tag">> => <<"Weapon">>},
              <<"/Lotus/Types/Game/PowerSuit">> =>
                  #{<<"id">> => 101, <<"name">> => <<>>,
                    <<"categories">> => [],
                    <<"data">> => #{<<"ArtifactSlots">> => [<<"AP_UNIVERSAL">>]},
                    <<"tag">> => <<"PowerSuit">>},
              <<"/Lotus/Types/Game/InternalWeapon">> =>
                  #{<<"id">> => 102,
                    <<"name">> => <<"/Lotus/Types/Game/InternalWeapon">>,
                    <<"categories">> => [],
                    <<"data">> => #{<<"ArtifactSlots">> => [<<"AP_UNIVERSAL">>]},
                    <<"tag">> => <<"Weapon">>},
              <<"/Lotus/Types/Game/UnknownWeapon">> =>
                  #{<<"id">> => 103, <<"name">> => <<"????">>,
                    <<"categories">> => [],
                    <<"data">> => #{<<"ArtifactSlots">> => [<<"AP_UNIVERSAL">>]},
                    <<"tag">> => <<"Weapon">>},
              <<"/Lotus/Weapons/Archwing/Agkuza">> =>
                  #{<<"id">> => 104, <<"name">> => <<"<ARCHWING> Agkuza">>,
                    <<"categories">> => [<<"weapon">>, <<"melee">>],
                    <<"data">> => #{<<"ArtifactSlots">> => [<<"AP_UNIVERSAL">>]},
                    <<"tag">> => <<"Weapon">>}},
    Mods = #{<<"/Lotus/Mods/Test">> =>
                 #{<<"id">> => 200, <<"name">> => <<"Test Mod">>,
                   <<"categories">> => [<<"mod">>],
                   <<"data">> => #{<<"ArtifactPolarity">> => 1,
                                     <<"IsGalvanized">> => 1,
                                     <<"SubUpgrades">> => [#{<<"ConditionalUpgrades">> => [<<"on_kill">>],
                                        <<"Upgrades">> => [#{<<"DamageType">> => <<"DT_ANY">>}]}],
                                     <<"Upgrades">> => [#{<<"DamageType">> => <<"DT_FREEZE">>},
                                                         #{<<"DamageType">> => <<"DT_EXPLOSION">>},
                                                         #{<<"DamageType">> => <<"DT_POISON">>}],
                                     <<"FusionLimit">> => <<"QA_VERY_HIGH">>}}},
    Rivens = #{},
    fun(Url, _Headers) ->
        case Url of
            "https://overframe.gg/_next/static/chunks/webpack.js" ->
                {ok, 404,
                 <<"<script src=\"https://static.overframe.gg/_next/static/"
                   "chunks/webpack-a1b2.js\"></script>">>};
            "https://static.overframe.gg/_next/static/chunks/webpack-a1b2.js" ->
                {ok, 200,
                 <<"({461:\"db/items\",7482:\"db/mods\",4287:\"db/rivens\"},"
                   "{461:\"a11\",7482:\"b22\",4287:\"c33\"})">>};
            "https://static.overframe.gg/_next/static/chunks/db/items.a11.js" ->
                {ok, 200, chunk(Items)};
            "https://static.overframe.gg/_next/static/chunks/db/mods.b22.js" ->
                {ok, 200, chunk(Mods)};
            "https://static.overframe.gg/_next/static/chunks/db/rivens.c33.js" ->
                {ok, 200, chunk(Rivens)}
        end
    end.

chunk(Database) ->
    Json = jsone:encode(Database),
    EscapedSlashes = binary:replace(Json, <<"\\">>, <<"\\\\">>, [global]),
    Escaped = binary:replace(EscapedSlashes, <<"'">>, <<"\\'">>, [global]),
    <<"(self.webpackChunk_N_E=[]).push([{x:function(e){e.exports=JSON.parse('",
      Escaped/binary, "')}}]);">>.

test_catalog() ->
    Item = #{<<"source">> => <<"overframe">>, <<"external_id">> => 100,
             <<"canonical_id">> => <<"/Lotus/Weapons/TestRifle">>,
             <<"name">> => <<"Test Rifle">>, <<"class">> => <<"primary">>,
             <<"categories">> => [<<"primary">>],
             <<"texture">> => <<"/Lotus/Weapons/TestRifle.png">>},
    Mod = #{<<"external_id">> => 200,
            <<"canonical_id">> => <<"/Lotus/Mods/Test">>,
            <<"name">> => <<"Test Mod">>, <<"categories">> => [<<"mod">>],
            <<"polarity">> => <<"madurai">>, <<"base_drain">> => 4,
            <<"rarity">> => <<"rare">>, <<"mod_variant">> => <<"galvanized">>,
            <<"max_rank">> => 10,
            <<"texture">> => <<"/Lotus/Mods/Test.png">>},
    #{schema => wfcli_overframe_source:catalog_schema(),
      items_by_id => #{100 => Item}, items_by_path => #{},
      mods_by_id => #{200 => Mod}, rivens_by_id => #{}}.

raw_build() ->
    #{<<"id">> => 300, <<"item">> => 100, <<"title">> => <<"Test Build">>,
      <<"score">> => 10, <<"formas">> => 2, <<"item_rank">> => 30,
      <<"buildstring">> => <<"encoded">>, <<"updated">> => <<"now">>,
      <<"description">> => <<"Test notes">>,
      <<"slots">> => [#{<<"slot_id">> => 1, <<"mod">> => 200,
                           <<"rank">> => 8, <<"polarity">> => 1,
                           <<"drain">> => 7}]}.

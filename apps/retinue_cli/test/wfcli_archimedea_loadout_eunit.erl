-module(wfcli_archimedea_loadout_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

game_update_warns_without_disabling_selection_test() ->
    Path = filename:join("/tmp", "wfcli-archimedea-executable-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    try
        ok = file:write_file(Path, <<"old build">>),
        {ok, #file_info{size = Size, mtime = Modified}} =
            file:read_file_info(Path, [{time, posix}]),
        Archimedea = empty_metadata(#{}, #{}),
        Snapshot = #{revision => 1,
                     data => #{<<"schema">> => 2,
                               <<"executable">> =>
                                   #{<<"path">> => list_to_binary(Path),
                                     <<"size">> => Size,
                                     <<"modified_unix_ms">> => Modified * 1000,
                                     <<"sha256">> => binary:copy(<<"a">>, 64)},
                               <<"archimedea">> => Archimedea}},
        {ok, Archimedea, OldKey, none} = wfcli_archimedea_loadout:metadata(Snapshot),
        ok = file:write_file(Path, <<"new game build">>),
        {ok, Archimedea, NewKey, Warning} = wfcli_archimedea_loadout:metadata(Snapshot),
        ?assertEqual(unverified_game_metadata, Warning),
        ?assertNotEqual(OldKey, NewKey),
        Data = #{<<"RandomSeed">> => 100},
        Expected = wfcli_archimedea_loadout:resolve(Data, context(23)),
        Actual = wfcli_archimedea_loadout:resolve(Data, (context(23))#{warning => Warning}),
        ?assertEqual(Expected#{warning => Warning}, Actual)
    after
        file:delete(Path)
    end.

invalid_catalog_is_not_used_as_a_fallback_test() ->
    Snapshot = #{revision => 1,
                 data => #{<<"schema">> => 2,
                           <<"executable">> => #{<<"sha256">> => binary:copy(<<"a">>, 64)},
                           <<"archimedea">> => #{<<"catalog">> => #{}},
                           <<"capture_error">> => #{<<"reason">> => <<"unsupported_executable">>}}},
    ?assertEqual({error, invalid_game_metadata}, wfcli_archimedea_loadout:metadata(Snapshot)).

owned_then_catalog_selection_test() ->
    Result = wfcli_archimedea_loadout:select(100, #{}, context(23)),
    ?assertEqual(123, maps:get(effective_seed, Result)),
    Expected = [<<"o1">>, <<"c">>, <<"a">>],
    lists:foreach(
      fun(Category) ->
          ?assertEqual(Expected, ids(maps:get(Category, Result)))
      end,
      [suits, primaries, secondaries, melees]),
    [Owned | _] = maps:get(suits, Result),
    ?assertEqual(owned, maps:get(source, Owned)),
    ?assertEqual(true, maps:get(owned, Owned)).

forced_entries_reduce_catalog_before_owned_test() ->
    Forced = #{suits => [<<"f1">>, <<"f2">>]},
    Result = wfcli_archimedea_loadout:select(100, Forced, context(23)),
    ?assertEqual([<<"o1">>, <<"f1">>, <<"f2">>], ids(maps:get(suits, Result))),
    ?assertEqual([owned, forced, forced],
                 [maps:get(source, Item) || Item <- maps:get(suits, Result)]).

three_forced_entries_replace_random_selection_test() ->
    Forced = #{suits => [<<"f1">>, <<"f2">>, <<"f3">>]},
    Result = wfcli_archimedea_loadout:select(100, Forced, context(23)),
    ?assertEqual([<<"f1">>, <<"f2">>, <<"f3">>], ids(maps:get(suits, Result))).

catalog_duplicate_is_removed_and_rerolls_test() ->
    Pool = #{owned => [<<"o">>], catalog => [<<"o">>, <<"a">>, <<"b">>, <<"c">>]},
    Context = (context(3))#{
                names => #{<<"o">> => "O", <<"a">> => "A",
                           <<"b">> => "B", <<"c">> => "C"},
                pools => #{suits => Pool, primaries => Pool,
                           secondaries => Pool, melees => Pool}},
    Result = wfcli_archimedea_loadout:select(0, #{}, Context),
    ?assertEqual([<<"o">>, <<"a">>, <<"c">>], ids(maps:get(suits, Result))).

missing_seed_is_explicit_test() ->
    ?assertEqual(#{status => unavailable, reason => missing_worldstate_seed},
                 wfcli_archimedea_loadout:resolve(#{}, context(23))).

seed_matches_luau_signed_32_bit_narrowing_test() ->
    Result = wfcli_archimedea_loadout:select(16#7fffffff, #{}, context(1)),
    ?assertEqual(-16#80000000, maps:get(effective_seed, Result)),
    ?assertEqual([<<"o1">>, <<"a">>, <<"b">>], ids(maps:get(suits, Result))).

metadata_pools_preserve_order_and_normalize_owned_items_test() ->
    Suit = <<"/Lotus/Powersuits/Test/TestSuit">>,
    SuitVariant = <<"/Lotus/Powersuits/Test/TestSuitPrime">>,
    Rifle = <<"/Lotus/Weapons/Test/TestRifle">>,
    RifleVariant = <<"/Lotus/Weapons/Test/TestRifleVariant">>,
    Quad = <<"/Lotus/Weapons/Tenno/Shotgun/QuadShotgun">>,
    QuadBase = <<"/Lotus/Weapons/Tenno/Shotgun/QuadShotgunBase">>,
    Raw = #{<<"Suits">> => [item(SuitVariant)],
            <<"LongGuns">> => [item(RifleVariant), item(QuadBase), item(RifleVariant)],
            <<"Pistols">> => [item(<<"not-allowed">>)],
            <<"Melee">> => []},
    Metadata = #{<<"catalog">> =>
                     #{<<"suits">> => [Suit, <<"second-suit">>],
                       <<"primaries">> => [Quad, Rifle],
                       <<"secondaries">> => [<<"secondary">>],
                       <<"melees">> => [<<"melee">>]},
                 <<"owned_suit_items">> => [SuitVariant],
                 <<"owned_weapon_items">> => [RifleVariant, QuadBase],
                 <<"suit_aliases">> => [alias(SuitVariant, Suit)],
                 <<"weapon_aliases">> => [alias(RifleVariant, Rifle)]},
    Pools = wfcli_archimedea_loadout:pools(Raw, Metadata),
    ?assertEqual([Suit], maps:get(owned, maps:get(suits, Pools))),
    ?assertEqual([Rifle, Quad], maps:get(owned, maps:get(primaries, Pools))),
    ?assertEqual([], maps:get(owned, maps:get(secondaries, Pools))),
    ?assertEqual([Quad, Rifle], maps:get(catalog, maps:get(primaries, Pools))).

alias_target_does_not_allow_a_rejected_source_item_test() ->
    Base = <<"/Lotus/Weapons/Test/TestRifle">>,
    Prime = <<"/Lotus/Weapons/Test/TestRiflePrime">>,
    Raw = #{<<"Suits">> => [], <<"LongGuns">> => [item(Prime)],
            <<"Pistols">> => [], <<"Melee">> => []},
    Metadata = empty_metadata(
                 #{<<"primaries">> => [Base]},
                 #{<<"owned_weapon_items">> => [Base],
                   <<"weapon_aliases">> => [alias(Prime, Base)]}),
    Pools = wfcli_archimedea_loadout:pools(Raw, Metadata),
    ?assertEqual([], maps:get(owned, maps:get(primaries, Pools))).

alternate_weapon_mode_is_not_an_owned_candidate_test() ->
    Weapon = <<"/Lotus/Weapons/Test/TestRifle">>,
    Raw = #{<<"Suits">> => [],
            <<"LongGuns">> => [#{<<"ItemType">> => Weapon,
                                  <<"mAltWeaponModeId">> => <<"alternate">>}],
            <<"Pistols">> => [], <<"Melee">> => []},
    Metadata = empty_metadata(
                 #{<<"primaries">> => [Weapon]},
                 #{<<"owned_weapon_items">> => [Weapon]}),
    Pools = wfcli_archimedea_loadout:pools(Raw, Metadata),
    ?assertEqual([], maps:get(owned, maps:get(primaries, Pools))).

temporal_seed_selects_observed_owned_indices_test() ->
    Pools = #{suits => indexed_pool(43, 5, <<"Cyte09">>),
              primaries => indexed_pool(73, 9, <<"Gorgon">>),
              secondaries => indexed_pool(56, 7, <<"Grimoire">>),
              melees => indexed_pool(93, 12, <<"DualEther">>)},
    Result = wfcli_archimedea_loadout:select(
               925698, #{}, #{account_seed => 10381186, pools => Pools,
                              names => #{}, catalog_source => fixture}),
    ?assertEqual(11306884, maps:get(effective_seed, Result)),
    ?assertEqual(<<"Cyte09">>, first_id(suits, Result)),
    ?assertEqual(<<"Gorgon">>, first_id(primaries, Result)),
    ?assertEqual(<<"Grimoire">>, first_id(secondaries, Result)),
    ?assertEqual(<<"DualEther">>, first_id(melees, Result)).

deep_seed_reconstructs_observed_loadout_test() ->
    Pools = #{
        suits => observed_pool(43, 117, 26, 29, 70,
                               <<"Nekros">>, <<"Titania">>, <<"Protea">>),
        primaries => observed_pool(75, 83, 45, 20, 50,
                                   <<"Argonak">>, <<"Karak">>, <<"Panthera">>),
        secondaries => observed_pool(61, 71, 37, 17, 42,
                                     <<"Dual Cestra">>, <<"Quatz">>, <<"Bolto">>),
        melees => observed_pool(95, 112, 58, 27, 67,
                                <<"Pennant">>, <<"Cerata">>, <<"Skana">>)},
    Result = wfcli_archimedea_loadout:select(
               173284, #{}, #{account_seed => 10381186, pools => Pools,
                              names => #{}, catalog_source => fixture}),
    ?assertEqual(10554470, maps:get(effective_seed, Result)),
    ?assertEqual([<<"Nekros">>, <<"Titania">>, <<"Protea">>], ids(maps:get(suits, Result))),
    ?assertEqual([<<"Argonak">>, <<"Karak">>, <<"Panthera">>],
                 ids(maps:get(primaries, Result))),
    ?assertEqual([<<"Dual Cestra">>, <<"Quatz">>, <<"Bolto">>],
                 ids(maps:get(secondaries, Result))),
    ?assertEqual([<<"Pennant">>, <<"Cerata">>, <<"Skana">>],
                 ids(maps:get(melees, Result))).

context(AccountSeed) ->
    Paths = [<<"o1">>, <<"o2">>, <<"a">>, <<"b">>, <<"c">>, <<"d">>],
    Names = maps:from_list([{Path, binary_to_list(Path)} || Path <- Paths]),
    Pool = #{owned => [<<"o1">>, <<"o2">>],
             catalog => [<<"a">>, <<"b">>, <<"c">>, <<"d">>]},
    #{status => ready, account_seed => AccountSeed, names => Names,
      pools => #{suits => Pool, primaries => Pool,
                 secondaries => Pool, melees => Pool},
      catalog_source => fixture}.

ids(Items) -> [list_to_binary(maps:get(id, Item)) || Item <- Items].

item(Path) -> #{<<"ItemType">> => Path}.

alias(Source, Canonical) -> #{<<"source">> => Source, <<"canonical">> => Canonical}.

empty_metadata(CatalogOverrides, Overrides) ->
    Catalog = maps:merge(
                #{<<"suits">> => [], <<"primaries">> => [],
                  <<"secondaries">> => [], <<"melees">> => []},
                CatalogOverrides),
    maps:merge(
      #{<<"catalog">> => Catalog,
        <<"owned_suit_items">> => [], <<"owned_weapon_items">> => [],
        <<"suit_aliases">> => [], <<"weapon_aliases">> => []},
      Overrides).

indexed_pool(Count, SelectedIndex, Selected) ->
    Owned = [case Index of
                 SelectedIndex -> Selected;
                 _ -> <<"owned-", (integer_to_binary(Index))/binary>>
             end || Index <- lists:seq(0, Count - 1)],
    #{owned => Owned, catalog => [<<"catalog-a">>, <<"catalog-b">>]}.

observed_pool(OwnedCount, CatalogCount, OwnedIndex, FirstCatalogIndex,
              SecondCatalogIndex, OwnedItem, FirstCatalogItem, SecondCatalogItem) ->
    Owned = [case Index of
                 OwnedIndex -> OwnedItem;
                 _ -> <<"owned-", (integer_to_binary(Index))/binary>>
             end || Index <- lists:seq(0, OwnedCount - 1)],
    Catalog = [case Index of
                   FirstCatalogIndex -> FirstCatalogItem;
                   SecondCatalogIndex -> SecondCatalogItem;
                   _ -> <<"catalog-", (integer_to_binary(Index))/binary>>
               end || Index <- lists:seq(0, CatalogCount - 1)],
    #{owned => Owned, catalog => Catalog}.

first_id(Category, Result) ->
    [First | _] = ids(maps:get(Category, Result)),
    First.

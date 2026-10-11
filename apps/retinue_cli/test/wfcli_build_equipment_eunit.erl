-module(wfcli_build_equipment_eunit).

-include_lib("eunit/include/eunit.hrl").

selected_instances_use_the_same_normalized_state_test() ->
    Full = wfcli_build_equipment:from_snapshot(snapshot(), catalog()),
    Selected = wfcli_build_equipment:from_snapshot(
                 snapshot(), catalog(), [<<"suit-1">>, <<"suit-1">>, <<"missing">>]),
    [Suit] = [Item || #{<<"id">> := <<"suit-1">>} = Item <-
                         maps:get(<<"instances">>, Full)],
    ?assertEqual([Suit], maps:get(<<"instances">>, Selected)),
    ?assertEqual(1, length(maps:get(<<"definitions">>, Selected))),
    ?assertEqual(maps:get(<<"player_revision">>, Full),
                 maps:get(<<"player_revision">>, Selected)),
    Empty = wfcli_build_equipment:from_snapshot(snapshot(), catalog(), []),
    ?assertEqual([], maps:get(<<"instances">>, Empty)),
    ?assertEqual([], maps:get(<<"definitions">>, Empty)).

preserves_incarnon_choices_test() ->
    Snapshot = snapshot(),
    Data = maps:get(data, Snapshot),
    Inventory = maps:get(<<"inventory">>, Data),
    Raw = maps:get(<<"raw">>, Inventory),
    [Exalted | Rest] = maps:get(<<"SpecialItems">>, Raw),
    Updated = Snapshot#{data => Data#{<<"inventory">> => Inventory#{<<"raw">> =>
        Raw#{<<"SpecialItems">> => [Exalted#{<<"SkillTree">> => <<"0102">>} | Rest]}}}},
    View = wfcli_build_equipment:from_snapshot(Updated, catalog()),
    [Item] = [I || I <- maps:get(<<"instances">>, View), maps:get(<<"id">>, I) =:= <<"exalted-1">>],
    ?assertEqual(<<"0102">>, maps:get(<<"skill_tree">>, Item)).

normalizes_definitions_instances_and_exalted_items_test() ->
    View = wfcli_build_equipment:from_snapshot(snapshot(), catalog()),
    ?assertEqual(7, maps:get(<<"player_revision">>, View)),
    Definitions = maps:get(<<"definitions">>, View),
    ?assertEqual(3, length(Definitions)),
    [ExaltedDefinition] =
        [Definition || Definition <- Definitions,
                       maps:get(<<"id">>, Definition) =:=
                           <<"/Lotus/Powersuits/Excalibur/DoomSword">>],
    ?assertEqual(<<"Exalted Blade">>, maps:get(<<"name">>, ExaltedDefinition)),
    ?assertEqual(<<"exalted">>, maps:get(<<"class">>, ExaltedDefinition)),
    ?assertEqual(<<"melee">>, maps:get(<<"layout_class">>, ExaltedDefinition)),

    [Exalted] = [Instance || Instance <- maps:get(<<"instances">>, View),
                              maps:get(<<"id">>, Instance) =:= <<"exalted-1">>],
    ?assertEqual(3, maps:get(<<"forma_count">>, Exalted)),
    ?assertEqual(true, maps:get(<<"incarnon_genesis">>,
                                maps:get(<<"features">>, Exalted))),
    ?assertEqual(2, maps:get(<<"mod_slot_purchases">>, Exalted)),
    [Config] = maps:get(<<"configs">>, Exalted),
    [Slot] = maps:get(<<"upgrade_slots">>, Config),
    ?assertEqual(<<"/Lotus/Upgrades/Mods/ExaltedTestMod">>,
                 maps:get(<<"item_type">>, Slot)).

projects_topology_config_state_and_instance_state_test() ->
    View = wfcli_build_equipment:from_snapshot(snapshot(), catalog()),
    ?assertEqual(3, maps:get(<<"schema">>, View)),
    [Suit] = [Instance || Instance <- maps:get(<<"instances">>, View),
                          maps:get(<<"id">>, Instance) =:= <<"suit-1">>],
    Topology = maps:get(<<"topology">>, Suit),
    ?assertEqual(<<"warframe">>, maps:get(<<"layout_class">>, Topology)),
    ?assert(lists:any(fun(#{<<"id">> := <<"shards">>}) -> true;
                        (_) -> false
                     end, maps:get(<<"regions">>, Topology))),
    [Override] = [Polarity
                  || Polarity <- maps:get(<<"effective_polarities">>, Suit),
                     maps:get(<<"slot_id">>, Polarity) =:= <<"mod-8">>],
    ?assertEqual(<<"madurai">>, maps:get(<<"polarity">>, Override)),
    ?assertEqual(<<"override">>, maps:get(<<"source">>, Override)),
    ?assertEqual(1, length(maps:get(<<"shard_slots">>, Suit))),
    [Config] = maps:get(<<"configs">>, Suit),
    ?assertEqual([<<"/Lotus/Powersuits/Powers/Roar">>],
                 maps:get(<<"ability_override">>, Config)),
    [Arcane] = [Upgrade
                || Upgrade <- maps:get(<<"upgrade_slots">>, Config),
                   maps:get(<<"slot">>, Upgrade) =:= 10],
    [Mod] = [Upgrade
             || Upgrade <- maps:get(<<"upgrade_slots">>, Config),
                maps:get(<<"slot">>, Upgrade) =:= 0],
    ?assertEqual(<<"Warframe">>, maps:get(<<"compat_name">>, Mod)),
    ?assertEqual([<<"Rank 8 effect">>], maps:get(<<"effects">>, Mod)),
    ?assertEqual(<<"arcane-1">>, maps:get(<<"topology_slot">>, Arcane)),
    ?assertEqual(<<"arcane">>, maps:get(<<"role">>, Arcane)),
    ?assertEqual(<<"Test Arcane">>, maps:get(<<"name">>, Arcane)).

projects_secondary_exalted_slots_in_game_order_test() ->
    View = wfcli_build_equipment:from_snapshot(regulators_snapshot(),
                                                regulators_catalog()),
    [Definition] = maps:get(<<"definitions">>, View),
    ?assertEqual(<<"secondary">>, maps:get(<<"layout_class">>, Definition)),
    [Instance] = maps:get(<<"instances">>, View),
    Topology = maps:get(<<"topology">>, Instance),
    ?assertEqual(<<"secondary">>, maps:get(<<"layout_class">>, Topology)),
    [Config] = maps:get(<<"configs">>, Instance),
    Upgrades = maps:get(<<"upgrade_slots">>, Config),
    ByTopology = maps:from_list(
                   [{maps:get(<<"topology_slot">>, Upgrade), Upgrade}
                    || Upgrade <- Upgrades]),
    ?assertEqual(
       [<<"Primed Pistol Gambit">>, <<"Primed Target Cracker">>,
        <<"Galvanized Diffusion">>, <<"Anemic Agility">>,
        <<"Pathogen Rounds">>, <<"Primed Convulsion">>,
        <<"Primed Heated Charge">>, <<"Lethal Torrent">>],
       [maps:get(<<"name">>, maps:get(<<"mod-", (integer_to_binary(N))/binary>>,
                                       ByTopology))
        || N <- lists:seq(1, 8)]),
    ?assertEqual(<<"galvanized">>,
                 maps:get(<<"mod_variant">>, maps:get(<<"mod-3">>, ByTopology))),
    ?assertEqual(<<"standard">>,
                 maps:get(<<"mod_variant">>, maps:get(<<"mod-1">>, ByTopology))),
    Mod1 = topology_slot(<<"mod-1">>, Topology),
    Mod8 = topology_slot(<<"mod-8">>, Topology),
    ?assertEqual(7, maps:get(<<"player_index">>, Mod1)),
    ?assertEqual(8, maps:get(<<"build_slot">>, Mod1)),
    ?assertEqual(0, maps:get(<<"player_index">>, Mod8)),
    ?assertEqual(1, maps:get(<<"build_slot">>, Mod8)),
    Arcane = maps:get(<<"arcane-1">>, ByTopology),
    ?assertEqual(<<"Secondary Outburst">>, maps:get(<<"name">>, Arcane)),
    ?assertEqual(<<"arcane">>, maps:get(<<"role">>, Arcane)),
    ?assertEqual(5, maps:get(<<"rank">>, Arcane)),
    ?assertEqual(<<"Legendary">>, maps:get(<<"rarity">>, Arcane)),
    ?assertEqual([6, 7, 7, 9, 6, 8, 8, 6],
                 [maps:get(<<"effective_drain">>,
                           maps:get(<<"mod-", (integer_to_binary(N))/binary>>,
                                    ByTopology))
                  || N <- lists:seq(1, 8)]),
    OmniMod = maps:get(<<"mod-6">>, ByTopology),
    ?assertEqual(<<"naramon">>, maps:get(<<"polarity">>, OmniMod)),
    ?assertEqual(<<"omni">>, maps:get(<<"slot_polarity">>, OmniMod)),
    ?assertEqual(<<"matched">>, maps:get(<<"polarity_state">>, OmniMod)),
    NeutralMod = maps:get(<<"mod-4">>, ByTopology),
    ?assertEqual(<<"neutral">>, maps:get(<<"polarity_state">>, NeutralMod)),
    [Mod1Polarity] = [Entry
                      || Entry <- maps:get(<<"effective_polarities">>, Instance),
                         maps:get(<<"slot_id">>, Entry) =:= <<"mod-1">>],
    [Mod8Polarity] = [Entry
                      || Entry <- maps:get(<<"effective_polarities">>, Instance),
                         maps:get(<<"slot_id">>, Entry) =:= <<"mod-8">>],
    ?assertEqual(<<"madurai">>, maps:get(<<"polarity">>, Mod1Polarity)),
    ?assertEqual(<<"madurai">>, maps:get(<<"polarity">>, Mod8Polarity)).

supports_class_specific_topology_sizes_test() ->
    assert_mod_region(<<"archgun">>, 8),
    assert_mod_region(<<"necramech">>, 12),
    assert_mod_region(<<"companion">>, 10),
    Archgun = maps:get(<<"topology">>,
                       wfcli_build_topology:definition(<<"archgun">>, #{})),
    ?assertEqual([<<"mods">>, <<"arcanes">>],
                 [maps:get(<<"id">>, Region)
                  || Region <- maps:get(<<"regions">>, Archgun)]),
    [Arcanes] = [Region
                 || Region <- maps:get(<<"regions">>, Archgun),
                    maps:get(<<"id">>, Region) =:= <<"arcanes">>],
    ?assertEqual(2, length(maps:get(<<"slots">>, Arcanes))),
    ?assertEqual(4, maps:get(<<"columns">>, Arcanes)),
    Necramech = maps:get(
                  <<"topology">>,
                  wfcli_build_topology:definition(<<"necramech">>, #{})),
    ?assertEqual([<<"mods">>],
                 [maps:get(<<"id">>, Region)
                  || Region <- maps:get(<<"regions">>, Necramech)]).

keeps_definition_when_catalog_is_missing_test() ->
    View = wfcli_build_equipment:from_snapshot(snapshot(), []),
    [Definition] =
        [Item || Item <- maps:get(<<"definitions">>, View),
                 maps:get(<<"id">>, Item) =:=
                     <<"/Lotus/Powersuits/Excalibur/DoomSword">>],
    ?assertEqual(<<"DoomSword">>, maps:get(<<"name">>, Definition)),
    ?assertEqual(null, maps:get(<<"asset">>, Definition)).

builtin_metadata_can_name_an_internal_identity_and_alias_its_asset_test() ->
    Identity = <<"/Lotus/Types/Friendly/PlayerControllable/Weapons/DuviriDualSwords">>,
    Alias = <<"/Lotus/Types/Friendly/PlayerControllable/Weapons/DuviriDualSwordsWeapon">>,
    Catalog = [#{<<"uniqueName">> => Alias, <<"name">> => <<"Sun & Moon">>,
                 <<"category">> => <<"Melee">>,
                 <<"imageName">> => <<"TeshinDualSwords.png">>}],
    {View, Issues} = wfcli_build_equipment:from_snapshot_with_issues(
                       equipment_snapshot(Identity), Catalog),
    [Definition] = maps:get(<<"definitions">>, View),
    ?assertEqual(<<"Sun & Moon">>, maps:get(<<"name">>, Definition)),
    ?assertEqual(Alias, maps:get(<<"catalog_id">>, Definition)),
    ?assertEqual(#{<<"name">> => <<"builtin">>, <<"asset">> => <<"wfcd">>},
                 maps:get(<<"metadata_sources">>, Definition)),
    ?assertEqual(<<"TeshinDualSwords.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, Definition))),
    ?assertEqual([], Issues).

exact_catalog_metadata_beats_builtin_alias_test() ->
    Identity = <<"/Lotus/Types/Friendly/PlayerControllable/Weapons/DuviriDualSwords">>,
    Catalog = [#{<<"uniqueName">> => Identity, <<"name">> => <<"Future Canonical Name">>,
                 <<"category">> => <<"Melee">>, <<"imageName">> => <<"future.png">>}],
    {View, Issues} = wfcli_build_equipment:from_snapshot_with_issues(
                       equipment_snapshot(Identity), Catalog),
    [Definition] = maps:get(<<"definitions">>, View),
    ?assertEqual(<<"Future Canonical Name">>, maps:get(<<"name">>, Definition)),
    ?assertNot(maps:is_key(<<"catalog_id">>, Definition)),
    ?assertEqual(#{<<"name">> => <<"wfcd">>, <<"asset">> => <<"wfcd">>},
                 maps:get(<<"metadata_sources">>, Definition)),
    ?assertEqual([], Issues).

builtin_name_is_used_without_a_catalog_alias_test() ->
    Identity = <<"/Lotus/Powersuits/Operator/AdultOperatorSuitRemaster">>,
    {View, Issues} = wfcli_build_equipment:from_snapshot_with_issues(
                       equipment_snapshot(Identity), []),
    [Definition] = maps:get(<<"definitions">>, View),
    ?assertEqual(<<"Drifter">>, maps:get(<<"name">>, Definition)),
    ?assertEqual(<<"builtin">>,
                 maps:get(<<"name">>, maps:get(<<"metadata_sources">>, Definition))),
    ?assertEqual([<<"asset">>],
                 [maps:get(<<"kind">>, Issue) || Issue <- Issues]).

unresolved_equipment_reports_name_and_asset_context_test() ->
    Identity = <<"/Lotus/Test/UnknownEquipment">>,
    {View, Issues} = wfcli_build_equipment:from_snapshot_with_issues(
                       equipment_snapshot(Identity), []),
    [Definition] = maps:get(<<"definitions">>, View),
    ?assertEqual(<<"UnknownEquipment">>, maps:get(<<"name">>, Definition)),
    ?assertEqual(null, maps:get(<<"asset">>, Definition)),
    ?assertEqual([<<"asset">>, <<"friendly_name">>],
                 lists:sort([maps:get(<<"kind">>, Issue) || Issue <- Issues])),
    ?assert(lists:all(
              fun(Issue) -> maps:get(<<"identity">>, Issue) =:= Identity end,
              Issues)).

snapshot() ->
    {ok, Body} = file:read_file(
                   "apps/retinue_cli/test/fixtures/player_inventory_sample.json"),
    Raw = jsone:decode(Body, [{object_format, map}]),
    #{revision => 7, updated_at => 1234,
      data => #{<<"inventory">> =>
                    #{<<"schema">> => 2, <<"collector">> => <<"test">>,
                      <<"raw">> => Raw}}}.

equipment_snapshot(Identity) ->
    Raw = #{<<"SpecialItems">> => [
        #{<<"ItemType">> => Identity,
          <<"ItemId">> => #{<<"$oid">> => <<"test-equipment">>},
          <<"XP">> => 0, <<"Configs">> => []}
    ]},
    #{revision => 1, updated_at => 1,
      data => #{<<"inventory">> => #{<<"schema">> => 2,
                                        <<"collector">> => <<"test">>,
                                        <<"raw">> => Raw}}}.

catalog() ->
    [#{<<"uniqueName">> => <<"/Lotus/Powersuits/TestSuit">>,
       <<"name">> => <<"Test Suit">>, <<"category">> => <<"Warframes">>,
       <<"type">> => <<"Warframe">>, <<"imageName">> => <<"test-suit.png">>},
     #{<<"uniqueName">> => <<"/Lotus/Weapons/TestGun">>,
       <<"name">> => <<"Test Gun">>, <<"category">> => <<"Primary">>,
       <<"type">> => <<"Rifle">>, <<"imageName">> => <<"test-gun.png">>},
     #{<<"uniqueName">> => <<"/Lotus/Powersuits/Excalibur/DoomSword">>,
       <<"name">> => <<"Exalted Blade">>, <<"category">> => <<"Melee">>,
       <<"type">> => <<"Exalted Weapon">>,
       <<"imageName">> => <<"exalted-blade.png">>},
     #{<<"uniqueName">> => <<"/Lotus/Upgrades/Mods/TestMod">>,
       <<"name">> => <<"Test Mod">>, <<"type">> => <<"Warframe Mod">>,
       <<"category">> => <<"Mods">>, <<"polarity">> => <<"madurai">>,
       <<"baseDrain">> => 4, <<"fusionLimit">> => 10,
       <<"compatName">> => <<"Warframe">>, <<"levelStats">> => level_stats(10),
       <<"imageName">> => <<"test-mod.png">>},
     #{<<"uniqueName">> => <<"/Lotus/Upgrades/Mods/ExaltedTestMod">>,
       <<"name">> => <<"Exalted Test Mod">>, <<"type">> => <<"Melee Mod">>,
       <<"category">> => <<"Mods">>, <<"polarity">> => <<"naramon">>},
     #{<<"uniqueName">> => <<"/Lotus/Upgrades/Mods/Warframe/TestArcane">>,
       <<"name">> => <<"Test Arcane">>, <<"type">> => <<"Warframe Arcane">>,
       <<"category">> => <<"Arcanes">>,
       <<"imageName">> => <<"test-arcane.png">>}].

regulators_snapshot() ->
    ModNames = [<<"Lethal Torrent">>, <<"Primed Heated Charge">>,
                <<"Primed Convulsion">>, <<"Pathogen Rounds">>,
                <<"Anemic Agility">>, <<"Galvanized Diffusion">>,
                <<"Primed Target Cracker">>, <<"Primed Pistol Gambit">>],
    ModIds = [<<"reg-mod-", (integer_to_binary(N))/binary>>
              || N <- lists:seq(1, 8)],
    ModRanks = [5, 10, 10, 5, 5, 10, 10, 10],
    UpgradeRefs = [#{<<"$oid">> => Id} || Id <- ModIds],
    Upgrades = [#{<<"ItemId">> => #{<<"$oid">> => Id},
                  <<"ItemType">> => mod_path(Name),
                  <<"UpgradeFingerprint">> =>
                      iolist_to_binary(io_lib:format("{\"lvl\":~B}", [Rank]))}
                || {Id, Name, Rank} <- lists:zip3(ModIds, ModNames, ModRanks)],
    ArcaneId = <<"secondary-outburst">>,
    Raw = #{<<"SpecialItems">> =>
                [#{<<"ItemType">> =>
                       <<"/Lotus/Powersuits/Cowgirl/PrimeSlingerPistols">>,
                   <<"ItemId">> => #{<<"$oid">> => <<"regulators">>},
                   <<"Features">> => 33,
                   <<"Polarity">> =>
                       [#{<<"Slot">> => 0, <<"Value">> => <<"AP_ATTACK">>},
                        #{<<"Slot">> => 1, <<"Value">> => <<"AP_TACTIC">>},
                        #{<<"Slot">> => 2, <<"Value">> => <<"AP_ANY">>},
                        #{<<"Slot">> => 3, <<"Value">> => <<"AP_TACTIC">>},
                        #{<<"Slot">> => 4, <<"Value">> => <<"AP_UNIVERSAL">>},
                        #{<<"Slot">> => 5, <<"Value">> => <<"AP_ATTACK">>},
                        #{<<"Slot">> => 6, <<"Value">> => <<"AP_ATTACK">>},
                        #{<<"Slot">> => 7, <<"Value">> => <<"AP_ATTACK">>},
                        #{<<"Slot">> => 8, <<"Value">> => <<"AP_UNIVERSAL">>}],
                   <<"Configs">> =>
                       [#{<<"Name">> => <<"Config A">>,
                          <<"Upgrades">> =>
                              UpgradeRefs ++ [null, #{<<"$oid">> => ArcaneId}]}]}],
            <<"Upgrades">> =>
                Upgrades ++
                    [#{<<"ItemId">> => #{<<"$oid">> => ArcaneId},
                       <<"ItemType">> =>
                           <<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/SecondaryOutburst">>,
                       <<"UpgradeFingerprint">> => <<"{\"lvl\":5}">>}]},
    #{revision => 8, updated_at => 2345,
      data => #{<<"inventory">> =>
                    #{<<"schema">> => 2, <<"collector">> => <<"test">>,
                      <<"raw">> => Raw}}}.

regulators_catalog() ->
    Mods = [{<<"Lethal Torrent">>, <<"madurai">>, 6, 5, <<"Grinder.jpg">>},
            {<<"Primed Heated Charge">>, <<"naramon">>, 6, 10,
             <<"PistolFireDamageMod.jpg">>},
            {<<"Primed Convulsion">>, <<"naramon">>, 6, 10,
             <<"PistolElectricityDamageMod.jpg">>},
            {<<"Pathogen Rounds">>, <<"naramon">>, 6, 5,
             <<"PoisonDamagePistolMod.jpg">>},
            {<<"Anemic Agility">>, <<"naramon">>, 4, 5,
             <<"CorruptedFireRateDamagePistolMod.jpg">>},
            {<<"Galvanized Diffusion">>, <<"madurai">>, 4, 10,
             <<"WeaponFireIterationsSPModMods.jpg">>},
            {<<"Primed Target Cracker">>, <<"madurai">>, 4, 10,
             <<"PistolCritDamageMod.jpg">>},
            {<<"Primed Pistol Gambit">>, <<"madurai">>, 2, 10,
             <<"PistolCritChanceMod.jpg">>}],
    [#{<<"uniqueName">> =>
           <<"/Lotus/Powersuits/Cowgirl/PrimeSlingerPistols">>,
       <<"name">> => <<"Regulators Prime">>, <<"category">> => <<"Misc">>,
       <<"type">> => <<"Exalted Weapon">>,
       <<"exaltedSlot">> => <<"Secondary">>,
       <<"polarities">> => [<<"madurai">>, <<"madurai">>, <<"madurai">>,
                              <<"none">>, <<"naramon">>, <<"omni">>,
                              <<"naramon">>, <<"madurai">>]},
      #{<<"uniqueName">> =>
            <<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/SecondaryOutburst">>,
        <<"name">> => <<"Secondary Outburst">>,
        <<"type">> => <<"Secondary Arcane">>, <<"category">> => <<"Arcanes">>,
        <<"rarity">> => <<"Legendary">>,
        <<"imageName">> => <<"ArcaneProjectionCU.png">>}
     | [#{<<"uniqueName">> => mod_path(Name), <<"name">> => Name,
          <<"type">> => <<"Secondary Mod">>, <<"category">> => <<"Mods">>,
          <<"polarity">> => Polarity, <<"baseDrain">> => BaseDrain,
          <<"fusionLimit">> => MaxRank, <<"rarity">> => <<"Rare">>,
          <<"isGalvanized">> => Name =:= <<"Galvanized Diffusion">>,
          <<"imageName">> => Image}
        || {Name, Polarity, BaseDrain, MaxRank, Image} <- Mods]].

mod_path(Name) -> <<"/Lotus/Upgrades/Mods/", Name/binary>>.

level_stats(MaxRank) ->
    [#{<<"stats">> =>
           [<<"Rank ", (integer_to_binary(Rank))/binary, " effect">>]}
     || Rank <- lists:seq(0, MaxRank)].

topology_slot(Id, Topology) ->
    hd([Slot
        || Region <- maps:get(<<"regions">>, Topology),
           Slot <- maps:get(<<"slots">>, Region),
           maps:get(<<"id">>, Slot) =:= Id]).

assert_mod_region(Class, Count) ->
    Definition = wfcli_build_topology:definition(Class, #{}),
    Topology = maps:get(<<"topology">>, Definition),
    [Region] = [Value
                || Value <- maps:get(<<"regions">>, Topology),
                   maps:get(<<"id">>, Value) =:= <<"mods">>],
    Slots = maps:get(<<"slots">>, Region),
    ?assertEqual(4, maps:get(<<"columns">>, Region)),
    ?assertEqual(Count, length(Slots)),
    ?assertEqual(lists:seq(Count - 1, 0, -1),
                 [maps:get(<<"player_index">>, Slot) || Slot <- Slots]),
    ?assertEqual(lists:seq(Count, 1, -1),
                 [maps:get(<<"build_slot">>, Slot) || Slot <- Slots]).

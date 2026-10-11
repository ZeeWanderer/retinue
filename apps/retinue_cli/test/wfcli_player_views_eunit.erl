-module(wfcli_player_views_eunit).

-include_lib("eunit/include/eunit.hrl").

inventory_and_mastery_join_test() ->
    Relic = <<"/Lotus/Types/Game/Projections/T1Test">>,
    Chassis = <<"/Lotus/Types/Recipes/TestPrimeChassis">>,
    Systems = <<"/Lotus/Types/Recipes/TestPrimeSystems">>,
    Frame = <<"/Lotus/Powersuits/TestPrime/TestPrime">>,
    Snapshot = #{revision => 7, updated_at => 100,
                 data => #{<<"inventory">> => #{
                     <<"profile">> => #{<<"player_name">> => <<"TestTenno">>,
                                        <<"player_level">> => 4},
                     <<"raw">> => #{
                         <<"InfestedFoundry">> => #{
                             <<"ConsumedSuits">> => [#{<<"s">> => Frame}]
                         },
                         <<"Missions">> => [
                             #{<<"Tag">> => <<"EarthNode">>,
                               <<"Completes">> => 1, <<"Tier">> => 1},
                             #{<<"Tag">> => <<"EventNode">>,
                               <<"Completes">> => 1, <<"Tier">> => 1},
                             #{<<"Tag">> => <<"EarthToVenusJunction">>,
                               <<"Completes">> => 2, <<"Tier">> => 1}]
                     },
                     <<"index">> => #{
                         <<"equipment">> => [entry(Frame, 1, 100000)],
                         <<"stacks">> => [entry(Relic, 2, 0), entry(Chassis, 1, 0)],
                         <<"mastery">> => [entry(Frame, 1, 100000)],
                         <<"pending_recipes">> => [],
                         <<"missions">> => [
                             #{<<"Tag">> => <<"EarthNode">>,
                               <<"Completes">> => 1, <<"Tier">> => 1},
                             #{<<"Tag">> => <<"EventNode">>,
                               <<"Completes">> => 1, <<"Tier">> => 1},
                             #{<<"Tag">> => <<"EarthToVenusJunction">>,
                               <<"Completes">> => 2, <<"Tier">> => 1}],
                         <<"player_skills">> => #{
                             <<"LPS_PILOTING">> => 7,
                             <<"LPS_GUNNERY">> => 8,
                             <<"LPS_DRIFT_RIDING">> => 9}
                     }}}},
    Drop = #{<<"location">> => <<"Lith T1 Relic (Intact)">>,
             <<"uniqueName">> => Relic, <<"chance">> => 25},
    Catalog = [
        #{<<"uniqueName">> => Frame, <<"name">> => <<"Test Prime">>,
          <<"category">> => <<"Warframes">>, <<"masterable">> => true,
          <<"isPrime">> => true, <<"vaulted">> => true,
          <<"imageName">> => <<"test.png">>,
          <<"components">> => [
              #{<<"uniqueName">> => Chassis, <<"name">> => <<"Chassis">>,
                <<"itemCount">> => 1, <<"tradable">> => true,
                <<"drops">> => [Drop]},
              #{<<"uniqueName">> => Systems, <<"name">> => <<"Systems">>,
                <<"itemCount">> => 1, <<"tradable">> => true,
                <<"drops">> => [Drop]}
          ]},
        #{<<"uniqueName">> => Relic, <<"name">> => <<"Lith T1 Intact">>,
          <<"category">> => <<"Relics">>, <<"masterable">> => false,
          <<"tradable">> => true,
          <<"components">> => []}
    ],

    {ok, Inventory} = wfcli_player_views:inventory(Snapshot, Catalog),
    InventoryItems = maps:get(<<"items">>, Inventory),
    ?assertEqual(3, length(InventoryItems)),
    ?assertEqual([<<"parts">>, <<"relics">>, <<"sets">>],
                 lists:sort([maps:get(<<"group">>, Item) || Item <- InventoryItems])),
    [Part] = [Item || Item <- InventoryItems,
                     maps:get(<<"group">>, Item) =:= <<"parts">>],
    ?assertEqual(<<"Test Prime Chassis">>, maps:get(<<"name">>, Part)),
    ?assertEqual(<<"Test Prime Chassis Blueprint">>,
                 maps:get(<<"market_name">>, Part)),
    ?assertEqual(true, maps:get(<<"is_prime">>, Part)),
    ?assertEqual(true, maps:get(<<"vaulted">>, Part)),
    [Set] = [Item || Item <- InventoryItems,
                    maps:get(<<"group">>, Item) =:= <<"sets">>],
    ?assertEqual(true, maps:get(<<"is_prime">>, Set)),
    ?assertEqual(true, maps:get(<<"vaulted">>, Set)),
    ?assertEqual(0, maps:get(<<"quantity">>, Set)),
    ?assertEqual(<<"Test Prime Set">>, maps:get(<<"market_name">>, Set)),
    [RelicItem] = [Item || Item <- InventoryItems,
                           maps:get(<<"group">>, Item) =:= <<"relics">>],
    ?assertEqual(<<"Lith T1 Intact">>, maps:get(<<"name">>, RelicItem)),
    ?assertEqual(<<"Lith T1 Relic">>, maps:get(<<"market_name">>, RelicItem)),

    {ok, Foundry} = wfcli_player_views:foundry(Snapshot, Catalog),
    [FoundryItem] = maps:get(<<"items">>, Foundry),
    ?assertEqual(<<"warframe">>, maps:get(<<"group">>, FoundryItem)),
    ?assertEqual(true, maps:get(<<"vaulted">>, FoundryItem)),
    ?assertEqual(true, maps:get(<<"subsumed">>, FoundryItem)),
    ?assertEqual(false, maps:get(<<"ready_to_build">>, FoundryItem)),
    ?assertEqual(1, maps:get(<<"components_owned">>, FoundryItem)),
    [ChassisComponent] =
        [Component || Component <- maps:get(<<"components">>, FoundryItem),
                      maps:get(<<"id">>, Component) =:= Chassis],
    ?assertEqual(true, maps:get(<<"owned_relic">>, ChassisComponent)),
    ?assertEqual(<<"Test Prime Chassis Blueprint">>,
                 maps:get(<<"market_name">>, ChassisComponent)),
    ?assertEqual(true, maps:get(<<"market_required">>, ChassisComponent)),

    StarChartMetadata = #{nodes => #{<<"EarthNode">> => 51},
                          junctions => #{<<"EarthToVenusJunction">> => true}},
    Projection = wfcli_player_projection:build(Snapshot),
    [ProjectedMission | _] = maps:get(<<"missions">>, Projection),
    ?assert(maps:is_key(<<"tag">>, ProjectedMission)),
    ?assertNot(maps:is_key(<<"Tag">>, ProjectedMission)),
    {ok, Mastery} = wfcli_player_views:mastery(
                      Snapshot, Catalog, StarChartMetadata),
    [MasteryItem] = maps:get(<<"items">>, Mastery),
    ?assertEqual(10, maps:get(<<"rank">>, MasteryItem)),
    ?assertEqual(4000, maps:get(<<"potential_xp">>, MasteryItem)),
    ?assert(abs(maps:get(<<"relic_probability">>, MasteryItem) - 0.4375) < 0.0001),
    ?assert(maps:get(<<"from_relics">>, MasteryItem)),
    ?assert(maps:get(<<"buyable">>, MasteryItem)),
    ?assert(maps:get(<<"has_recipe">>, MasteryItem)),
    Summary = maps:get(<<"summary">>, Mastery),
    Profile = maps:get(<<"profile">>, Mastery),
    ?assertEqual(<<"TestTenno">>,
                 maps:get(<<"player_name">>, Profile)),
    ?assertEqual(
       #{<<"id">> => <<"mastery-rank:4">>,
         <<"source">> => <<"mastery">>,
         <<"image_name">> => <<"4.webp">>},
       maps:get(<<"rank_asset">>, Profile)),
    ?assertEqual(<<"TestTenno">>, maps:get(<<"player_name">>, Summary)),
    ?assertEqual(4, maps:get(<<"player_level">>, Summary)),
    RankProgress = maps:get(<<"rank_progress">>, Summary),
    ?assertEqual(true, maps:get(<<"available">>, RankProgress)),
    ?assertEqual(102, maps:get(<<"current">>, RankProgress)),
    ?assertEqual(22500, maps:get(<<"total">>, RankProgress)),
    ?assertEqual(0, maps:get(<<"percent">>, maps:get(<<"warframes">>, Summary))),
    Intrinsics = maps:get(<<"intrinsics">>, Summary),
    ?assertEqual(15, maps:get(<<"current">>, maps:get(<<"railjack">>, Intrinsics))),
    ?assertEqual(9, maps:get(<<"current">>, maps:get(<<"duviri">>, Intrinsics))),
    StarChart = maps:get(<<"star_chart">>, Summary),
    ?assertEqual(1, maps:get(<<"current">>, maps:get(<<"normal">>, StarChart))),
    ?assertEqual(1, maps:get(<<"total">>, maps:get(<<"normal">>, StarChart))),
    ?assertEqual(1, maps:get(<<"current">>, maps:get(<<"junctions">>, StarChart))),
    ?assertEqual(1, maps:get(<<"total">>, maps:get(<<"junctions">>, StarChart))),
    ?assertEqual(100, maps:get(<<"percent">>, StarChart)).

mastery_uses_newer_equipment_xp_test() ->
    Weapon = <<"/Lotus/Weapons/Test/LiveXpWeapon">>,
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"equipment">> => [entry(Weapon, 1, 32000)],
        <<"mastery">> => [entry(Weapon, 1, 4500)]
    }}}},
    Catalog = [#{<<"uniqueName">> => Weapon,
                 <<"name">> => <<"Live XP Weapon">>,
                 <<"category">> => <<"Primary">>,
                 <<"masterable">> => true,
                 <<"components">> => []}],

    {ok, View} = wfcli_player_views:mastery(Snapshot, Catalog),
    [Item] = maps:get(<<"items">>, View),
    ?assertEqual(8, maps:get(<<"rank">>, Item)),
    ?assertEqual(800, maps:get(<<"earned_xp">>, Item)).

inventory_preserves_unknown_vault_state_test() ->
    Parent = <<"/Lotus/Weapons/Test/TestPrime">>,
    Part = <<"/Lotus/Weapons/Test/TestPrimeBarrel">>,
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"stacks">> => [entry(Part, 1, 0)], <<"mastery">> => []}}}},
    Catalog = [#{<<"uniqueName">> => Parent, <<"name">> => <<"Test Prime">>,
                 <<"category">> => <<"Primary">>, <<"isPrime">> => true,
                 <<"components">> => [
                     #{<<"uniqueName">> => Part, <<"name">> => <<"Barrel">>,
                       <<"tradable">> => true}]}],
    {ok, Inventory} = wfcli_player_views:inventory(Snapshot, Catalog),
    [Item] = maps:get(<<"items">>, Inventory),
    ?assertEqual(true, maps:get(<<"is_prime">>, Item)),
    ?assertEqual(null, maps:get(<<"vaulted">>, Item)).

inventory_includes_mod_variants_and_recipe_aliases_test() ->
    Hush = <<"/Lotus/Upgrades/Mods/Rifle/WeaponNoiseReductionMod">>,
    Chroma = <<"/Lotus/Powersuits/Dragon/Dragon">>,
    ChromaBlueprint = <<"/Lotus/Types/Recipes/WarframeRecipes/ChromaBlueprint">>,
    Chassis = <<"/Lotus/Types/Recipes/WarframeRecipes/ChromaChassisComponent">>,
    ChassisBlueprint =
        <<"/Lotus/Types/Recipes/WarframeRecipes/ChromaChassisBlueprint">>,
    Snapshot = #{data => #{<<"inventory">> => #{
        <<"raw">> => #{
            <<"LongGuns">> => [#{<<"ItemId">> => <<"rifle-1">>,
                                  <<"ItemType">> => <<"/Lotus/Weapons/TestRifle">>,
                                  <<"Configs">> => [#{<<"Upgrades">> => [Hush]}]}],
            <<"RawUpgrades">> => [#{<<"ItemType">> => Hush,
                                      <<"ItemCount">> => 31}]},
        <<"index">> => #{
            <<"stacks">> => [entry(ChromaBlueprint, 1, 0),
                               entry(ChassisBlueprint, 2, 0)],
            <<"mastery">> => []}}}},
    Catalog = [
        #{<<"uniqueName">> => Hush, <<"name">> => <<"Hush">>,
          <<"category">> => <<"Mods">>, <<"tradable">> => true,
          <<"fusionLimit">> => 5, <<"imageName">> => <<"Hush.png">>,
          <<"components">> => []},
        #{<<"uniqueName">> => Chroma, <<"name">> => <<"Chroma">>,
          <<"category">> => <<"Warframes">>, <<"components">> => [
              #{<<"uniqueName">> => ChromaBlueprint, <<"name">> => <<"Blueprint">>,
                <<"tradable">> => false, <<"imageName">> => <<"Chroma.png">>},
              #{<<"uniqueName">> => Chassis, <<"name">> => <<"Chassis">>,
                <<"tradable">> => false, <<"imageName">> => <<"Chassis.png">>,
                <<"recipeAliases">> => [ChassisBlueprint]}
          ]}
    ],
    {ok, Inventory} = wfcli_player_views:inventory(Snapshot, Catalog),
    Items = maps:from_list([{maps:get(<<"name">>, Item), Item}
                            || Item <- maps:get(<<"items">>, Inventory)]),
    HushItem = maps:get(<<"Hush">>, Items),
    ?assertEqual(<<"mods">>, maps:get(<<"group">>, HushItem)),
    ?assertEqual(31, maps:get(<<"quantity">>, HushItem)),
    ?assertEqual(0, maps:get(<<"rank">>, HushItem)),
    ?assertEqual(5, maps:get(<<"max_rank">>, HushItem)),
    ?assertEqual(true, maps:get(<<"equipped">>, HushItem)),
    ?assertEqual(1, length(maps:get(<<"equipped_in">>, HushItem))),
    NormalBlueprint = maps:get(<<"Chroma Blueprint">>, Items),
    ?assertEqual(<<"parts">>, maps:get(<<"group">>, NormalBlueprint)),
    ChassisItem = maps:get(<<"Chroma Chassis">>, Items),
    ?assertEqual(<<"parts">>, maps:get(<<"group">>, ChassisItem)),
    ?assertEqual(2, maps:get(<<"quantity">>, ChassisItem)),
    ?assertEqual(<<"Chassis.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, ChassisItem))).

star_chart_compaction_test() ->
    Regions = #{
        <<"SolNode1">> => #{<<"masteryExp">> => 51},
        <<"EventNode">> => #{<<"masteryExp">> => 0},
        <<"EarthToVenusJunction">> => #{<<"masteryExp">> => 0},
        <<"NotAJunction">> => #{<<"masteryExp">> => 0}},
    Chart = wfcli_star_chart:compact(Regions),
    ?assertEqual(#{<<"SolNode1">> => 51}, maps:get(nodes, Chart)),
    ?assertEqual(#{<<"EarthToVenusJunction">> => true},
                 maps:get(junctions, Chart)).

mastery_market_names_test() ->
    Parent = <<"/Lotus/Weapons/Test/AklexPrime">>,
    Weapon = <<"/Lotus/Weapons/Test/LexPrime">>,
    Cell = <<"/Lotus/Types/Items/MiscItems/OrokinCell">>,
    Forma = <<"/Lotus/Types/Items/MiscItems/Forma">>,
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{}}}},
    Catalog = [
        #{<<"uniqueName">> => Parent, <<"name">> => <<"Aklex Prime">>,
          <<"category">> => <<"Secondary">>, <<"masterable">> => true,
          <<"components">> => [
              #{<<"uniqueName">> => Weapon, <<"name">> => <<"Lex Prime">>,
                <<"tradable">> => true},
              #{<<"uniqueName">> => Cell, <<"name">> => <<"Orokin Cell">>,
                <<"tradable">> => false, <<"itemCount">> => 10},
              #{<<"uniqueName">> => Forma, <<"name">> => <<"Forma">>,
                <<"tradable">> => false}
          ]},
        #{<<"uniqueName">> => Weapon, <<"name">> => <<"Lex Prime">>,
          <<"category">> => <<"Secondary">>, <<"masterable">> => true,
          <<"components">> => []}
    ],
    {ok, Mastery} = wfcli_player_views:mastery(Snapshot, Catalog),
    [Item] = [Value || Value <- maps:get(<<"items">>, Mastery),
                       maps:get(<<"id">>, Value) =:= Parent],
    [Lex, OrokinCell, FormaBlueprint] = maps:get(<<"components">>, Item),
    ?assertEqual(<<"Lex Prime Set">>, maps:get(<<"market_name">>, Lex)),
    ?assertEqual(null, maps:get(<<"market_name">>, OrokinCell)),
    ?assertEqual(<<"Forma">>, maps:get(<<"name">>, FormaBlueprint)),
    ?assertEqual(null, maps:get(<<"market_name">>, FormaBlueprint)),
    ?assertEqual(false, maps:get(<<"market_required">>, FormaBlueprint)),
    ?assertEqual(true, maps:get(<<"buyable">>, Item)).

duplicate_recipe_slots_share_owned_count_test() ->
    Twin = <<"/Lotus/Weapons/Test/TwinKrohkur">>,
    Krohkur = <<"/Lotus/Weapons/Test/Krohkur">>,
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"equipment">> => [entry(Krohkur, 1, 0)]}}}},
    Catalog = [#{<<"uniqueName">> => Twin, <<"name">> => <<"Twin Krohkur">>,
                 <<"category">> => <<"Melee">>, <<"masterable">> => true,
                 <<"components">> => [
                     #{<<"uniqueName">> => Krohkur, <<"name">> => <<"Krohkur">>,
                       <<"itemCount">> => 1},
                     #{<<"uniqueName">> => Krohkur, <<"name">> => <<"Krohkur">>,
                       <<"itemCount">> => 1}]}],

    {ok, Foundry} = wfcli_player_views:foundry(Snapshot, Catalog),
    [Item] = maps:get(<<"items">>, Foundry),
    ?assertEqual([1, 0], [maps:get(<<"owned">>, Component)
                          || Component <- maps:get(<<"components">>, Item)]),
    ?assertEqual(1, maps:get(<<"components_owned">>, Item)),
    ?assertEqual(2, maps:get(<<"components_required">>, Item)),
    ?assertEqual(1, maps:get(<<"missing_parts">>, Item)),
    ?assertEqual(false, maps:get(<<"ready_to_build">>, Item)).

foundry_distinguishes_part_blueprints_from_built_parts_test() ->
    Frame = <<"/Lotus/Powersuits/Test/RecipeFrame">>,
    Chassis = <<"/Lotus/Types/Recipes/Test/RecipeFrameChassis">>,
    ChassisBlueprint =
        <<"/Lotus/Types/Recipes/Test/RecipeFrameChassisBlueprint">>,
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"stacks">> => [entry(ChassisBlueprint, 2, 0)]}}}},
    Catalog = [#{<<"uniqueName">> => Frame, <<"name">> => <<"Recipe Frame">>,
                 <<"category">> => <<"Warframes">>, <<"masterable">> => true,
                 <<"components">> => [
                     #{<<"uniqueName">> => Chassis, <<"name">> => <<"Chassis">>,
                       <<"itemCount">> => 1,
                       <<"recipeAliases">> => [ChassisBlueprint]}]}],

    {ok, Foundry} = wfcli_player_views:foundry(Snapshot, Catalog),
    [Item] = maps:get(<<"items">>, Foundry),
    [Component] = maps:get(<<"components">>, Item),
    ?assertEqual(0, maps:get(<<"owned">>, Component)),
    ?assertEqual(2, maps:get(<<"blueprint_owned">>, Component)),
    ?assertEqual([ChassisBlueprint], maps:get(<<"recipe_ids">>, Component)),
    ?assertEqual(0, maps:get(<<"components_owned">>, Item)),
    ?assertEqual(false, maps:get(<<"ready_to_build">>, Item)).

refined_relic_probability_uses_variant_inventory_test() ->
    Frame = <<"/Lotus/Powersuits/Test/RefinementPrime">>,
    Part = <<"/Lotus/Types/Recipes/TestRefinementPart">>,
    Intact = <<"/Lotus/Types/Game/Projections/T4TestABronze">>,
    Radiant = <<"/Lotus/Types/Game/Projections/T4TestAPlatinum">>,
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"stacks">> => [entry(Intact, 2, 0), entry(Radiant, 1, 0)]}}}},
    Drops = [
        #{<<"location">> => <<"Axi T1 Relic">>,
          <<"uniqueName">> => Intact, <<"chance">> => 25},
        #{<<"location">> => <<"Axi T1 Relic (Exceptional)">>,
          <<"uniqueName">> => Intact, <<"chance">> => 20},
        #{<<"location">> => <<"Axi T1 Relic (Flawless)">>,
          <<"uniqueName">> => Intact, <<"chance">> => 15},
        #{<<"location">> => <<"Axi T1 Relic (Radiant)">>,
          <<"uniqueName">> => Intact, <<"chance">> => 10}],
    Catalog = [#{<<"uniqueName">> => Frame, <<"name">> => <<"Refinement Prime">>,
                 <<"category">> => <<"Warframes">>, <<"masterable">> => true,
                 <<"components">> => [
                     #{<<"uniqueName">> => Part, <<"name">> => <<"Blueprint">>,
                       <<"itemCount">> => 1, <<"drops">> => Drops}]}],

    {ok, Mastery} = wfcli_player_views:mastery(Snapshot, Catalog),
    [Item] = maps:get(<<"items">>, Mastery),
    [Component] = maps:get(<<"components">>, Item),
    Expected = 1.0 - math:pow(0.75, 2) * 0.9,
    ?assert(abs(maps:get(<<"relic_probability">>, Item) - Expected) < 0.0001),
    ?assert(abs(maps:get(<<"relic_probability">>, Component) - Expected) < 0.0001),
    ?assert(maps:get(<<"owned_relic">>, Component)).

item_catalog_compaction_test() ->
    Item = #{<<"uniqueName">> => <<"/Lotus/Test">>,
             <<"name">> => <<"Test">>, <<"vaulted">> => true,
             <<"exaltedSlot">> => <<"Secondary">>,
             <<"noise">> => true,
             <<"components">> => [
                 #{<<"uniqueName">> => <<"/Lotus/TestPart">>,
                   <<"name">> => <<"Part">>,
                   <<"drops">> => [
                       #{<<"location">> => <<"Lith T1 Relic (Radiant)">>,
                         <<"uniqueName">> => <<"/Lotus/Relic">>,
                         <<"chance">> => 10, <<"noise">> => true},
                       #{<<"location">> => <<"Enemy Drop">>,
                         <<"uniqueName">> => <<"/Lotus/Enemy">>}
                   ]}
             ]},
    Compact = wfcli_item_catalog:compact(Item),
    ?assertNot(maps:is_key(<<"noise">>, Compact)),
    ?assertEqual(true, maps:get(<<"vaulted">>, Compact)),
    ?assertEqual(<<"Secondary">>, maps:get(<<"exaltedSlot">>, Compact)),
    [Component] = maps:get(<<"components">>, Compact),
    [Drop] = maps:get(<<"drops">>, Component),
    ?assertEqual(<<"/Lotus/Relic">>, maps:get(<<"uniqueName">>, Drop)).

canonical_catalog_items_beat_recipe_components_test() ->
    Forma = <<"/Lotus/Types/Items/MiscItems/Forma">>,
    Kuva = <<"/Lotus/Types/Items/MiscItems/Kuva">>,
    Recipe = #{<<"uniqueName">> => <<"/Lotus/Weapons/Test/Recipe">>,
               <<"name">> => <<"Test Recipe">>,
               <<"components">> => [
                   #{<<"uniqueName">> => Forma, <<"name">> => <<"Test Recipe Forma">>},
                   #{<<"uniqueName">> => Kuva, <<"name">> => <<"Test Recipe Kuva">>}
               ]},
    Catalog = [
        #{<<"uniqueName">> => Forma, <<"name">> => <<"Forma">>,
          <<"imageName">> => <<"GenericComponent.png">>, <<"components">> => []},
        Recipe,
        #{<<"uniqueName">> => Kuva, <<"name">> => <<"Kuva">>,
          <<"imageName">> => <<"Kuva.png">>, <<"components">> => []}
    ],
    Index = wfcli_item_catalog:index(Catalog),
    ?assertEqual(false, maps:get(<<"component">>, wfcli_item_catalog:lookup(Forma, Index))),
    ?assertEqual(false, maps:get(<<"component">>, wfcli_item_catalog:lookup(Kuva, Index))),
    Snapshot = #{data => #{<<"inventory">> => #{<<"index">> => #{
        <<"stacks">> => [entry(Forma, 1, 0), entry(Kuva, 10, 0)]}}}},
    {ok, Inventory} = wfcli_player_views:inventory(Snapshot, Catalog),
    Items = maps:from_list([{maps:get(<<"id">>, Item), Item}
                            || Item <- maps:get(<<"items">>, Inventory)]),
    FormaItem = maps:get(Forma, Items),
    KuvaItem = maps:get(Kuva, Items),
    ?assertEqual(<<"Forma">>, maps:get(<<"name">>, FormaItem)),
    ?assertEqual(<<"Kuva">>, maps:get(<<"name">>, KuvaItem)),
    ?assertEqual(<<"wfcd">>, maps:get(<<"name_source">>, FormaItem)),
    ?assertEqual(<<"GenericComponent.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, FormaItem))),
    ?assertEqual(<<"Kuva.png">>,
                 maps:get(<<"image_name">>, maps:get(<<"asset">>, KuvaItem))).

entry(Type, Count, Xp) ->
    #{<<"item_type">> => Type, <<"count">> => Count, <<"xp">> => Xp}.

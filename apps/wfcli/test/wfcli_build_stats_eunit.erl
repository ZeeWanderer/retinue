-module(wfcli_build_stats_eunit).

-include_lib("eunit/include/eunit.hrl").

-export([weapon/0, equipment/0, options/0]).

-define(VASTO, <<"/Lotus/Weapons/Tenno/Pistols/PrimeVasto/PrimeVastoPistol">>).
-define(MESA, <<"/Lotus/Powersuits/Cowgirl/MesaPrime">>).
-define(CERAMIC, <<"/Lotus/Weapons/Tenno/Melee/Dagger/CeramicDagger">>).
-define(OUTBURST, <<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/MultishotForCombo">>).

unowned_catalog_weapon_test() ->
    {ok, Result} = calc(unmodded(), #{}),
    ?assertEqual(ready, maps:get(status, Result)),
    ?assertEqual(null, maps:get(config_index, maps:get(<<"weapon">>, maps:get(buildset, Result)))),
    near(143.88, maps:get(expected_target_damage_per_trigger, maps:get(stats, Result))),
    ?assert(is_binary(jsone:encode(Result))).

ranked_mods_and_mesa_passive_test() ->
    {ok, Result} = calc(options(), equipment()),
    Stats = maps:get(stats, Result),
    near(1 / 1.85, maps:get(reload_time, Stats)),
    near(0.6314, maps:get(critical_chance, Stats)),
    near(2.3990232944488525 * 2.1, maps:get(critical_multiplier, Stats)),
    near(2.1, maps:get(multishot, Stats)),
    Damage = maps:get(damage_per_projectile, Stats),
    near(198, maps:get(corrosive, Damage)),
    near(181.5, maps:get(heat, Damage)),
    near(maps:get(expected_raw_damage_per_projectile, Stats) * 2.1,
         maps:get(expected_raw_damage_per_trigger, Stats)),
    near(2.43, maps:get(ability_strength, maps:get(<<"warframe">>, maps:get(frame_multipliers, Result)))),
    ?assertEqual(101, maps:get(player_revision, Result)),
    ?assertEqual(0, maps:get(config_index, maps:get(<<"weapon">>, maps:get(buildset, Result)))),
    ?assertEqual(partial, maps:get(status, Result)),
    ?assert(is_binary(jsone:encode(Result))),
    Text = iolist_to_binary(wfcli_build_stats_cli:format(Result)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Partial direct-fire estimate">>)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Per-projectile outcomes (body hits)">>)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Critical tier">>)).

normalized_topology_controls_order_test() ->
    E = equipment(),
    [W, F] = maps:get(<<"instances">>, E),
    [Config] = maps:get(<<"configs">>, W),
    Shuffled = Config#{<<"upgrade_slots">> => lists:reverse(maps:get(<<"upgrade_slots">>, Config))},
    {ok, Before} = calc(options(), E),
    {ok, After} = calc(options(), E#{<<"instances">> => [W#{<<"configs">> => [Shuffled]}, F]}),
    ?assertEqual(maps:get(stats, Before), maps:get(stats, After)),
    ?assertEqual(maps:get(effect_graph, Before), maps:get(effect_graph, After)).

research_dependencies_match_tracker_test() ->
    {ok, Data} = file:read_file("apps/wfcompanion/src/inspect/luau/features.json"),
    Features = maps:get(<<"features">>, jsone:decode(Data)),
    {ok, Result} = calc(options(), equipment()),
    Dependencies = maps:get(dependencies, maps:get(provenance, Result)),
    Mechanics = maps:get(mechanics, Dependencies),
    ?assert(lists:member(<<"calculator.direct-fire.core">>, Mechanics)),
    ?assert(lists:member(<<"calculator.mesa">>, Mechanics)),
    ?assert(lists:member(<<"calculator.upgrades">>, Mechanics)),
    ?assert(lists:all(fun(Id) -> maps:is_key(Id, Features) end, Mechanics)),
    ?assert(lists:member(?VASTO, maps:get(properties, Dependencies))),
    ?assert(lists:member(?MESA, maps:get(properties, Dependencies))),
    Core = maps:get(<<"calculator.direct-fire.core">>, Features),
    ?assert(lists:member(<<"calculator.health-packet">>, maps:get(<<"depends_on">>, Core))),
    Packet = maps:get(<<"calculator.health-packet">>, Features),
    ?assert(lists:all(fun(Id) -> lists:member(Id, maps:get(<<"native">>, Packet)) end,
        [<<"damage.packet-init">>, <<"damage.channel-rescale">>, <<"damage.hit-normalization">>,
         <<"damage.critical-factor">>, <<"damage.health-channels">>, <<"damage.health-conversion">>])).

conditional_stacks_are_explicit_test() ->
    {ok, Before} = calc(options(), equipment()),
    {ok, After} = calc((options())#{<<"effects">> =>
        #{<<"weapon/galvanized-diffusion">> => 4, <<"warframe/arcane-velocity">> => 1}}, equipment()),
    near(3.3, maps:get(multishot, maps:get(stats, After))),
    near(5.5, maps:get(fire_rate, maps:get(stats, After))),
    near(2.5, maps:get(fire_rate, maps:get(stats, Before))),
    ?assertMatch({error, {invalid_effect_stacks, _}},
                 calc((options())#{<<"effects">> => #{<<"weapon/galvanized-diffusion">> => 5}}, equipment())),
    ?assertMatch({error, {unavailable_effects, _}},
                 calc((options())#{<<"effects">> => #{<<"nourish">> => 1}}, equipment())).

unsupported_inputs_are_visible_test() ->
    E = equipment(),
    [W, F] = maps:get(<<"instances">>, E),
    [Config] = maps:get(<<"configs">>, W),
    Slots = maps:get(<<"upgrade_slots">>, Config),
    Unknown = (slot(9, <<"/unknown/mod">>, 3))#{<<"topology_slot">> => <<"exilus">>},
    [First | Rest] = Slots,
    BadRank = First#{<<"rank">> => 99},
    W1 = W#{<<"features">> => #{<<"incarnon_genesis">> => true},
            <<"configs">> => [Config#{<<"upgrade_slots">> => [BadRank, Unknown | Rest]}]},
    {ok, Result} = calc(options(), E#{<<"instances">> => [W1, F]}),
    Encoded = jsone:encode(maps:get(excluded, Result)),
    ?assertNotEqual(nomatch, binary:match(Encoded, <<"unsupported upgrade">>)),
    ?assertNotEqual(nomatch, binary:match(Encoded, <<"invalid or unknown rank">>)),
    ?assertNotEqual(nomatch, binary:match(Encoded, <<"Incarnon evolutions">>)).

selection_is_unambiguous_test() ->
    E = equipment(),
    [W, F] = maps:get(<<"instances">>, E),
    W2 = W#{<<"instance_id">> => <<"weapon-2">>},
    Both = E#{<<"instances">> => [W, W2, F]},
    ?assertMatch({error, {ambiguous_equipment, _, _}}, calc(options(), Both)),
    ?assertMatch({ok, _}, calc(member_options(options(), <<"weapon">>, #{<<"item">> => <<"weapon-2">>}), Both)),
    NoActive = E#{<<"instances">> => [W#{<<"active_config_indices">> => []}, F]},
    ?assertMatch({error, {config_required, _}}, calc(options(), NoActive)),
    ?assertMatch({ok, _}, calc(member_options(options(), <<"weapon">>, #{<<"config">> => 0}), NoActive)),
    ?assertMatch({error, {config_not_found, _, 3}},
                 calc(member_options(options(), <<"weapon">>, #{<<"config">> => 3}), E)).

no_missing_base_defaults_test() ->
    Opts = unmodded(),
    ?assertEqual({error, {invalid_base_stat, <<"criticalChance">>}},
        wfcli_build_stats:calculate(Opts, #{}, [maps:remove(<<"criticalChance">>, weapon())])),
    ?assertEqual({error, changed_attack_projectiles},
        wfcli_build_stats:calculate(Opts, #{}, [(weapon())#{<<"multishot">> => 6}])),
    ?assertEqual({error, inconsistent_base_damage},
        wfcli_build_stats:calculate(Opts, #{}, [(weapon())#{<<"totalDamage">> => 420}])).

request_validation_test() ->
    Bad = [#{}, [], #{<<"target">> => 4},
           (options())#{<<"unknown">> => true},
           (options())#{<<"armor">> => -1}, (options())#{<<"armor">> => <<"x">>},
           (options())#{<<"armor">> => 0.5}, (options())#{<<"armor">> => 10000001},
           (options())#{<<"effects">> => []},
           member_options(options(), <<"weapon">>, #{<<"config">> => -1}),
           (options())#{<<"buildset">> => []}, (options())#{<<"target">> => <<"missing">>}],
    [?assertEqual({error, invalid_stat_request}, wfcli_build_stats:validate(O)) || O <- Bad].

changed_game_is_not_reported_current_test() ->
    {ok, Result} = calc(options(), (equipment())#{<<"game_executable_sha256">> => <<"changed">>}),
    ?assertEqual(false, maps:get(executable_matches, maps:get(provenance, Result))),
    ?assert(lists:member(<<"Observed game executable differs from the researched rule version">>,
                         maps:get(excluded, Result))).

cli_request_test() ->
    {ok, Args, ["build-stats"], _} = wfcli_cli_args:parse(
        ["build-stats", "Vasto Prime", "--config", "0", "--armor", "2700",
         "--with", "warframe=Mesa Prime", "--with", "melee=Ceramic Dagger", "--config-for", "melee=0",
         "--bounds", "--effect", "item/galvanized-diffusion=4", "--format", "json"]),
    {ok, #{source := build_stats, options := Options}} = wfcli_build_stats_cli:request(Args),
    ?assertEqual(#{<<"item/galvanized-diffusion">> => 4}, maps:get(<<"effects">>, Options)),
    ?assertEqual(true, maps:get(<<"bounds">>, Options)),
    ?assertEqual(#{<<"item">> => <<"Ceramic Dagger">>, <<"config">> => 0},
                 maps:get(<<"melee">>, maps:get(<<"buildset">>, Options))),
    ?assertEqual(2700, maps:get(<<"armor">>, Options)),
    ?assertEqual(ok, wfcli_build_stats:validate(Options)),
    ?assertMatch({error, {duplicate_effect, _}}, wfcli_build_stats_cli:request(
        #{item => "Vasto Prime", effects => ["a=1", "a=2"]})),
    ?assertMatch({error, {invalid_effect, _}}, wfcli_build_stats_cli:request(
        #{item => "Vasto Prime", effects => ["a=-1"]})),
    ?assertEqual({help, ["build-stats"]}, wfcli_cli_args:parse(["build-stats", "--help"])),
    ?assertEqual(wfcli_build_service, wfcli_protocol:owner(#{source => build_stats})).

buildset_bounds_trace_synergies_test() ->
    {ok, Result} = calc(synergy_options(), #{}),
    ?assertEqual(0, maps:get(player_revision, Result)),
    Bounds = maps:get(bounds, Result),
    Lower = maps:get(lower, Bounds),
    Upper = maps:get(upper, Bounds),
    near(0.22 * (1 + 1.87 + 0.8), maps:get(critical_chance, Lower)),
    near(2.3990232944488525 * (1 + 1.1 + 0.8), maps:get(critical_multiplier, Lower)),
    near(0.22 * (1 + 1.87 + 3.75 + 2.4), maps:get(critical_chance, Upper)),
    near(2.3990232944488525 * (1 + 1.1 + 2.4), maps:get(critical_multiplier, Upper)),
    ?assertEqual(12, maps:get(magazine, Lower)),
    ?assertEqual(0.0, maps:get(ammo_per_trigger, Upper)),
    ?assertEqual(maps:get(theoretical_burst_dps, Upper), maps:get(theoretical_sustained_dps, Upper)),
    Combo = graph_node(<<"melee/combo">>, Result),
    near(66, maps:get(initial_combo, maps:get(lower, maps:get(states, Combo)))),
    ?assertEqual(4, maps:get(multiplier, maps:get(lower, maps:get(states, Combo)))),
    ?assertEqual(12, maps:get(multiplier, maps:get(upper, maps:get(states, Combo)))),
    ?assert(lists:member(<<"melee/evolution-3">>, maps:get(depends_on, Combo))),
    ?assertEqual([<<"melee/combo">>], maps:get(depends_on, graph_node(<<"weapon/arcane-1">>, Result))),
    Shard = graph_node(<<"warframe/shard-1">>, Result),
    ?assertEqual([], maps:get(effects, maps:get(lower, maps:get(states, Shard)))),
    [Effect] = maps:get(effects, maps:get(upper, maps:get(states, Shard))),
    near(0.75, maps:get(value, Effect)),
    ?assert(is_binary(jsone:encode(Result))),
    ?assertNotEqual(nomatch, binary:match(iolist_to_binary(wfcli_build_stats_cli:format(Result)),
                                         <<"combo multiplier 4x to 12x">>)).

selected_combo_and_inactive_state_are_distinct_test() ->
    Options = (synergy_options())#{<<"effects">> => #{
        <<"weapon/secondary-outburst">> => 1, <<"melee/melee-crescendo">> => 15}},
    {ok, Result} = calc(Options, #{}),
    Combo = maps:get(selected, maps:get(states, graph_node(<<"melee/combo">>, Result))),
    near(126, maps:get(initial_combo, Combo)),
    ?assertEqual(7, maps:get(multiplier, Combo)),
    near(0.22 * (1 + 1.87 + 1.4), maps:get(critical_chance, maps:get(stats, Result))),
    near(0.6314, maps:get(critical_chance, maps:get(inactive, maps:get(bounds, Result)))),
    {ok, Capped} = calc(Options#{<<"effects">> => #{<<"melee/melee-crescendo">> => 60}}, #{}),
    CappedCombo = maps:get(selected, maps:get(states, graph_node(<<"melee/combo">>, Capped))),
    near(220, maps:get(initial_combo, CappedCombo)),
    ?assertEqual(12, maps:get(multiplier, CappedCombo)),
    {ok, Consumed} = calc(Options#{<<"effects">> => #{<<"melee/combo">> => 0,
                                                      <<"weapon/secondary-outburst">> => 1}}, #{}),
    ConsumedCombo = maps:get(selected, maps:get(states, graph_node(<<"melee/combo">>, Consumed))),
    ?assertEqual(0, maps:get(combo, ConsumedCombo)),
    near(66, maps:get(initial_combo, ConsumedCombo)),
    near(0.6314, maps:get(critical_chance, maps:get(stats, Consumed))).

source_target_filters_and_requirements_test() ->
    Options = synergy_options(),
    Builds = maps:get(<<"buildset">>, Options),
    {ok, Base} = calc(Options, #{}),
    #{<<"build">> := Frame} = maps:get(<<"warframe">>, Builds),
    [Config] = maps:get(<<"configs">>, hd(maps:get(<<"instances">>, equipment()))),
    Rifle = inline(<<"/Test/Rifle">>, <<"primary">>, maps:get(<<"upgrade_slots">>, Config)),
    WithPrimary = Options#{<<"buildset">> => Builds#{<<"primary">> => #{<<"build">> => Rifle}}},
    {ok, Context} = calc(WithPrimary, #{}),
    ?assertEqual(maps:get(stats, Base), maps:get(stats, Context)),
    near((0.22 + 0.30) * (1 + 1.87 + 3.75 + 2.4),
         maps:get(critical_chance, maps:get(upper, maps:get(bounds, Context)))),
    ?assertMatch({error, {unmet_effect_requirement, _}}, calc(Options#{<<"effects">> =>
        #{<<"weapon/deathtrap-trigger">> => 1}}, #{})),
    {ok, Alone} = calc(Options#{<<"buildset">> => maps:with([<<"weapon">>], Builds)}, #{}),
    ?assertNotEqual(nomatch, binary:match(jsone:encode(maps:get(excluded, Alone)),
                                         <<"Outburst requires">>)),
    ?assertEqual(false, lists:any(fun(#{owner := Owner}) -> Owner =:= <<"warframe">> end,
                                  maps:get(effect_graph, Alone))),
    ?assertEqual(5, length(maps:get(<<"shard_slots">>, Frame))).

unknown_skill_tree_and_inline_data_test() ->
    Options = synergy_options(),
    Builds = maps:get(<<"buildset">>, Options),
    #{<<"build">> := W} = maps:get(<<"weapon">>, Builds),
    {ok, InvalidTree} = calc(Options#{<<"buildset">> => Builds#{<<"weapon">> =>
        #{<<"build">> => W#{<<"skill_tree">> => <<"0992">>}}}}, #{}),
    ?assertNotEqual(nomatch, binary:match(jsone:encode(maps:get(excluded, InvalidTree)),
                                         <<"Unrecognized Incarnon">>)),
    BadBuilds = [W#{<<"config">> => []}, W#{<<"features">> => #{<<"incarnon_genesis">> => 4}},
                 W#{<<"topology">> => #{<<"regions">> => [null]}}, W#{<<"shard_slots">> => [#{}]}],
    [?assertEqual({error, invalid_stat_request}, calc(Options#{<<"buildset">> =>
        Builds#{<<"weapon">> => #{<<"build">> => B}}}, #{})) || B <- BadBuilds].

automatic_evolution_uses_buildset_presence_test() ->
    W = (inline(?VASTO, <<"secondary">>, []))#{<<"skill_tree">> => <<"0000">>},
    Options = #{<<"target">> => <<"sidearm">>, <<"bounds">> => true,
                <<"buildset">> => #{<<"sidearm">> => #{<<"build">> => W}}},
    {ok, Alone} = calc(Options, #{}),
    ?assertEqual(26, maps:get(magazine, maps:get(stats, Alone))),
    [Condition] = [C || C <- maps:get(available_effects, Alone), maps:get(id, C) =:= <<"sidearm/lone-gun">>],
    ?assertEqual(1, maps:get(active_stacks, Condition)),
    ?assertEqual(maps:get(inactive, maps:get(bounds, Alone)), maps:get(upper, maps:get(bounds, Alone))),
    ?assertEqual({error, {automatic_effect, <<"sidearm/lone-gun">>}},
        calc(Options#{<<"effects">> => #{<<"sidearm/lone-gun">> => 0}}, #{})),
    Builds = maps:get(<<"buildset">>, Options),
    Rifle = inline(<<"/Test/Rifle">>, <<"primary">>, []),
    {ok, WithPrimary} = calc(Options#{<<"buildset">> => Builds#{<<"primary">> => #{<<"build">> => Rifle}}}, #{}),
    ?assertEqual(12, maps:get(magazine, maps:get(stats, WithPrimary))),
    ?assertNotEqual(nomatch, binary:match(jsone:encode(maps:get(excluded, WithPrimary)),
                                         <<"Intrinsic and ability effects">>)).

inline_file_cli_and_duplicate_instance_test() ->
    Path = filename:join("/tmp", "wfcli-buildset-" ++ integer_to_list(erlang:unique_integer([positive])) ++ ".json"),
    Options = synergy_options(),
    ok = file:write_file(Path, jsone:encode(maps:get(<<"buildset">>, Options))),
    try
        {ok, Args, ["build-stats"], _} = wfcli_cli_args:parse(
            ["build-stats", "--buildset", Path, "--target", "weapon", "--bounds"]),
        {ok, #{options := Parsed}} = wfcli_build_stats_cli:request(Args),
        ?assertMatch({ok, _}, calc(Parsed, #{}))
    after file:delete(Path)
    end,
    ?assertMatch({error, {missing_buildset_target, _}}, wfcli_build_stats_cli:request(#{})),
    ?assertMatch({error, {duplicate_buildset_slot, _}}, wfcli_build_stats_cli:request(
        #{item => "Vasto Prime", members => ["item=Vasto Prime"]})),
    ?assertEqual({error, duplicate_buildset_instance}, calc(member_options(options(), <<"copy">>,
        #{<<"item">> => <<"Vasto Prime">>}), equipment())).

graph_node(Id, Result) ->
    [Node] = [N || N <- maps:get(effect_graph, Result), maps:get(id, N) =:= Id],
    Node.

synergy_options() ->
    [W, F] = maps:get(<<"instances">>, equipment()),
    [WC] = maps:get(<<"configs">>, W),
    [FC] = maps:get(<<"configs">>, F),
    Outburst = (slot(9, ?OUTBURST, 5))#{<<"topology_slot">> => <<"arcane-1">>},
    Weapon = (inline(?VASTO, <<"secondary">>, maps:get(<<"upgrade_slots">>, WC) ++ [Outburst]))#{
        <<"skill_tree">> => <<"0102">>},
    Frame = (inline(?MESA, <<"warframe">>, maps:get(<<"upgrade_slots">>, FC)))#{
        <<"shard_slots">> => [#{<<"slot_id">> => <<"shard-", (integer_to_binary(N))/binary>>,
            <<"upgrade">> => #{<<"UpgradeType">> =>
                <<"/Lotus/Upgrades/Invigorations/ArchonCrystalUpgrades/ArchonCrystalUpgradeWeaponCritBoostFromHeatMythic">>}}
            || N <- lists:seq(1, 5)]},
    MeleeSlots = [slot(1, <<"/Lotus/Upgrades/Mods/Melee/DualStat/CorruptedHeavyDamageChargeSpeedMod">>, 3),
                  slot(2, <<"/Lotus/Upgrades/Mods/Melee/WeaponMeleeStealthLethalMod">>, 3),
                  (slot(3, <<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/PermaComboOnFinisher">>, 3))#{
                      <<"topology_slot">> => <<"arcane-1">>}],
    Melee = (inline(?CERAMIC, <<"melee">>, MeleeSlots))#{<<"skill_tree">> => <<"0010">>},
    #{<<"target">> => <<"weapon">>, <<"bounds">> => true, <<"buildset">> => #{
        <<"weapon">> => #{<<"build">> => Weapon}, <<"warframe">> => #{<<"build">> => Frame},
        <<"melee">> => #{<<"build">> => Melee}}}.

inline(Type, Class, Slots) ->
    #{<<"definition_id">> => Type, <<"class">> => Class, <<"name">> => filename:basename(Type),
      <<"config">> => #{<<"upgrade_slots">> => Slots}}.

calc(Options, Equipment) -> wfcli_build_stats:calculate(Options, Equipment, [weapon()]).
options() -> #{<<"target">> => <<"weapon">>, <<"buildset">> => #{
    <<"weapon">> => #{<<"item">> => <<"Vasto Prime">>},
    <<"warframe">> => #{<<"item">> => <<"Mesa Prime">>}}}.
unmodded() -> #{<<"target">> => <<"weapon">>, <<"buildset">> => #{
    <<"weapon">> => #{<<"item">> => <<"Vasto Prime">>, <<"unmodded">> => true}}}.
member_options(Options, Slot, Changes) ->
    Builds = maps:get(<<"buildset">>, Options),
    Options#{<<"buildset">> => Builds#{Slot => maps:merge(maps:get(Slot, Builds, #{}), Changes)}}.
near(Expected, Actual) -> ?assert(abs(Expected - Actual) < 0.001).

weapon() ->
    #{<<"uniqueName">> => ?VASTO, <<"name">> => <<"Vasto Prime">>,
      <<"productCategory">> => <<"Pistols">>,
      <<"damagePerShot">> => [16.5, 16.5, 77] ++ lists:duplicate(17, 0),
      <<"totalDamage">> => 110, <<"criticalChance">> => 0.22,
      <<"criticalMultiplier">> => 2.4000001, <<"procChance">> => 0.22000003,
      <<"fireRate">> => 2.5000002, <<"magazineSize">> => 6, <<"reloadTime">> => 1,
      <<"multishot">> => 1, <<"trigger">> => <<"SEMI">>}.

equipment() ->
    Prefix = <<"/Lotus/Upgrades/Mods/Pistol/">>,
    WeaponSlots = [slot(N, <<Prefix/binary, Path/binary>>, Rank)
        || {N, Path, Rank} <- [
            {1, <<"Expert/WeaponCritChanceModBeginnerExpert">>, 10},
            {2, <<"Expert/PrimedWeaponCritDamageMod">>, 10},
            {3, <<"WeaponFireIterationsSPMod">>, 10},
            {4, <<"WeaponStatusChanceSPMod">>, 10},
            {5, <<"DualStat/PoisonEventPistolMod">>, 3},
            {6, <<"Expert/PrimedWeaponElectricityDamageMod">>, 7},
            {7, <<"Expert/WeaponFireDamageModExpert">>, 10},
            {8, <<"PistolStatusDamageDuoMod">>, 5}]],
    FrameSlots = [
        slot(1, <<"/Lotus/Upgrades/Mods/Warframe/DualStat/CorruptedPowerEfficiencyWarframe">>, 10),
        slot(2, <<"/Lotus/Upgrades/Mods/Sets/Umbra/WarframeUmbraModC">>, 10),
        (slot(3, <<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/GolemArcanePistolSpeedOnCrit">>, 5))#{
            <<"topology_slot">> => <<"arcane-1">>}],
    #{<<"player_revision">> => 101, <<"player_updated_at">> => 123456789,
      <<"definitions">> => [#{<<"id">> => ?VASTO, <<"name">> => <<"Vasto Prime">>},
                              #{<<"id">> => ?MESA, <<"name">> => <<"Mesa Prime">>}],
      <<"instances">> => [instance(<<"weapon-1">>, ?VASTO, <<"secondary">>, WeaponSlots),
                            instance(<<"frame-1">>, ?MESA, <<"warframe">>, FrameSlots)]}.

instance(Id, Type, Class, Slots) ->
    #{<<"instance_id">> => Id, <<"definition_id">> => Type,
      <<"class">> => Class,
      <<"active_config_indices">> => [0],
      <<"topology">> => maps:get(<<"topology">>, wfcli_build_topology:definition(Class, #{})),
      <<"configs">> => [#{<<"config_index">> => 0, <<"fingerprint">> => <<"fixture">>,
                           <<"upgrade_slots">> => Slots}]}.

slot(N, Type, Rank) ->
    #{<<"topology_slot">> => <<"mod-", (integer_to_binary(N))/binary>>,
      <<"item_type">> => Type, <<"name">> => filename:basename(Type), <<"rank">> => Rank}.

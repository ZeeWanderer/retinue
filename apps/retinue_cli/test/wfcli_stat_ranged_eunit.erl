-module(wfcli_stat_ranged_eunit).

-include_lib("eunit/include/eunit.hrl").

reviewed_profiles_use_per_projectile_packets_test() ->
    lists:foreach(fun(Export) ->
        Name = maps:get(<<"name">>, Export),
        {ok, Result} = calculate(Name, [], #{}),
        Stats = maps:get(stats, Result),
        Mean = lists:sum([P * D || #{probability := P, raw_damage := D}
                                      <- maps:get(projectile_outcomes, Stats)]),
        near(Mean * maps:get(<<"multishot">>, Export),
             maps:get(expected_raw_damage_per_trigger, Stats)),
        near(maps:get(<<"procChance">>, Export), maps:get(status_chance, Stats)),
        case maps:get(<<"multishot">>, Export) > 1 of
            true -> grouping_gap(Result);
            false -> ?assertEqual(ready, maps:get(status, Result))
        end,
        ?assertEqual(hitscan, maps:get(kind, maps:get(profile, Result)))
    end, exports()).

native_profile_packets_test_() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/native_profile_packets.json"),
    #{<<"cases">> := Cases} = jsone:decode(Json),
    Catalog = [wfcli_build_stats_eunit:weapon() | exports()],
    [{binary_to_list(Type), ?_test(begin
        Options = #{<<"target">> => <<"weapon">>, <<"buildset">> => #{
            <<"weapon">> => #{<<"item">> => Type, <<"unmodded">> => true}}},
        {ok, Result} = wfcli_build_stats:calculate(Options, #{}, Catalog),
        Stats = maps:get(stats, Result),
        Packet = maps:get(source_packet, Stats),
        Weights = native_channels(maps:get(<<"weights">>, Case)),
        ?assertEqual(float32(maps:get(<<"scalar">>, Case)), maps:get(scalar, Packet)),
        ?assertEqual(Weights, maps:get(channels, Packet)),
        ?assertEqual(native_channels(maps:get(<<"hit_weights">>, Case)),
            wfcli_stat_math:hit_channels(Weights, maps:get(quantization, Packet))),
        ?assertEqual(float32(maps:get(<<"hit_critical_multiplier">>, Case)),
                     maps:get(critical_multiplier, Stats)),
        ?assert(is_binary(jsone:encode(Result)))
    end)} || Case = #{<<"definition_id">> := Type} <- Cases].

profile_damage_weight_drift_test() ->
    Export = export(<<"Karak">>),
    Profile = wfcli_stat_profiles:weapon(maps:get(<<"uniqueName">>, Export)),
    [Impact, Puncture | Rest] = maps:get(<<"damagePerShot">>, Export),
    Changed = Export#{<<"damagePerShot">> => [Impact + 1, Puncture - 1 | Rest]},
    ?assertThrow({stats, changed_attack_damage_weights}, wfcli_stat_profiles:base(Changed, Profile)),
    Scaled = Export#{<<"totalDamage">> => 58,
        <<"damagePerShot">> => [2 * D || D <- maps:get(<<"damagePerShot">>, Export)]},
    Base = wfcli_stat_profiles:base(Scaled, Profile),
    ?assertEqual(58, maps:get(scalar, maps:get(packet, Base))),
    ?assertEqual(maps:get(channels, maps:get(packet, wfcli_stat_profiles:base(Export, Profile))),
                 maps:get(channels, maps:get(packet, Base))).

unrepresentable_critical_chance_returns_error_test() ->
    Request = request(<<"Karak">>, mods(<<"Rifle">>, [{<<"WeaponCritChanceMod">>, 5}])),
    Export = (export(<<"Karak">>))#{<<"criticalChance">> => 1000000000},
    ?assertMatch({error, {unsupported_critical_chance, _}},
                 wfcli_build_stats:calculate(Request, #{}, [Export])).

karak_public_health_outcomes_test() ->
    {ok, Result} = calculate(<<"Karak">>,
        mods(<<"Rifle">>, [{<<"WeaponDamageAmountMod">>, 8}]), #{}),
    Stats = maps:get(stats, Result),
    ?assertEqual([68, 102], [D || #{health_damage := D} <- maps:get(projectile_outcomes, Stats)]),
    ?assertEqual(ready, maps:get(status, Result)),
    ?assertEqual(11, maps:get(revision, maps:get(provenance, Result))).

public_armor_channel_floor_test() ->
    {ok, Result} = calculate(<<"Karak">>, [], #{<<"armor">> => 10000000}),
    Stats = maps:get(stats, Result),
    ?assertEqual([3, 3], [D || #{health_damage := D} <- maps:get(projectile_outcomes, Stats)]),
    near(3, maps:get(expected_target_damage_per_trigger, Stats)),
    ?assertEqual(ready, maps:get(status, Result)).

native_channels(Channels) ->
    maps:from_list([{binary_to_existing_atom(K), float32(V)} || {K, V} <- maps:to_list(Channels)]).
float32(V) ->
    <<Rounded:32/float>> = <<(float(V)):32/float>>,
    Rounded.

hek_multishot_and_element_damage_test() ->
    Mods = mods(<<"Shotgun">>, [{<<"WeaponFreezeDamageMod">>, 0},
        {<<"WeaponDamageAmountMod">>, 5}, {<<"WeaponFireIterationsMod">>, 5}]),
    {ok, Result} = calculate(<<"Hek">>, Mods, #{}),
    Stats = maps:get(stats, Result),
    near(7 * 2.2, maps:get(multishot, Stats)),
    near(75 * 0.15 * 1.9, maps:get(cold, maps:get(damage_per_projectile, Stats))),
    near(maps:get(expected_raw_damage_per_projectile, Stats) * 7 * 2.2,
         maps:get(expected_raw_damage_per_trigger, Stats)),
    grouping_gap(Result).

compound_mods_do_not_pair_test() ->
    Mods = mods(<<"Rifle">>, [{<<"DualStat/RadiationReloadRifleMod">>, 0},
        {<<"DualStat/PoisonEventRifleMod">>, 3}, {<<"WeaponFreezeDamageMod">>, 5}]),
    {ok, Result} = calculate(<<"Karak">>, Mods, #{}),
    Stats = maps:get(stats, Result),
    Damage = maps:get(damage_per_projectile, Stats),
    near(29 * 0.15, maps:get(radiation, Damage)),
    near(29 * 1.5, maps:get(viral, Damage)),
    ?assertNot(maps:is_key(toxin, Damage)),
    near(2 / 1.1, maps:get(reload_time, Stats)).

galvanized_family_caps_test() ->
    lists:foreach(fun({Name, Family, Id, Base, Step, Cap, Pellets}) ->
        Mods = mods(Family, [{<<"WeaponFireIterationsSPMod">>, 10}]),
        {ok, Result} = calculate(Name, Mods, #{<<"bounds">> => true}),
        Bounds = maps:get(bounds, Result),
        near(Pellets * (1 + Base * 11), maps:get(multishot, maps:get(lower, Bounds))),
        near(Pellets * (1 + Base * 11 + Step * 11 * Cap),
             maps:get(multishot, maps:get(upper, Bounds))),
        ?assertMatch({error, {invalid_effect_stacks, _}},
            calculate(Name, Mods, #{<<"effects">> => #{<<"weapon/", Id/binary>> => Cap + 1}}))
    end, [{<<"Braton">>, <<"Rifle">>, <<"galvanized-chamber">>, 0.072729997, 0.02727, 5, 1},
          {<<"Hek">>, <<"Shotgun">>, <<"galvanized-hell">>, 0.1, 0.02727, 4, 7},
          {<<"Akvasto Prime">>, <<"Pistol">>, <<"galvanized-diffusion">>, 0.1, 0.027272699, 4, 1}]).

galvanized_aptitude_keeps_damage_gap_explicit_test() ->
    {ok, Base} = calculate(<<"Karak">>, [], #{}),
    {ok, Result} = calculate(<<"Karak">>,
        mods(<<"Rifle">>, [{<<"WeaponStatusChanceSPMod">>, 8}]), #{}),
    Stats = maps:get(stats, Result),
    near(0.15 * (1 + 9 * 0.072727002), maps:get(status_chance, Stats)),
    ?assertEqual(maps:get(damage_per_projectile, maps:get(stats, Base)),
                 maps:get(damage_per_projectile, Stats)),
    ?assertEqual(partial, maps:get(status, Result)),
    ?assertMatch([#{reason := <<"Galvanized per-status conditional damage is not evaluated">>}],
                 maps:get(excluded, Result)).

wrong_family_does_not_apply_test() ->
    {ok, Base} = calculate(<<"Hek">>, [], #{}),
    {ok, Wrong} = calculate(<<"Hek">>, mods(<<"Rifle">>, [{<<"WeaponDamageAmountMod">>, 10}]), #{}),
    ?assertEqual(maps:get(stats, Base), maps:get(stats, Wrong)),
    ?assertEqual(partial, maps:get(status, Wrong)),
    ?assert(lists:member(<<"Upgrade is incompatible with the weapon family">>, reasons(Wrong))).

primary_crux_bounds_test() ->
    {ok, Result} = calculate(<<"Boar Prime">>, [crux(5)], #{<<"bounds">> => true}),
    Bounds = maps:get(bounds, Result),
    Low = maps:get(lower, Bounds),
    High = maps:get(upper, Bounds),
    near(1, maps:get(ammo_per_trigger, Low)),
    near(0.4, maps:get(ammo_per_trigger, High)),
    near(maps:get(status_chance, Low) * 4, maps:get(status_chance, High)),
    near(maps:get(expected_raw_damage_per_trigger, Low),
         maps:get(expected_raw_damage_per_trigger, High)),
    ?assert(maps:get(theoretical_sustained_dps, High) > maps:get(theoretical_sustained_dps, Low)),
    grouping_gap(Result),
    {ok, Selected} = calculate(<<"Boar Prime">>, [crux(0)],
        #{<<"effects">> => #{<<"weapon/primary-crux">> => 3}}),
    near(0.97, maps:get(ammo_per_trigger, maps:get(stats, Selected))),
    ?assertEqual({error, {invalid_effect_stacks, <<"weapon/primary-crux">>}},
        calculate(<<"Boar Prime">>, [crux(5)],
            #{<<"effects">> => #{<<"weapon/primary-crux">> => 11}})),
    {ok, Invalid} = calculate(<<"Boar Prime">>, [crux(6)], #{}),
    ?assertEqual(partial, maps:get(status, Invalid)).

primary_crux_does_not_affect_secondary_test() ->
    {ok, Base} = calculate(<<"Akvasto Prime">>, [], #{}),
    Options = request(<<"Akvasto Prime">>, []),
    #{<<"weapon">> := Primary} = maps:get(<<"buildset">>, request(<<"Boar Prime">>, [crux(5)])),
    {ok, Context} = wfcli_build_stats:calculate(Options#{<<"buildset">> =>
        (maps:get(<<"buildset">>, Options))#{<<"primary">> => Primary},
        <<"bounds">> => true}, #{}, exports()),
    ?assertEqual(maps:get(stats, Base), maps:get(upper, maps:get(bounds, Context))),
    {ok, Wrong} = calculate(<<"Akvasto Prime">>, [crux(5)], #{}),
    ?assertEqual(maps:get(stats, Base), maps:get(stats, Wrong)),
    ?assertEqual(partial, maps:get(status, Wrong)),
    ?assertMatch([#{reason := <<"Upgrade is incompatible with the equipment class">>}],
                 maps:get(excluded, Wrong)).

ordinary_rifle_does_not_get_bow_only_rate_test() ->
    {ok, Result} = calculate(<<"Braton">>, mods(<<"Rifle">>, [{<<"WeaponFireRateMod">>, 5}]), #{}),
    near(8.75 * 1.6, maps:get(fire_rate, maps:get(stats, Result))),
    {ok, Corrupted} = calculate(<<"Braton">>,
        mods(<<"Rifle">>, [{<<"DualStat/CorruptedCritRateFireRateRifle">>, 5}]), #{}),
    near(8.75 * (1 - 0.033300001 * 6), maps:get(fire_rate, maps:get(stats, Corrupted))).

mesa_passive_distinguishes_single_dual_and_primary_test() ->
    [_, Frame] = maps:get(<<"instances">>, wfcli_build_stats_eunit:equipment()),
    Context = #{<<"warframe">> => #{<<"build">> => maps:without([<<"instance_id">>, <<"configs">>],
        Frame#{<<"config">> => #{<<"upgrade_slots">> => []}})}},
    lists:foreach(fun({Name, Rate, Reload}) ->
        Options = request(Name, []),
        Builds = maps:merge(maps:get(<<"buildset">>, Options), Context),
        {ok, Result} = wfcli_build_stats:calculate(Options#{<<"buildset">> => Builds}, #{}, exports()),
        Stats = maps:get(stats, Result),
        near(Rate, maps:get(fire_rate, Stats)),
        near(Reload, maps:get(reload_time, Stats))
    end, [{<<"Vasto">>, 2.5000002, 1 / 1.25},
          {<<"Akvasto Prime">>, 3.3333335 * 1.15, 1.4},
          {<<"Hek">>, 2.1666667, 2}]).

changed_and_unknown_attack_profiles_test() ->
    Options = request(<<"Hek">>, []),
    Export = export(<<"Hek">>),
    lists:foreach(fun({Changes, Error}) ->
        ?assertEqual({error, Error},
            wfcli_build_stats:calculate(Options, #{}, [maps:merge(Export, Changes)]))
    end, [{#{<<"productCategory">> => <<"Pistols">>}, changed_attack_class},
          {#{<<"trigger">> => <<"BURST">>}, changed_attack_profile},
          {#{<<"multishot">> => 8}, changed_attack_projectiles},
          {#{<<"damagePerShot">> => [11, 49, 14, 1] ++ lists:duplicate(16, 0)},
            changed_attack_damage_channels}]),
    Unknown = Export#{<<"uniqueName">> => <<"/Unreviewed/SemiAuto">>},
    UnknownOptions = #{<<"target">> => <<"weapon">>, <<"buildset">> => #{
        <<"weapon">> => #{<<"item">> => <<"/Unreviewed/SemiAuto">>, <<"unmodded">> => true}}},
    ?assertEqual({error, {unsupported_attack_profile, <<"/Unreviewed/SemiAuto">>}},
                 wfcli_build_stats:calculate(UnknownOptions, #{}, [Unknown])).

unreviewed_evolutions_still_mark_partial_test() ->
    Options = request(<<"Braton Prime">>, []),
    #{<<"weapon">> := #{<<"build">> := Build}} = maps:get(<<"buildset">>, Options),
    {ok, Result} = wfcli_build_stats:calculate(Options#{<<"buildset">> => #{<<"weapon">> =>
        #{<<"build">> => Build#{<<"skill_tree">> => <<"0000">>}}}}, #{}, exports()),
    ?assertEqual(partial, maps:get(status, Result)),
    ?assertMatch([#{reason := <<"Incarnon evolutions are not reviewed">>}], maps:get(excluded, Result)).

boar_evolutions_share_scalar_engine_test() ->
    lists:foreach(fun(Name) ->
        {ok, Base} = calculate(Name, [], #{}),
        {ok, Result} = evolution(Name, <<"0001">>, #{}),
        Low = maps:get(lower, maps:get(bounds, Result)),
        High = maps:get(upper, maps:get(bounds, Result)),
        Original = maps:get(stats, Base),
        Total = lists:sum(maps:values(maps:get(damage_per_projectile, Original))),
        near(Total + 10, lists:sum(maps:values(maps:get(damage_per_projectile, Low)))),
        near(Total + 24, lists:sum(maps:values(maps:get(damage_per_projectile, High)))),
        near(maps:get(critical_chance, Original) + 0.2, maps:get(critical_chance, Low)),
        Crit = case Name of <<"Boar">> -> 2.000488519668579; _ -> 2.500610589981079 end,
        near(Crit, maps:get(critical_multiplier, Low)),
        near(maps:get(reload_time, Original), maps:get(reload_time, Low)),
        near(maps:get(reload_time, Original) / 2, maps:get(reload_time, High)),
        grouping_gap(Result),
        Deps = maps:get(dependencies, maps:get(provenance, Result)),
        ?assert(lists:member(<<"calculator.evolutions">>, maps:get(mechanics, Deps)))
    end, [<<"Boar">>, <<"Boar Prime">>]),
    {ok, Selected} = evolution(<<"Boar Prime">>, <<"0001">>,
        #{<<"effects">> => #{<<"weapon/boar-empty-reload-damage">> => 1,
                             <<"weapon/boar-empty-reload-speed">> => 1}}),
    ?assertEqual(maps:get(upper, maps:get(bounds, Selected)), maps:get(stats, Selected)),
    {ok, Other} = evolution(<<"Boar Prime">>, <<"0121">>, #{}),
    near(56, lists:sum(maps:values(maps:get(damage_per_projectile, maps:get(stats, Other))))),
    grouping_gap(Other).

boar_status_evolutions_use_base_pellets_test() ->
    lists:foreach(fun(Name) ->
        Export = export(Name),
        lists:foreach(fun({Tree, Bonus, CritBonus}) ->
            {ok, Plain} = evolution(Name, Tree, #{}),
            PlainStats = maps:get(stats, Plain),
            Expected = maps:get(<<"procChance">>, Export) + Bonus,
            near(Expected, maps:get(status_chance, PlainStats)),
            near(maps:get(<<"criticalChance">>, Export) + CritBonus,
                 maps:get(critical_chance, PlainStats)),
            grouping_gap(Plain),
            {ok, Modded} = evolution(Name, Tree,
                mods(<<"Shotgun">>, [{<<"DualStat/PoisonEventShotgunMod">>, 3},
                                     {<<"WeaponFireIterationsMod">>, 5}]) ++ [crux(5)], #{}),
            Stats = maps:get(stats, Modded),
            Upper = maps:get(upper, maps:get(bounds, Modded)),
            near(17.6, maps:get(multishot, Stats)),
            near(Expected * 1.6, maps:get(status_chance, Stats)),
            near(Expected * 4.6, maps:get(status_chance, Upper)),
            grouping_gap(Modded)
        end, [{<<"0000">>, 0.12, 0}, {<<"0002">>, 0.06, 0.1}])
    end, [<<"Boar">>, <<"Boar Prime">>]).

evolution(Name, Tree, Extra) ->
    evolution(Name, Tree, [], Extra).

evolution(Name, Tree, Mods, Extra) ->
    Options = request(Name, Mods),
    #{<<"weapon">> := #{<<"build">> := Build}} = maps:get(<<"buildset">>, Options),
    wfcli_build_stats:calculate(maps:merge(Options#{<<"bounds">> => true,
        <<"buildset">> => #{<<"weapon">> => #{<<"build">> => Build#{<<"skill_tree">> => Tree}}}}, Extra),
        #{}, exports()).

calculate(Name, Mods, Extra) ->
    wfcli_build_stats:calculate(maps:merge(request(Name, Mods), Extra), #{}, exports()).
request(Name, Mods) ->
    Export = export(Name),
    Class = case maps:get(<<"productCategory">>, Export) of
        <<"Pistols">> -> <<"secondary">>;
        _ -> <<"primary">>
    end,
    #{<<"target">> => <<"weapon">>, <<"buildset">> => #{<<"weapon">> => #{<<"build">> =>
        #{<<"definition_id">> => maps:get(<<"uniqueName">>, Export), <<"class">> => Class,
          <<"config">> => #{<<"upgrade_slots">> => Mods}}}}}.
mods(Family, Mods) ->
    [#{<<"topology_slot">> => <<"mod-", (integer_to_binary(N))/binary>>,
       <<"item_type">> => <<"/Lotus/Upgrades/Mods/", Family/binary, "/", Key/binary>>,
       <<"rank">> => Rank} || {N, {Key, Rank}} <- lists:enumerate(Mods)].
crux(Rank) ->
    #{<<"topology_slot">> => <<"arcane-1">>, <<"rank">> => Rank,
      <<"item_type">> =>
          <<"/Lotus/Upgrades/CosmeticEnhancers/Offensive/StatusAndAmmoEfficiencyOnWeakpointHit">>}.
export(Name) ->
    [Export] = [E || E <- exports(), maps:get(<<"name">>, E) =:= Name],
    Export.
exports() ->
    {ok, Json} = file:read_file("apps/retinue_cli/test/fixtures/ranged_stats_exports.json"),
    jsone:decode(Json).
near(Expected, Actual) -> ?assert(abs(Expected - Actual) < 0.001).

reasons(Result) -> [R || #{reason := R} <- maps:get(excluded, Result)].
grouping_gap(Result) ->
    ?assertEqual(partial, maps:get(status, Result)),
    ?assert(lists:member(
        <<"Multi-projectile totals assume separate health packets; native grouping is not verified">>,
        maps:get(excluded, Result))).

-module(wfcli_stat_frames_eunit).

-include_lib("eunit/include/eunit.hrl").

-export([exports/0, request/3]).

-define(PRIME, <<"/Lotus/Powersuits/AntiMatter/Abilities/MolecularPrimeAbility">>).
-define(NOURISH, <<"/Lotus/Powersuits/Devourer/Abilities/DevourerConsumeAbility">>).

reviewed_profiles_use_export_base_and_rank_additions_test() ->
    lists:foreach(fun(Export) ->
        Name = maps:get(<<"name">>, Export),
        {ok, Result} = calculate(Name, 30, []),
        Base = maps:get(base, Result),
        ?assertEqual(maps:get(<<"health">>, Export), maps:get(health, Base)),
        near(maps:get(<<"shield">>, Export) + 100, stat(shield, Result)),
        near(maps:get(<<"power">>, Export) + 50, stat(energy, Result)),
        near(maps:get(<<"armor">>, Export), stat(armor, Result)),
        ?assert(is_binary(jsone:encode(Result))),
        ?assertEqual(warframe, maps:get(kind, Result)),
        ?assertNot(maps:is_key(theoretical_burst_dps, maps:get(stats, Result)))
    end, exports()).

rank_additions_precede_mod_scaling_test() ->
    Mods = [mod(1, <<"AvatarHealthMaxMod">>, 10), mod(2, <<"AvatarShieldMaxMod">>, 10),
            mod(3, <<"AvatarArmourMod">>, 10), mod(4, <<"Expert/AvatarPowerMaxModExpert">>, 10),
            mod(5, <<"AvatarSprintSpeedMod">>, 5)],
    {ok, Result} = calculate(<<"Excalibur">>, 30, Mods),
    near(740, stat(health, Result)),
    near(740, stat(shield, Result)),
    near(480, stat(armor, Result)),
    near(427.5, stat(energy, Result)),
    near(1.3, stat(sprint_speed, Result)),
    near(1200, stat(overshield_capacity, Result)),
    {ok, Rank0} = calculate(<<"Excalibur">>, 0, Mods),
    near(540, stat(health, Rank0)),
    near(285, stat(energy, Rank0)).

intermediate_rank_schedule_test() ->
    lists:foreach(fun({Rank, Health, Shield, Energy}) ->
        {ok, Result} = calculate(<<"Excalibur">>, Rank, []),
        near(Health, stat(health, Result)), near(Shield, stat(shield, Result)), near(Energy, stat(energy, Result))
    end, [{0,270,270,100}, {1,280,270,100}, {2,280,280,100},
          {3,280,280,105}, {29,370,370,145}, {30,370,370,150}]).

mesa_passive_health_is_not_multiplied_by_vitality_test() ->
    Request = request(<<"Mesa Prime">>, 30, [mod(1, <<"AvatarHealthMaxMod">>, 10)]),
    {ok, Alone} = wfcli_build_stats:calculate(Request, #{}, exports()),
    near(1050, stat(health, Alone)),
    Builds = maps:get(<<"buildset">>, Request),
    Dagger = #{<<"definition_id">> => <<"/Lotus/Weapons/Tenno/Melee/Dagger/CeramicDagger">>,
               <<"class">> => <<"melee">>, <<"config">> => #{<<"upgrade_slots">> => []}},
    {ok, WithMelee} = wfcli_build_stats:calculate(Request#{<<"buildset">> =>
        Builds#{<<"melee">> => #{<<"build">> => Dagger}}}, #{}, exports()),
    near(1000, stat(health, WithMelee)).

nova_preview_uses_independent_scaling_axes_test() ->
    {ok, Base} = calculate(<<"Nova Prime">>, 30, []),
    Outputs = ability_outputs(Base),
    ?assertEqual(#{slow => 0.5, speed => 1.5}, maps:get(variants, maps:get(crowd_control, Outputs))),
    near(35, value(wave_radius, Outputs)),
    near(10, value(explosion_radius, Outputs)),
    near(800, value(explosion, Outputs)),
    Mods = [mod(1, <<"AvatarAbilityDurationMod">>, 5), mod(2, <<"AvatarAbilityRangeMod">>, 5),
            mod(3, <<"DualStat/CorruptedPowerEfficiencyWarframe">>, 10)],
    {ok, Modified} = calculate(<<"Nova Prime">>, 30, Mods),
    New = ability_outputs(Modified),
    ?assertEqual(#{slow => 0.25, speed => 1.75}, maps:get(variants, maps:get(crowd_control, New))),
    near(44, value(wave_radius, New)),
    near(14.5, value(explosion_radius, New)),
    near(1592, value(explosion, New)),
    near(39, value(effect_duration, New)),
    near(2, value(damage_vulnerability, New)).

nova_rank_and_strength_do_not_select_cast_variant_test() ->
    Rule = wfcli_stat_abilities:definition(?PRIME, native),
    Multipliers = (wfcli_stat_frames:multipliers([]))#{ability_strength => 0.4},
    Low = maps:get(outputs, wfcli_stat_abilities:evaluate(Rule, 0, Multipliers, 0)),
    #{slow := Slow, speed := Speed} = maps:get(variants, maps:get(crowd_control, Low)),
    near(0.88, Slow), near(1.12, Speed),
    near(60, value(explosion, Low)),
    near(15, value(wave_radius, Low)).

overridden_native_ability_is_not_evaluated_twice_test() ->
    Request = change_config(request(<<"Nova">>, 30, []),
        #{<<"ability_override">> => #{<<"Ability">> => ?NOURISH, <<"Index">> => 3}}),
    {ok, Result} = wfcli_build_stats:calculate(Request, #{}, exports()),
    [Ability] = maps:get(selected, maps:get(abilities, Result)),
    ?assertEqual(?NOURISH, maps:get(item_type, Ability)),
    near(0.45, value(weapon_damage_bonus, maps:get(outputs, Ability))),
    ?assertEqual(0, maps:get(active_stacks, Ability)),
    ?assertEqual([], [N || N = #{item_type := ?PRIME} <- maps:get(effect_graph, Result)]).

unknowns_are_not_zero_or_fabricated_profiles_test() ->
    Request = request(<<"Excalibur">>, 30, []),
    #{<<"frame">> := #{<<"build">> := Item}} = maps:get(<<"buildset">>, Request),
    UnknownRank = Request#{<<"buildset">> => #{<<"frame">> => #{<<"build">> => maps:remove(<<"rank">>, Item)}}},
    ?assertMatch({error, {frame_rank_unknown, _}}, wfcli_build_stats:calculate(UnknownRank, #{}, exports())),
    UnknownType = Item#{<<"definition_id">> => <<"/unreviewed">>},
    ?assertMatch({error, {unsupported_frame_profile, _}}, wfcli_build_stats:calculate(
        Request#{<<"buildset">> => #{<<"frame">> => #{<<"build">> => UnknownType}}}, #{}, exports())),
    ?assertMatch({error, {frame_stat_unavailable, <<"health">>}}, wfcli_build_stats:calculate(
        Request, #{}, [maps:remove(<<"health">>, E) || E <- exports()])),
    ?assertEqual({error, armor_requires_weapon_target},
        wfcli_build_stats:calculate(Request#{<<"armor">> => 2700}, #{}, exports())),
    ?assertEqual(unsupported, wfcli_stat_frames:upgrade(<<"/Lotus/Upgrades/Mods/Warframe/FutureMod">>)).

rank_provenance_and_class_filters_test() ->
    Request = request(<<"Mesa Prime">>, 30, []),
    #{<<"frame">> := #{<<"build">> := Item}} = maps:get(<<"buildset">>, Request),
    XpItem = (maps:remove(<<"rank">>, Item))#{<<"xp">> => 900000},
    {ok, Result} = wfcli_build_stats:calculate(Request#{<<"buildset">> =>
        #{<<"frame">> => #{<<"build">> => XpItem}}}, #{}, exports()),
    ?assertEqual(max_rank_equipment, maps:get(rank_source, maps:get(base, Result))),
    lists:foreach(fun(Rank) ->
        ?assertEqual({error, invalid_stat_request}, wfcli_build_stats:validate(Request#{<<"buildset">> =>
            #{<<"frame">> => #{<<"build">> => Item#{<<"rank">> => Rank}}}}))
    end, [-1, 31, 3.5]),
    Weapon = wfcli_build_stats_eunit:weapon(),
    Builds = #{<<"weapon">> => #{<<"build">> => #{<<"definition_id">> => maps:get(<<"uniqueName">>, Weapon),
        <<"class">> => <<"secondary">>, <<"config">> => #{<<"upgrade_slots">> =>
            [mod(1, <<"AvatarAbilityStrengthMod">>, 5)]}}}},
    {ok, Rejected} = wfcli_build_stats:calculate(#{<<"target">> => <<"weapon">>, <<"buildset">> => Builds},
                                                #{}, [Weapon]),
    ?assertMatch([#{reason := <<"Upgrade is incompatible with the equipment class">>}],
                  maps:get(excluded, Rejected)).

malformed_catalog_ability_is_explicit_gap_test() ->
    Catalog = [E#{<<"abilities">> => [#{<<"unexpected">> => true}]} || E <- exports()],
    {ok, Result} = wfcli_build_stats:calculate(request(<<"Nova">>, 30, []), #{}, Catalog),
    ?assertEqual([], maps:get(selected, maps:get(abilities, Result))),
    ?assert(lists:any(fun(#{reason := Reason}) -> Reason =:= <<"Unrecognized ability catalog entry">>;
                        (_) -> false end, maps:get(excluded, Result))).

catalog_and_cli_are_frame_aware_test() ->
    {ok, #{options := Request}} = wfcli_build_stats_cli:request(#{item => "Nova", unmodded => true, bounds => true}),
    {ok, Result} = wfcli_build_stats:calculate(Request, #{}, exports()),
    Text = iolist_to_binary(wfcli_build_stats_cli:format(Result)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Warframe stat estimate">>)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Molecular Prime">>)),
    ?assertEqual(nomatch, binary:match(Text, <<"DPS">>)),
    ?assertEqual(maps:get(stats, Result), maps:get(upper, maps:get(bounds, Result))),
    Dependencies = maps:get(dependencies, maps:get(provenance, Result)),
    ?assert(lists:member(<<"calculator.warframe.core">>, maps:get(mechanics, Dependencies))),
    ?assert(lists:member(<<"calculator.molecular-prime">>, maps:get(mechanics, Dependencies))),
    ?assertNot(lists:member(<<"calculator.direct-fire.core">>, maps:get(mechanics, Dependencies))).

change_config(Request, Changes) ->
    #{<<"frame">> := #{<<"build">> := Build}} = maps:get(<<"buildset">>, Request),
    Request#{<<"buildset">> => #{<<"frame">> => #{<<"build">> => Build#{<<"config">> =>
        maps:merge(maps:get(<<"config">>, Build), Changes)}}}}.
calculate(Name, Rank, Mods) -> wfcli_build_stats:calculate(request(Name, Rank, Mods), #{}, exports()).
request(Name, Rank, Mods) ->
    [Export] = [E || E <- exports(), maps:get(<<"name">>, E) =:= Name],
    #{<<"target">> => <<"frame">>, <<"buildset">> => #{<<"frame">> => #{<<"build">> => #{
        <<"definition_id">> => maps:get(<<"uniqueName">>, Export), <<"class">> => <<"warframe">>,
        <<"rank">> => Rank, <<"config">> => #{<<"upgrade_slots">> => Mods}}}}}.
exports() ->
    {ok, Data} = file:read_file("apps/retinue_cli/test/fixtures/frame_stats_exports.json"),
    jsone:decode(Data).
mod(N, Key, Rank) -> #{<<"topology_slot">> => <<"mod-", (integer_to_binary(N))/binary>>,
    <<"item_type">> => <<"/Lotus/Upgrades/Mods/Warframe/", Key/binary>>, <<"rank">> => Rank}.
stat(Key, Result) -> maps:get(Key, maps:get(stats, Result)).
ability_outputs(Result) ->
    [Ability] = maps:get(selected, maps:get(abilities, Result)),
    maps:get(outputs, Ability).
value(Key, Outputs) -> maps:get(value, maps:get(Key, Outputs)).
near(Expected, Actual) -> ?assert(abs(Expected - Actual) < 0.02).

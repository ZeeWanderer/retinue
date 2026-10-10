-module(wfcli_stat_abilities_eunit).

-include_lib("eunit/include/eunit.hrl").

-define(NOURISH, <<"/Lotus/Powersuits/Devourer/Abilities/DevourerConsumeAbility">>).
-define(MESA, <<"/Lotus/Powersuits/Cowgirl/MesaPrime">>).

nourish_requires_activation_test() ->
    {ok, Result} = calculate(frame([]), [], #{<<"bounds">> => true}),
    Bounds = maps:get(bounds, Result),
    Inactive = maps:get(inactive, Bounds),
    ?assertEqual(Inactive, maps:get(stats, Result)),
    ?assertEqual(Inactive, maps:get(lower, Bounds)),
    ?assertNot(maps:is_key(viral, maps:get(damage_per_projectile, Inactive))),
    near(110 * 0.45, damage(viral, maps:get(upper, Bounds))),
    {ok, Active} = calculate(frame([]), [], active()),
    ?assertEqual(maps:get(upper, Bounds), maps:get(stats, Active)),
    ?assertMatch({error, {invalid_effect_stacks, <<"frame/nourish">>}},
        calculate(frame([]), [], #{<<"effects">> => #{<<"frame/nourish">> => 2}})).

ability_scaling_depends_on_source_modifiers_test() ->
    Mods = [mod(1, <<"Warframe/DualStat/CorruptedPowerEfficiencyWarframe">>, 10),
            mod(2, <<"Warframe/AvatarAbilityDurationMod">>, 5)],
    {ok, Result} = calculate(frame(Mods), [], active()),
    near(110 * 0.45 * 1.99, damage(viral, maps:get(stats, Result))),
    Node = ability(Result),
    ?assertEqual([<<"frame/mod-1">>, <<"frame/mod-2">>], maps:get(depends_on, Node)),
    State = maps:get(selected, maps:get(states, Node)),
    near(1.99, maps:get(ability_strength, State)),
    near(25 * 1.3, maps:get(duration, State)),
    ?assert(lists:all(fun(#{target := Target}) -> Target =:= <<"weapon">> end,
                     maps:get(effects, State))).

source_name_order_does_not_change_scaling_test() ->
    Frame = frame([mod(1, <<"Warframe/DualStat/CorruptedPowerEfficiencyWarframe">>, 10)]),
    Request = request(Frame, []),
    Builds = maps:get(<<"buildset">>, Request),
    Renamed = (maps:remove(<<"frame">>, Builds))#{<<"zframe">> => maps:get(<<"frame">>, Builds)},
    {ok, Result} = wfcli_build_stats:calculate(Request#{<<"buildset">> => Renamed,
        <<"effects">> => #{<<"zframe/nourish">> => 1}}, #{}, [wfcli_build_stats_eunit:weapon()]),
    near(110 * 0.45 * 1.99, damage(viral, maps:get(stats, Result))),
    ?assertEqual([<<"zframe/mod-1">>], maps:get(depends_on, ability(Result))).

evolution_base_damage_contributes_to_buff_test() ->
    Request = request(frame([]), []),
    Builds = maps:get(<<"buildset">>, Request),
    #{<<"build">> := Weapon} = maps:get(<<"weapon">>, Builds),
    Updated = Request#{<<"buildset">> => Builds#{<<"weapon">> =>
        #{<<"build">> => Weapon#{<<"skill_tree">> => <<"0102">>}}}},
    {ok, Result} = wfcli_build_stats:calculate(maps:merge(Updated, active()), #{},
                                              [wfcli_build_stats_eunit:weapon()]),
    near(134 * 0.45, damage(viral, maps:get(stats, Result))).

viral_addition_does_not_repair_primary_elements_test() ->
    Mods = [mod(1, <<"Pistol/DualStat/PoisonEventPistolMod">>, 3),
            mod(2, <<"Pistol/Expert/PrimedWeaponElectricityDamageMod">>, 7),
            mod(3, <<"Pistol/Expert/WeaponFireDamageModExpert">>, 10),
            mod(4, <<"Pistol/WeaponDamageAmountMod">>, 10)],
    {ok, Before} = calculate(frame([]), Mods, #{}),
    {ok, After} = calculate(frame([]), Mods, active()),
    A = maps:get(damage_per_projectile, maps:get(stats, After)),
    B = maps:get(damage_per_projectile, maps:get(stats, Before)),
    ?assertEqual(B, maps:remove(viral, A)),
    near(110 * 0.45 * 3.2, maps:get(viral, A)),
    near(110 * 1.8 * 3.2, maps:get(corrosive, A)).

existing_viral_damage_adds_in_same_channel_test() ->
    Mods = [mod(1, <<"Pistol/DualStat/PoisonEventPistolMod">>, 3),
            mod(2, <<"Pistol/WeaponFreezeDamageMod">>, 5)],
    {ok, Result} = calculate(frame([]), Mods, active()),
    near(110 * (0.6 + 0.9 + 0.45), damage(viral, maps:get(stats, Result))).

ability_rank_is_not_mod_rank_scaling_test() ->
    lists:foreach(fun({Rank, Coefficient, Duration}) ->
        F = config(maps:remove(<<"xp">>, frame([])), #{<<"ability_ranks">> => #{?NOURISH => Rank}}),
        {ok, Result} = calculate(F, [], active()),
        near(110 * Coefficient, damage(viral, maps:get(stats, Result))),
        Node = ability(Result),
        ?assertEqual(Rank, maps:get(rank, Node)),
        ?assertEqual(explicit, maps:get(rank_source, Node)),
        near(Duration, maps:get(duration, maps:get(selected, maps:get(states, Node))))
    end, [{0, 0.24, 10}, {1, 0.30, 15}, {2, 0.36, 20}, {3, 0.45, 25}]).

unknown_rank_is_explicit_gap_test() ->
    lists:foreach(fun(Xp) ->
        F = (frame([]))#{<<"xp">> => Xp},
        {ok, Result} = calculate(F, [], #{}),
        ?assertEqual([], [N || N = #{kind := ability} <- maps:get(effect_graph, Result)]),
        ?assert(lists:any(fun(#{reason := Reason}) -> binary:match(Reason, <<"rank is unknown">>) =/= nomatch;
                            (_) -> false end, maps:get(excluded, Result))),
        ?assertEqual({error, {unavailable_effects, [<<"frame/nourish">>]}}, calculate(F, [], active()))
    end, [0, 899999]),
    {ok, Result} = calculate(frame([]), [], #{}),
    ?assertEqual(max_rank_equipment, maps:get(rank_source, ability(Result))),
    ?assertMatch({error, invalid_stat_request},
        calculate(config(frame([]), #{<<"ability_ranks">> => #{?NOURISH => 4}}), [], #{})).

unreviewed_override_does_not_gain_nourish_test() ->
    F = config(frame([]), #{<<"ability_override">> => #{<<"Ability">> => <<"/unknown/ability">>,
                                                       <<"Index">> => 0}}),
    {ok, Result} = calculate(F, [], #{}),
    ?assertEqual([], [N || N = #{kind := ability} <- maps:get(effect_graph, Result)]),
    ?assertEqual({error, {unavailable_effects, [<<"frame/nourish">>]}}, calculate(F, [], active())),
    ?assertMatch({ok, #{status := partial}},
        calculate(config(frame([]), #{<<"ability_override">> => #{<<"Ability">> => ?NOURISH,
                                                                 <<"Index">> => 4}}), [], #{})).

ability_cannot_be_equipped_on_weapon_test() ->
    F = (frame([]))#{<<"class">> => <<"secondary">>},
    {ok, Result} = calculate(F, [], #{}),
    ?assertEqual([], [N || N = #{kind := ability} <- maps:get(effect_graph, Result)]),
    ?assertEqual({error, {unavailable_effects, [<<"frame/nourish">>]}}, calculate(F, [], active())).

buildset_is_not_multiple_squad_frames_test() ->
    Request = request(frame([]), []),
    Builds = maps:get(<<"buildset">>, Request),
    ?assertEqual({error, multiple_buildset_warframes},
        wfcli_build_stats:calculate(Request#{<<"buildset">> => Builds#{
            <<"ally">> => #{<<"build">> => frame([])}}}, #{}, [wfcli_build_stats_eunit:weapon()])).

ability_tracks_script_and_property_dependencies_test() ->
    {ok, Result} = calculate(frame([]), [], #{}),
    Dependencies = maps:get(dependencies, maps:get(provenance, Result)),
    ?assert(lists:member(<<"calculator.nourish">>, maps:get(mechanics, Dependencies))),
    ?assert(lists:member(?NOURISH, maps:get(properties, Dependencies))),
    {ok, Data} = file:read_file("apps/wfcompanion/src/inspect/luau/features.json"),
    ?assertMatch(#{<<"scripts">> := [_ | _], <<"properties">> := [?NOURISH]},
        maps:get(<<"calculator.nourish">>, maps:get(<<"features">>, jsone:decode(Data)))),
    ?assert(is_binary(jsone:encode(Result))).

frame(Mods) ->
    #{<<"definition_id">> => ?MESA, <<"class">> => <<"warframe">>, <<"xp">> => 900000,
      <<"config">> => #{<<"upgrade_slots">> => Mods,
          <<"ability_override">> => #{<<"Ability">> => ?NOURISH, <<"Index">> => 0}}}.
config(Frame, Changes) -> Frame#{<<"config">> => maps:merge(maps:get(<<"config">>, Frame), Changes)}.
active() -> #{<<"effects">> => #{<<"frame/nourish">> => 1}}.
request(Frame, Mods) ->
    #{<<"target">> => <<"weapon">>, <<"buildset">> => #{
        <<"frame">> => #{<<"build">> => Frame},
        <<"weapon">> => #{<<"build">> => #{
            <<"definition_id">> => maps:get(<<"uniqueName">>, wfcli_build_stats_eunit:weapon()),
            <<"class">> => <<"secondary">>, <<"config">> => #{<<"upgrade_slots">> => Mods}}}}}.
calculate(Frame, Mods, Options) ->
    wfcli_build_stats:calculate(maps:merge(request(Frame, Mods), Options), #{},
                                [wfcli_build_stats_eunit:weapon()]).
mod(N, Path, Rank) ->
    #{<<"topology_slot">> => <<"mod-", (integer_to_binary(N))/binary>>,
      <<"item_type">> => <<"/Lotus/Upgrades/Mods/", Path/binary>>, <<"rank">> => Rank}.
ability(Result) ->
    [Node] = [N || N = #{kind := ability} <- maps:get(effect_graph, Result)],
    Node.
damage(Element, Stats) -> maps:get(Element, maps:get(damage_per_projectile, Stats)).
near(Expected, Actual) -> ?assert(abs(Expected - Actual) < 0.001).

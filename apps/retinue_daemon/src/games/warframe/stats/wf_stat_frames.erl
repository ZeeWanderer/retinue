-module(wf_stat_frames).

-export([profile/1, base/2, evaluate/2, multipliers/1, upgrade/1]).

profile(Type) ->
    Reviewed = [<<"/Lotus/Powersuits/Cowgirl/Cowgirl">>,
                <<"/Lotus/Powersuits/Cowgirl/MesaPrime">>,
                <<"/Lotus/Powersuits/AntiMatter/Anti">>,
                <<"/Lotus/Powersuits/AntiMatter/NovaPrime">>,
                <<"/Lotus/Powersuits/Excalibur/Excalibur">>],
    case lists:member(Type, Reviewed) of
        true -> #{class => <<"warframe">>, max_rank => 30,
                  level_upgrades => lists:flatten(lists:duplicate(10,
                      [#{stat => Stat, operation => add_base, value => Value}
                       || {Stat, Value} <- [{health, 10}, {shield, 10}, {energy, 5}]])),
                  overshield_capacity => 1200};
        false -> unsupported
    end.

base(Export, Item) ->
    Type = maps:get(<<"definition_id">>, Item),
    Profile = case profile(Type) of
        unsupported -> throw({stats, {unsupported_frame_profile, Type}});
        Known -> Known
    end,
    {Rank, Source} = case {maps:get(<<"rank">>, Item, undefined), maps:get(<<"xp">>, Item, 0)} of
        {R, _} when is_integer(R), R >= 0, R =< 30 -> {R, explicit};
        {undefined, Xp} when is_integer(Xp), Xp >= 900000 -> {30, max_rank_equipment};
        _ -> throw({stats, {frame_rank_unknown, Type}})
    end,
    Stats = maps:from_list([{Key, required(Export, Field)} || {Key, Field} <-
        [{health, <<"health">>}, {shield, <<"shield">>}, {armor, <<"armor">>},
         {energy, <<"power">>}, {sprint_speed, <<"sprintSpeed">>}]]),
    Stats#{rank => Rank, rank_source => Source,
           overshield_capacity => maps:get(overshield_capacity, Profile),
           level_upgrades => lists:sublist(maps:get(level_upgrades, Profile), Rank)}.

required(Export, Key) ->
    case maps:get(Key, Export, undefined) of
        Value when is_number(Value), Value >= 0 -> Value;
        _ -> throw({stats, {frame_stat_unavailable, Key}})
    end.

evaluate(Base, Effects) ->
    Entries = maps:get(level_upgrades, Base) ++ Effects,
    Stats = maps:from_list([{Stat, max(0.0, value(Stat, maps:get(Stat, Base), Entries))}
        || Stat <- [health, shield, armor, energy, sprint_speed, overshield_capacity]]),
    maps:merge(Stats, multipliers(Effects)).

multipliers(Effects) ->
    maps:from_list([{Stat, max(0.0, value(Stat, 1, Effects))}
        || Stat <- [ability_strength, ability_duration, ability_range, ability_efficiency]]).

value(Stat, Base, Effects) ->
    wf_stat_math:scalar(Base, [{Op, V} || #{stat := S, operation := Op, value := V} <- Effects,
                                           S =:= Stat]).

upgrade(<<"/Lotus/Upgrades/Mods/Warframe/", Key/binary>>) ->
    case maps:find(Key, definitions()) of
        {ok, {Rank, Effects}} -> #{max_rank => Rank, effects => Effects, source_class => <<"warframe">>};
        error -> unsupported
    end;
upgrade(_) -> unsupported.

definitions() ->
    #{<<"AvatarHealthMaxMod">> => {10, [{health, stacking_multiply, 0.090910003}]},
      <<"AvatarShieldMaxMod">> => {10, [{shield, stacking_multiply, 0.090910003}]},
      <<"AvatarArmourMod">> => {10, [{armor, stacking_multiply, 0.090910003}]},
      <<"AvatarPowerMaxMod">> => {5, [{energy, stacking_multiply, 0.16666999}]},
      <<"Expert/AvatarPowerMaxModExpert">> => {10, [{energy, stacking_multiply, 0.168181}]},
      <<"AvatarSprintSpeedMod">> => {5, [{sprint_speed, stacking_multiply, 0.050000001}]},
      <<"AvatarAbilityStrengthMod">> => {5, [{ability_strength, stacking_multiply, 0.050000001}]},
      <<"AvatarAbilityRangeMod">> => {5, [{ability_range, stacking_multiply, 0.075000003}]},
      <<"AvatarAbilityDurationMod">> => {5, [{ability_duration, stacking_multiply, 0.050000001}]},
      <<"AvatarAbilityEfficiencyMod">> => {5, [{ability_efficiency, stacking_multiply, 0.050000001}]},
      <<"DualStat/CorruptedPowerEfficiencyWarframe">> =>
          {10, [{ability_strength, stacking_multiply, 0.090000004},
                {ability_efficiency, stacking_multiply, -0.050000001}]}}.

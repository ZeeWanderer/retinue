-module(wfcli_stat_profiles).

-export([weapon/1, base/2]).

weapon(Type) ->
    case maps:get(Type, profiles(), unsupported) of
        unsupported -> unsupported;
        {Family, Trigger, Pellets, Grip, Weights} ->
            #{class => case Family of pistol -> <<"secondary">>; _ -> <<"primary">> end,
              family => Family, mode => normal, kind => hitscan, trigger => Trigger,
              damage_channels => [impact, puncture, slash], pellets => Pellets,
              damage_weights => Weights,
              single_sidearm => Grip =:= single, dual_sidearm => Grip =:= dual}
    end.

%% Reviewed normal behaviors; matching an export's trigger alone is not enough.
profiles() ->
    #{<<"/Lotus/Weapons/Tenno/Pistols/PrimeVasto/PrimeVastoPistol">> =>
          {pistol, <<"SEMI">>, 1, single, [0.15000001, 0.15000001, 0.69999999]},
      <<"/Lotus/Weapons/Tenno/Pistol/RevolverPistol">> =>
          {pistol, <<"SEMI">>, 1, single, [0.25, 0.25, 0.5]},
      <<"/Lotus/Weapons/Tenno/Pistols/PrimeVasto/AkPrimeVasto/AkPrimeVastoPistol">> =>
          {pistol, <<"SEMI">>, 1, dual, [0.15000001, 0.15000001, 0.69999999]},
      <<"/Lotus/Weapons/Tenno/Rifle/Rifle">> =>
          {rifle, <<"AUTO">>, 1, none, [0.33000001, 0.33000001, 0.33999997]},
      <<"/Lotus/Weapons/Tenno/Rifle/BratonPrime">> =>
          {rifle, <<"AUTO">>, 1, none, [0.050000001, 0.34999999, 0.60000002]},
      <<"/Lotus/Weapons/Grineer/LongGuns/GrineerM16Homage/GrineerM16Rifle">> =>
          {rifle, <<"AUTO">>, 1, none, [0.44999999, 0.30000001, 0.25]},
      <<"/Lotus/Weapons/Tenno/Shotgun/QuadShotgun">> =>
          {shotgun, <<"SEMI">>, 7, none, [0.15000001, 0.64999998, 0.20000005]},
      <<"/Lotus/Weapons/Tenno/Shotgun/FullAutoShotgun">> =>
          {shotgun, <<"AUTO">>, 8, none, [0.55000001, 0.15000001, 0.29999995]},
      <<"/Lotus/Weapons/Tenno/Shotgun/PrimeBoar">> =>
          {shotgun, <<"AUTO">>, 8, none, [0.64999998, 0.15000001, 0.20000005]}}.

base(Export, Profile) ->
    ensure(maps:get(<<"trigger">>, Export, null) =:= maps:get(trigger, Profile), changed_attack_profile),
    Class = case maps:get(<<"productCategory">>, Export, undefined) of
        <<"LongGuns">> -> <<"primary">>;
        <<"Pistols">> -> <<"secondary">>;
        _ -> unsupported
    end,
    ensure(Class =:= maps:get(class, Profile), changed_attack_class),
    Damage = maps:get(<<"damagePerShot">>, Export, undefined),
    ensure(is_list(Damage) andalso length(Damage) =:= 20 andalso
           lists:all(fun nonnegative/1, Damage), invalid_base_damage),
    ensure(lists:all(fun(V) -> V == 0 end, lists:nthtail(3, Damage)), changed_attack_damage_channels),
    ensure(positive(<<"multishot">>, Export) == maps:get(pellets, Profile), changed_attack_projectiles),
    Total = positive(<<"totalDamage">>, Export),
    ensure(abs(lists:sum(Damage) - Total) < max(0.001, Total * 0.00001), inconsistent_base_damage),
    ensure(Total == trunc(Total), changed_attack_base_amount),
    Weights = maps:get(damage_weights, Profile),
    ensure(lists:all(fun({D, W}) -> abs(D - Total * W) =< max(0.00001, Total * 0.0000002) end,
                     lists:zip(lists:sublist(Damage, 3), Weights)), changed_attack_damage_weights),
    Magazine = positive(<<"magazineSize">>, Export),
    ensure(is_integer(Magazine), unsupported_fractional_magazine),
    #{damage => maps:from_list(lists:zip(maps:get(damage_channels, Profile), lists:sublist(Damage, 3))),
      packet => #{scalar => Total, channels => lists:zip(maps:get(damage_channels, Profile), Weights),
                  quantization => quantized},
      critical_chance => number(<<"criticalChance">>, Export),
      critical_multiplier => positive(<<"criticalMultiplier">>, Export),
      multishot => positive(<<"multishot">>, Export), status_chance => number(<<"procChance">>, Export),
      fire_rate => positive(<<"fireRate">>, Export), reload_time => number(<<"reloadTime">>, Export),
      magazine => Magazine}.

positive(Key, Map) ->
    Value = number(Key, Map),
    ensure(Value > 0, {invalid_base_stat, Key}),
    Value.
number(Key, Map) ->
    Value = maps:get(Key, Map, undefined),
    ensure(nonnegative(Value), {invalid_base_stat, Key}),
    Value.
nonnegative(Value) -> is_number(Value) andalso Value >= 0 andalso Value =< 1.0e9.
ensure(true, _) -> ok;
ensure(false, Reason) -> throw({stats, Reason}).

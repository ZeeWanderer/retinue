-module(wfcli_stat_math).

-export([scalar/2, scalar_updates/2, critical_multiplier/3, critical_factor/3, critical_tiers/1,
         critical_selection/3, critical_effects/3,
         ranked/4, pair_elements/2, armor_reduction/1,
         source_packet/3, contact_packet/3, per_status_damage/3,
         hit_channels/2, health_hit/2, health_outcomes/3,
         damage_amount/1,
         direct_fire/3]).

%% Entries are in evaluator traversal order, after filtering and insertion.
-spec scalar(number(), [{atom(), number()}]) -> float().
scalar(Base, Entries) ->
    evaluate(accumulate_bucket(Entries, accumulator(Base))).

%% Direct writes continue after SET; settle preserves native evaluation boundaries.
-spec scalar_updates(number(), list({atom(), number()} | settle)) -> float().
scalar_updates(Base, Updates) ->
    evaluate(accumulate_updates(Updates, accumulator(Base))).

accumulate_updates(Updates, Initial) ->
    lists:foldl(fun
        (settle, Acc) -> accumulator(evaluate(Acc));
        (Update, Acc) -> update_accumulator(Update, Acc)
    end, Initial, Updates).

accumulator(Base) ->
    #{base => f32(Base), add => 0.0, add_base => 0.0, stacking => 0.0, multiply => 1.0}.

evaluate(#{set := Value}) -> Value;
evaluate(Acc) ->
    Base = f32(maps:get(base, Acc) + maps:get(add_base, Acc)),
    Scaled = f32(Base * f32(1 + maps:get(stacking, Acc))),
    Value = f32(f32(Scaled + maps:get(add, Acc)) * maps:get(multiply, Acc)),
    Clamped = max(maps:get(minimum, Acc, Value), Value),
    min(maps:get(maximum, Acc, Clamped), Clamped).

accumulate_bucket([], Acc) -> Acc;
accumulate_bucket([{set, _} = Entry | _], Acc) -> update_accumulator(Entry, Acc);
accumulate_bucket([Entry | Rest], Acc) ->
    accumulate_bucket(Rest, update_accumulator(Entry, Acc)).

update_accumulator({set, Value}, Acc) -> Acc#{set => f32(Value)};
update_accumulator({set_base, Value}, Acc) -> Acc#{base => f32(Value)};
update_accumulator({set_min, Value}, Acc) ->
    Acc#{minimum => max(maps:get(minimum, Acc, 1.1754943508222875e-38), f32(Value))};
update_accumulator({set_max, Value}, Acc) ->
    Acc#{maximum => min(maps:get(maximum, Acc, 3.4028234663852886e38), f32(Value))};
update_accumulator({multiply, Value}, Acc) ->
    Acc#{multiply => f32(maps:get(multiply, Acc) * f32(Value))};
update_accumulator({Operation, Value}, Acc) ->
    Key = case Operation of
        add -> add;
        add_base -> add_base;
        stacking_multiply -> stacking
    end,
    Acc#{Key => f32(maps:get(Key, Acc) + f32(Value))}.

%% Native normalization quantizes base plus flat additions, before other modifiers.
-spec critical_multiplier(number(), [{atom(), number()}], quantized | unquantized) -> float().
critical_multiplier(Base, Entries, unquantized) -> scalar(Base, Entries);
critical_multiplier(Base, Entries, quantized) ->
    Acc = accumulate_bucket(Entries, accumulator(Base)),
    case Acc of
        #{set := Value} -> Value;
        _ ->
            Added = f32(maps:get(base, Acc) + maps:get(add_base, Acc)),
            Bounded = max(0, min(32, Added)),
            Encoded = floor(f32(f32(Bounded * 127.96875) + 0.5)),
            Quantized = f32(Encoded * 0.007814408279955387),
            evaluate(Acc#{base => Quantized, add_base => 0.0})
    end.

%% Add to the existing hit factor; the native caller clamps afterward.
-spec critical_factor(number(), number(), non_neg_integer()) -> float().
critical_factor(Factor, _Multiplier, 0) -> f32(Factor);
critical_factor(Factor, Multiplier, Tier)
  when is_integer(Tier), Tier > 0, Tier =< 16#7fffffff ->
    Extra = f32(f32(f32(Multiplier) - 1) * f32(Tier)),
    f32(f32(Factor) + Extra).

%% Fresh eligible tier only, before contextual tier boosts; uniform native 24-bit rolls.
-spec critical_tiers(number()) -> [#{tier := non_neg_integer(), probability := float()}].
critical_tiers(Chance) when is_number(Chance) ->
    {Tier, Fraction} = critical_parts(Chance),
    Upper = case Fraction > 0 of
        true -> min(16777216, floor(Fraction * 16777216) + 1) / 16777216;
        false -> 0.0
    end,
    [#{tier => T, probability => P}
     || {T, P} <- [{Tier, 1 - Upper}, {Tier + 1, Upper}], P > 0].

critical_parts(Chance) ->
    Native = f32(Chance),
    case Native of
        C when C =< 0 -> {0, 0.0};
        C when C < 2147483648.0 ->
            Whole = floor(C),
            {Whole band 255, f32(C - Whole)};
        _ -> error({unsupported_critical_chance, Chance})
    end.

%% Source override and extra chance are resolved inputs; preserve unused rolls for later stages.
-spec critical_selection(map(), map(), [0..16777215]) -> {map(), [0..16777215]}.
critical_selection(#{tier := Stored, critical_chance := Chance, critical_multiplier := Multiplier,
                     factor := Factor} = Hit,
                   #{bypass := Bypass, source_bypass_override := Override,
                     extra_tier_chance := Extra}, Rolls)
  when is_integer(Stored), Stored >= 0, Stored =< 16#7fffffff,
       is_boolean(Bypass), is_boolean(Override) ->
    {Tier, Remaining} = case Stored =:= 0 andalso (not Bypass orelse Override) of
        true ->
            {Whole, Fraction} = critical_parts(Chance),
            {Selected, Tail} = critical_increment(Whole, Fraction, Rolls),
            case Selected > 0 andalso Extra =/= none of
                true -> critical_increment(Selected, Extra, Tail);
                false -> {Selected, Tail}
            end;
        false -> {Stored, Rolls}
    end,
    {Hit#{tier := Tier, factor := critical_factor(Factor, Multiplier, Tier)}, Remaining}.

critical_increment(Tier, Chance, Rolls) ->
    {Apply, Tail} = critical_roll(Chance, Rolls),
    {Tier + case Apply of true -> 1; false -> 0 end, Tail}.

%% Resolved native packet flags/status IDs and upgrades; rolls are supplied, not generated.
-spec critical_effects(map(), map(), [0..16777215]) -> {map(), [0..16777215]}.
critical_effects(#{factor := Factor, tier := Tier, has_weapon := Weapon, self_hit := Self} = Hit,
                 Effects, Rolls)
  when is_integer(Tier), Tier >= 0, is_boolean(Weapon), is_boolean(Self) ->
    Initial = Hit#{factor := f32(Factor)},
    case Weapon andalso not Self of
        false -> {Initial, Rolls};
        true when Tier =:= 0 -> noncritical_effect(Initial, Effects, Rolls);
        true -> critical_hit_effects(Initial, Effects, Rolls)
    end.

noncritical_effect(#{hit_type := Type, proc_type := Proc} = Hit, Effects, Rolls)
  when Type =/= 7, Proc =:= 30 ->
    {Apply, Tail} = critical_roll(maps:get(noncritical_chance, Effects), Rolls),
    Factor = maps:get(factor, Hit),
    Final = case Apply of
        true -> f32(Factor + f32(maps:get(noncritical_bonus, Effects)));
        false -> Factor
    end,
    {Hit#{factor := Final}, Tail};
noncritical_effect(Hit, _, Rolls) -> {Hit, Rolls}.

critical_hit_effects(#{factor := Factor, active_status_types := Active,
                       forced_procs := Forced} = Hit,
                     #{slash_chance := Chance, low_status_bonus := Amount,
                       low_status_limit := Limit}, Rolls)
  when is_integer(Limit), Limit >= -2147483648, Limit =< 2147483647 ->
    {Apply, Tail} = critical_roll(Chance, Rolls),
    Updated = case Apply of
        true -> Forced#{2 => min(255, maps:get(2, Forced, 0) + 1)};
        false -> Forced
    end,
    Count = length(lists:usort([Id || Id <- Active, Id >= 0, Id < 13])),
    Bonus = f32(Amount),
    Final = case Bonus > 0 andalso Count < Limit of
        true -> f32(Factor + Bonus);
        false -> Factor
    end,
    {Hit#{factor := Final, forced_procs := Updated}, Tail}.

critical_roll(Chance, Rolls) ->
    Native = f32(Chance),
    case Native > 0 of
        true ->
            case Rolls of
                [Roll | Tail] when is_integer(Roll), Roll >= 0, Roll < 16777216 ->
                    {Roll / 16777216 =< Native, Tail};
                [] -> error(critical_rolls_exhausted);
                [Roll | _] -> error({invalid_critical_roll, Roll})
            end;
        false -> {false, Rolls}
    end.

%% Supported rules use the verified default linear rank curve.
-spec ranked(atom(), number(), non_neg_integer(), non_neg_integer()) -> float().
ranked(multiply, Value, Rank, MaxRank) when Value >= 0.5, Value < 1 ->
    max(0.0, f32(1 - ranked(add, f32(1 - Value), Rank, MaxRank)));
ranked(_Operation, Value, _Rank, 0) -> f32(Value);
ranked(_Operation, Value, Rank, _MaxRank) -> f32(f32(Value) * (Rank + 1)).

-spec pair_elements(map(), [atom()]) -> map().
pair_elements(Damage, Priority) ->
    pair_elements(Damage, Priority, fun(A, B, Existing) -> Existing + (A + B) end).

pair_elements(Damage, Priority, Combine) ->
    Primary = [heat, cold, electricity, toxin],
    Ordered = wfcli_forma_elements:unique([E || E <- Priority, lists:member(E, Primary)] ++ Primary),
    Present = [E || E <- Ordered, maps:get(E, Damage, 0) > 0],
    pair(Damage, Present, Combine).

pair(Damage, [A, B | Rest], Combine) ->
    Combined = compound(lists:sort([A, B])),
    Value = Combine(maps:get(A, Damage), maps:get(B, Damage), maps:get(Combined, Damage, 0)),
    Damage1 = maps:without([A, B], Damage),
    pair(Damage1#{Combined => Value}, Rest, Combine);
pair(Damage, _, _) -> Damage.

compound([cold, heat]) -> blast;
compound([electricity, heat]) -> radiation;
compound([heat, toxin]) -> gas;
compound([cold, electricity]) -> magnetic;
compound([cold, toxin]) -> viral;
compound([electricity, toxin]) -> corrosive.

%% Explicit native weights and eligible ordinary effects; priority is resolved by the caller.
-spec source_packet(#{scalar := number(), channels := [{atom(), number()}]}, [map()], [atom()]) -> map().
source_packet(#{scalar := Scalar, channels := Channels}, Effects, Priority) ->
    Primary = [impact, puncture, slash, heat, cold, electricity, toxin],
    Compound = [blast, radiation, gas, magnetic, viral, corrosive],
    Order = Primary ++ Compound,
    Initial = maps:from_list([{C, f32(V)} || {C, V} <- Channels]),
    true = map_size(Initial) =:= length(Channels),
    true = lists:all(fun(C) -> lists:member(C, Order) end, maps:keys(Initial)),
    Base = stat(base_damage, Scalar, Effects),
    Amount = stat(damage, Base, Effects),
    Upgraded = channel_effects(Initial, Primary, Effects),
    Paired = pair_elements(Upgraded, Priority, fun(A, B, Existing) ->
        f32(f32(A + B) + Existing)
    end),
    Weights = channel_effects(Paired, Compound, Effects),
    Largest = lists:max([0.0 | maps:values(Weights)]),
    {FinalScalar, FinalWeights} = case Largest > 15.96875 of
        true ->
            Scale = f32(Largest * 0.0626223087310791),
            Inverse = f32(1 / Scale),
            {f32(Amount * Scale), maps:map(fun(_, V) -> f32(V * Inverse) end, Weights)};
        false -> {Amount, Weights}
    end,
    #{scalar => FinalScalar,
      channels => [{C, V} || C <- Order, V <- [maps:get(C, FinalWeights, 0.0)], V /= 0]}.

channel_effects(Channels, Names, Effects) ->
    lists:foldl(fun(C, Acc) ->
        Acc#{C => stat({element, C}, maps:get(C, Acc, 0.0), Effects)}
    end, Channels, Names).

%% Resolved numeric contact stage only: no channel conversion,
%% subentity overrides or proc flags. Head factor and eligibility are caller evidence.
-spec contact_packet(map(), none | map(),
    #{head := boolean(), suppressed := boolean(), head_factor := number(),
      deferred_scalar := boolean(), weakpoint => none | map(), critical => none | map()}) -> map().
contact_packet(Packet, Proxy, #{head := Head, suppressed := Suppressed,
                               head_factor := HeadFactor, deferred_scalar := Deferred} = Context)
  when is_boolean(Head), is_boolean(Suppressed), is_boolean(Deferred) ->
    Keys = [scalar, critical_chance, critical_multiplier],
    Pending = maps:get(pending_updates, Packet, #{}),
    Initial = maps:from_list([{K, accumulate_updates(maps:get(K, Pending, []),
                               accumulator(maps:get(K, Packet)))} || K <- Keys]),
    Status = f32(maps:get(status_chance, Packet)),
    Channels = [{C, f32(V)} || {C, V} <- maps:get(channels, Packet)],
    {Modified, ContactStatus, ContactChannels} = proxy_modifiers(Initial, Status, Channels, Proxy),
    WithWeakpoint = case Proxy of
        none -> Modified;
        _ -> weakpoint_modifiers(Modified, maps:get(weakpoint, Context, none), Deferred)
    end,
    Settled = settle_contact_scalar(WithWeakpoint, Deferred),
    {Contact, FinalStatus} = case {Proxy, Head, Suppressed} of
        {none, _, _} -> {Initial, Status};
        {_, true, false} ->
            WithHead = Settled#{scalar := update_accumulator(
                {multiply, HeadFactor}, maps:get(scalar, Settled))},
            {settle_contact_scalar(WithHead, Deferred), ContactStatus};
        {_, true, true} ->
            case maps:get(treat_head_as_normal, Proxy) of
                false -> {Initial, Status};
                true -> {Settled, ContactStatus}
            end;
        _ -> {Settled, ContactStatus}
    end,
    %% Chance/multiplier must retain pending modifiers through contextual upgrades.
    Critical = critical_modifiers(Contact, maps:get(critical, Context, none)),
    Values = maps:map(fun(_, Acc) -> evaluate(Acc) end, Critical),
    maps:merge(maps:remove(pending_updates, Packet),
        Values#{status_chance => FinalStatus, channels => ContactChannels}).

settle_contact_scalar(Values, true) -> Values;
settle_contact_scalar(#{scalar := Acc} = Values, false) ->
    Values#{scalar := accumulator(evaluate(Acc))}.

weakpoint_modifiers(Values, none, _) -> Values;
weakpoint_modifiers(#{scalar := Acc} = Values, #{target := Target, weapon := Weapon}, Deferred) ->
    TargetAcc = upgrade_manager(Target, accumulator(0), Deferred),
    Bonus = max(0.0, evaluate(upgrade_manager(Weapon, TargetAcc, Deferred))),
    %% Native writes the multiplier field directly, not multiply-by-(1+bonus).
    Values#{scalar := Acc#{multiply := f32(maps:get(multiply, Acc) + Bonus)}}.

%% none skips an empty native bucket; entries=[] still settles a fully filtered bucket.
upgrade_manager(none, Acc, _) -> Acc;
upgrade_manager(#{entries := Entries} = Bucket, Acc, Deferred) ->
    Updated = accumulate_bucket(Entries, Acc),
    Scaled = case maps:find(scale, Bucket) of
        {ok, Scale} -> update_accumulator({multiply, Scale}, Updated);
        error -> Updated
    end,
    case Deferred of
        true -> Scaled;
        false -> accumulator(evaluate(Scaled))
    end.

critical_modifiers(Values, none) -> Values;
critical_modifiers(#{critical_chance := Chance, critical_multiplier := Multiplier} = Values,
                   #{chance := ChanceBuckets, multiplier := MultiplierBuckets,
                     chance_limit := Limit, low_chance := LowChance} = Context) ->
    Prepared = critical_buckets(ChanceBuckets, Chance),
    Linked = critical_link(Prepared, maps:get(linked_chance, Context, none)),
    Weapon = upgrade_manager(maps:get(weapon_chance, Context, none), Linked, true),
    Status = critical_status(Weapon, maps:get(per_status_chance, Context, none)),
    Updated = upgrade_manager(maps:get(stealth_chance, Context, none), Status, true),
    Limited = case Limit of
        none -> Updated;
        _ ->
            Maximum = f32(Limit),
            case Maximum < evaluate(Updated) of
                true -> update_accumulator({set, Maximum}, Updated);
                false -> Updated
            end
    end,
    Scaled = critical_buckets(MultiplierBuckets, Multiplier),
    Final = case LowChance of
        none -> Scaled;
        #{threshold := Threshold, multiplier := Bucket} ->
            case evaluate(Limited) =< f32(Threshold) of
                true -> upgrade_manager(Bucket, Scaled, true);
                false -> Scaled
            end
    end,
    Values#{critical_chance := Limited, critical_multiplier := Final}.

critical_link(Acc, none) -> Acc;
critical_link(Acc, #{base := Base, value := Value}) ->
    update_accumulator({add, f32(f32(Value) - f32(Base))}, Acc).

critical_status(Acc, none) -> Acc;
critical_status(#{base := Base, add_base := Add} = Acc, Statuses) when is_map(Statuses) ->
    %% Native uses SET-or-base plus ADD_BASE, not the evaluated chance.
    NativeBase = f32(maps:get(set, Acc, Base) + Add),
    Extra = lists:foldl(fun
        ({Id, #{count := Count, entries := Entries}}, Total)
          when is_integer(Id), Id >= 0, Id < 30, is_integer(Count),
               Count >= 0, Count =< 16#ffffffffffffffff ->
            case Count of
                0 -> Total;
                _ ->
                    Delta = f32(scalar(NativeBase, Entries) - NativeBase),
                    f32(Total + f32(Delta * critical_count(Count)))
            end
    end, 0.0, lists:sort(maps:to_list(Statuses))),
    update_accumulator({add, Extra}, Acc).

%% Preserve sticky bits before binary64 conversion, then round only to binary32.
critical_count(Count) when Count =< (1 bsl 53) -> f32(Count);
critical_count(Count) ->
    f32(2 * critical_count((Count bsr 1) bor (Count band 1))).

%% Critical accumulators always defer; the configurable scalar mode does not apply.
critical_buckets(Buckets, Acc) ->
    lists:foldl(fun(Bucket, Current) -> upgrade_manager(Bucket, Current, true) end, Acc, Buckets).

proxy_modifiers(Values, Status, Channels, none) -> {Values, Status, Channels};
proxy_modifiers(Values, Status, Channels, #{enabled := false}) -> {Values, Status, Channels};
proxy_modifiers(#{scalar := Damage, critical_chance := Chance,
                  critical_multiplier := Multiplier} = Values, Status, Channels,
                #{enabled := true, damage_multiplier := DamageScale,
                  hit_type_multiplier := TypeScale, critical_chance_add := ChanceAdd,
                  critical_multiplier := CriticalScale, status_multiplier := StatusScale,
                  channel_add := Adds, channel_multiply := Scales}) ->
    Modified = Values#{
        scalar := accumulate_updates([{multiply, DamageScale}, {multiply, TypeScale}], Damage),
        critical_chance := update_accumulator({add, ChanceAdd}, Chance),
        critical_multiplier := update_accumulator({multiply, CriticalScale}, Multiplier)},
    Present = [C || {C, _} <- Channels],
    true = length(lists:usort(Present)) =:= length(Present),
    true = lists:all(fun(C) -> lists:member(C, Present) end, maps:keys(Adds) ++ maps:keys(Scales)),
    Applied = [{C, f32(max(0.0, f32(V + f32(maps:get(C, Adds, 0)))) *
                      f32(maps:get(C, Scales, 1)))} || {C, V} <- Channels],
    {Modified, f32(Status * f32(StatusScale)), Applied}.

%% Preserve native float32 stages; algebraic simplification changes rounding.
-spec armor_reduction(non_neg_integer()) -> float().
armor_reduction(Armor) when is_integer(Armor), Armor >= 0, Armor =< 16#7fffffff ->
    Peak = 2700,
    Base = case Armor =< Peak of true -> Peak; false -> Armor end,
    Weighted = f32(f32(Base) * f32(1 / 300)),
    Ratio = f32(Weighted / f32(Weighted + 1)),
    case Armor =< Peak of
        true -> f32(f32(math:sqrt(f32(f32(Armor) / Peak))) * Ratio);
        false -> Ratio
    end.

%% Each list contains the eligible upgrades for one active status kind (native 0x103).
%% Base comes from the attack's base getter, not its already modified damage.
-spec per_status_damage(number(), number(), [[{atom(), number()}]]) -> float().
per_status_damage(Base, Current, Kinds) ->
    NativeBase = f32(Base),
    Extra = lists:foldl(fun(Entries, Acc) ->
        f32(Acc + f32(scalar(NativeBase, Entries) - NativeBase))
    end, 0.0, Kinds),
    f32(f32(Current) + Extra).

%% Relative weights, not damage amounts. Preserve the packet's bypass policy.
-spec hit_channels(map(), quantized | unquantized) -> map().
hit_channels(Channels, quantized) ->
    maps:map(fun(_, Weight) ->
        Bounded = f32(max(0, min(15.96875, Weight))),
        floor(f32(f32(Bounded * 32) + 0.5)) / 32
    end, Channels);
hit_channels(Channels, unquantized) ->
    maps:map(fun(_, Weight) -> f32(Weight) end, Channels).

%% Resolved uniform health mitigation; preserve native channel order and round the sum.
-spec health_hit(#{scalar := number(), channels := [{atom(), number()}],
                   quantization := quantized | unquantized},
                 #{multipliers := #{atom() => number()}, armor_reduction := number()}) -> map().
health_hit(#{scalar := Scalar, channels := Channels, quantization := Policy},
           #{multipliers := Multipliers, armor_reduction := Reduction}) ->
    ChannelMap = maps:from_list(Channels),
    true = map_size(ChannelMap) =:= length(Channels),
    Weights = hit_channels(ChannelMap, Policy),
    Base = f32(Scalar),
    NativeReduction = f32(Reduction),
    Applied = [{Channel,
                apply_armor(f32(f32(Base * maps:get(Channel, Weights)) *
                    f32(maps:get(Channel, Multipliers, 1))), NativeReduction)}
               || {Channel, _} <- Channels],
    Amount = lists:foldl(fun({_, Value}, Sum) -> f32(Sum + Value) end, 0.0, Applied),
    #{scalar => Base, channels => Weights, applied_channels => maps:from_list(Applied),
      raw_amount => Amount, health_amount => damage_amount(Amount)}.

apply_armor(Amount, Reduction) when Amount > 1, Reduction > 0 ->
    max(1.0, f32(Amount * f32(1 - Reduction)));
apply_armor(Amount, _Reduction) -> Amount.

%% Resolved multiplier/context and uniform-health target; no extra-tier effects or pellet grouping.
-spec health_outcomes(map(), #{chance := number(), multiplier := number(), factor := number()}, map()) -> map().
health_outcomes(#{scalar := Scalar} = Packet,
                #{chance := Chance, multiplier := Multiplier, factor := Context}, Target) ->
    Outcomes = [begin
        Factor = max(0.0, critical_factor(Context, Multiplier, Tier)),
        HitScalar = scalar(Scalar, [{multiply, Factor}]),
        Result#{factor => Factor, hit => health_hit(Packet#{scalar => HitScalar}, Target)}
    end || Result = #{tier := Tier} <- critical_tiers(Chance)],
    #{outcomes => Outcomes,
      expected_health_amount => lists:sum([P * maps:get(health_amount, Hit)
          || #{probability := P, hit := Hit} <- Outcomes])}.

%% Health packet conversion after channel summation; not expected damage or DPS.
-spec damage_amount(number()) -> non_neg_integer().
damage_amount(Value) ->
    Limit = 2147483520.0,
    Bounded = max(-Limit, min(Limit, Value)),
    max(0, floor(min(Limit, f32(f32(Bounded) + 0.5)))).

-spec direct_fire(map(), [map()], map()) -> map().
direct_fire(Base, Effects, Scenario) ->
    Initial = maps:get(packet, Base),
    Policy = maps:get(quantization, Initial),
    Priority = [Element || #{stat := {element, Element}} <- Effects],
    Packet = (source_packet(Initial, Effects, Priority))#{quantization => Policy},
    Damage = maps:from_list([{C, f32(maps:get(scalar, Packet) * V)}
                             || {C, V} <- maps:get(channels, Packet)]),
    CritChance = max(0.0, stat(critical_chance, maps:get(critical_chance, Base), Effects)),
    case CritChance < 2147483648.0 of
        true -> ok;
        false -> throw({stats, {unsupported_critical_chance, CritChance}})
    end,
    CritDamage = critical_multiplier(maps:get(critical_multiplier, Base),
        [{Op, V} || #{stat := critical_multiplier, operation := Op, value := V} <- Effects], Policy),
    Multishot = max(0.0, stat(multishot, maps:get(multishot, Base), Effects)),
    Rate = max(0.0, stat(fire_rate, maps:get(fire_rate, Base), Effects)),
    Reload = maps:get(reload_time, Base) / max(0.01, stat(reload_speed, 1, Effects)),
    Magazine = max(1, round(stat(magazine, maps:get(magazine, Base), Effects))),
    Consumption = max(0.0, stat(ammo_consumption, 1, Effects)),
    Reduction = armor_reduction(maps:get(armor, Scenario)),
    Critical = #{chance => CritChance, multiplier => CritDamage, factor => 1},
    Target = #{multipliers => #{}, armor_reduction => Reduction},
    Health = health_outcomes(Packet, Critical, Target),
    Raw = health_outcomes(Packet, Critical, Target#{armor_reduction => 0}),
    Outcomes = lists:zipwith(fun(#{tier := Tier, probability := P, factor := Factor, hit := R},
                                 #{hit := H}) ->
        #{tier => Tier, probability => P, factor => Factor,
          raw_damage => maps:get(raw_amount, R), health_damage => maps:get(health_amount, H)}
    end, maps:get(outcomes, Raw), maps:get(outcomes, Health)),
    CritMean = lists:sum([P * F || #{probability := P, factor := F} <- Outcomes]),
    RawMean = lists:sum([P * D || #{probability := P, raw_damage := D} <- Outcomes]),
    HealthMean = maps:get(expected_health_amount, Health),
    RawShot = RawMean * Multishot,
    TargetShot = HealthMean * Multishot,
    Shots = case Consumption of C when C == 0 -> null; _ -> Magazine / Consumption end,
    Sustained = case {Rate, Shots} of
        {R, _} when R == 0 -> 0.0;
        {_, null} -> TargetShot * Rate;
        _ -> TargetShot * Shots / (Shots / Rate + Reload)
    end,
    #{damage_per_projectile => Damage,
      source_packet => Packet#{channels => maps:from_list(maps:get(channels, Packet))},
      projectile_outcomes => Outcomes,
      expected_raw_damage_per_projectile => RawMean,
      expected_target_damage_per_projectile => HealthMean,
      critical_chance => CritChance, critical_multiplier => CritDamage,
      expected_critical_multiplier => CritMean, multishot => Multishot,
      status_chance => status_per_projectile(Base, Effects),
      status_damage_multiplier => stat(status_damage, 1, Effects),
      fire_rate => Rate, reload_time => Reload, magazine => Magazine,
      ammo_per_trigger => Consumption, expected_triggers_per_magazine => Shots,
      magazine_unbounded => Shots =:= null,
      armor_reduction => Reduction,
      expected_raw_damage_per_trigger => RawShot,
      expected_target_damage_per_trigger => TargetShot,
      expected_target_damage_per_magazine => case Shots of null -> null; _ -> TargetShot * Shots end,
      theoretical_burst_dps => TargetShot * Rate,
      theoretical_sustained_dps => Sustained}.

status_per_projectile(Base, Effects) ->
    %% Export is per pellet; native upgrades run before division by base fire iterations.
    Pellets = maps:get(multishot, Base),
    Chance = stat(status_chance, maps:get(status_chance, Base) * Pellets, Effects),
    f32(max(0.0, Chance) * f32(1 / Pellets)).

stat(Name, Base, Effects) ->
    scalar(Base, [{Op, Value} || #{stat := Stat, operation := Op, value := Value} <- Effects,
                                Stat =:= Name]).

f32(Value) ->
    <<Rounded:32/float>> = <<(float(Value)):32/float>>,
    Rounded.

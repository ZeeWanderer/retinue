-module(wfcli_stat_abilities).

-export([definition/2, evaluate/4, outputs/2]).

definition(<<"/Lotus/Powersuits/Devourer/Abilities/DevourerConsumeAbility">>, helminth) ->
    #{name => <<"Nourish (Helminth)">>, mechanic => <<"calculator.nourish">>, evaluator => weapon_buff,
      effects => [], max_rank => 3, element => viral,
      values_by_rank => [0.4, 0.5, 0.6, 0.75], variant_multiplier => 0.6,
      durations_by_rank => [10, 15, 20, 25],
      target_classes => [<<"primary">>, <<"secondary">>, <<"melee">>],
      conditional => #{id => <<"nourish">>, max_stacks => 1, effects => [],
                       activation => <<"Cast Nourish; weapon buff held active">>},
      outside_scope => <<"Nourish energy gain, healing, retaliation and status procs are not evaluated">>};
definition(<<"/Lotus/Powersuits/AntiMatter/Abilities/MolecularPrimeAbility">>, native) ->
    #{name => <<"Molecular Prime">>, mechanic => <<"calculator.molecular-prime">>,
      evaluator => molecular_prime, effects => [], max_rank => 3, target_classes => [],
      slow_by_rank => [0.3, 0.35, 0.4, 0.5], slow_cap => 0.75,
      wave_durations => [2, 3, 4, 6], effect_durations => [10, 15, 20, 30],
      blast_damage => [150, 300, 500, 800], blast_radius => [6, 7, 8, 10],
      initial_radius => 5, wave_speed => 5,
      outside_scope => <<"Molecular Prime target application, resistance, detonations and augments are not simulated">>};
definition(_, _) -> unsupported.

evaluate(#{evaluator := weapon_buff} = Rule, Rank, Multipliers, Count) ->
    Strength = maps:get(ability_strength, Multipliers),
    Duration = maps:get(ability_duration, Multipliers),
    Bonus = wfcli_stat_math:scalar(at(values_by_rank, Rule, Rank),
                [{multiply, maps:get(variant_multiplier, Rule)}, {multiply, Strength}]),
    Seconds = at(durations_by_rank, Rule, Rank) * Duration,
    #{active_stacks => Count, ability_strength => Strength, duration => Seconds,
      outputs => #{weapon_damage_bonus => #{kind => elemental_bonus, element => maps:get(element, Rule),
                                            value => Bonus, unit => fraction},
                   duration => quantity(Seconds, seconds)},
      effects => [#{stat => {element, maps:get(element, Rule)}, operation => add,
                     value => Bonus * Count} || Count > 0]};
evaluate(#{evaluator := molecular_prime} = Rule, Rank, Multipliers, _Count) ->
    Strength = maps:get(ability_strength, Multipliers),
    Duration = maps:get(ability_duration, Multipliers),
    Range = maps:get(ability_range, Multipliers),
    Slow = min(maps:get(slow_cap, Rule), at(slow_by_rank, Rule, Rank) * Strength),
    Wave = at(wave_durations, Rule, Rank) * Duration,
    #{effects => [], outputs => #{
        crowd_control => #{kind => speed_modifier, unit => multiplier,
                           variants => #{slow => 1 - Slow, speed => 1 + Slow}},
        damage_vulnerability => quantity(2.0, multiplier),
        wave_duration => quantity(Wave, seconds),
        effect_duration => quantity(at(effect_durations, Rule, Rank) * Duration, seconds),
        wave_radius => quantity(maps:get(initial_radius, Rule) + maps:get(wave_speed, Rule) * Wave, meters),
        explosion => #{kind => damage, damage_type => blast,
                       value => at(blast_damage, Rule, Rank) * Strength, unit => damage},
        explosion_radius => quantity(at(blast_radius, Rule, Rank) * Range, meters)}}.

at(Key, Rule, Rank) -> lists:nth(Rank + 1, maps:get(Key, Rule)).
quantity(Value, Unit) -> #{value => Value, unit => Unit}.

outputs(Graph, State) ->
    [maps:merge(maps:with([owner, slot, item, item_type, rank, rank_source, variant], Node),
                maps:with([outputs, active_stacks], maps:get(maps:get(id, Node), maps:get(nodes, State))))
        || Node = #{kind := ability} <- maps:get(nodes, Graph)].

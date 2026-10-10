-module(wfcli_stat_effects).

-export([compile/4, evaluate/3, conditions/2, public/2, dependencies/2]).

compile(Buildset, Target, Profile, Catalog) ->
    State = lists:foldl(fun({Owner, Item}, Acc) ->
        WithItem = item(Owner, Item, Buildset, Target, Profile, Acc),
        native_abilities(Owner, Item, Buildset, Catalog, WithItem)
    end, #{nodes => [], excluded => []}, lists:sort(maps:to_list(Buildset))),
    Nodes = maps:get(nodes, State),
    Combo = [combo_node(Owner, Item, Nodes) || {Owner, Item} <- lists:sort(maps:to_list(Buildset)),
                                               maps:get(<<"class">>, Item) =:= <<"melee">>],
    KnownCombo = [N || N <- Combo, N =/= unsupported],
    {Outburst, Gaps} = outburst_nodes(Nodes, KnownCombo),
    Ordinary = [N || N <- Nodes, maps:get(kind, N) =/= ability,
                      maps:get(derive, maps:get(rule, N), none) =/= outburst],
    Abilities = [N#{depends_on => [maps:get(id, Source) || Source <- Ordinary,
                     lists:member(maps:get(owner, N), maps:get(targets, Source)),
                     affects(ability_strength, maps:get(rule, Source)) orelse
                     affects(ability_duration, maps:get(rule, Source)) orelse
                     affects(ability_range, maps:get(rule, Source)) orelse
                     affects(ability_efficiency, maps:get(rule, Source))]}
                 || N = #{kind := ability} <- Nodes],
    Ordered = Ordinary ++ Abilities ++ KnownCombo ++ Outburst,
    Ids = [maps:get(id, N) || N <- Ordered],
    ensure(length(Ids) =:= length(lists:usort(Ids)), duplicate_effect_source),
    %% Apply source modifiers before ability scaling and combo-dependent buffs.
    State#{nodes => Ordered, core => case maps:get(class, Profile) of
               <<"warframe">> -> <<"calculator.warframe.core">>;
               _ -> <<"calculator.direct-fire.core">>
           end,
           excluded => maps:get(excluded, State) ++ Gaps}.

item(Owner, Item, Buildset, Target, Profile, State) ->
    State1 = lists:foldl(fun(Slot, Acc) ->
        Type = maps:get(<<"item_type">>, Slot),
        Source = source(Owner, Item, maps:get(<<"topology_slot">>, Slot),
                        maps:get(<<"name">>, Slot, Type), Type),
        add_rule(Source, maps:get(<<"rank">>, Slot, undefined), wfcli_stat_rules:upgrade(Type),
                 Buildset, Acc)
    end, State, wfcli_stat_buildset:ordered_upgrades(Item)),
    State2 = evolutions(Owner, Item, Buildset, State1),
    State3 = lists:foldl(fun(Shard, Acc) ->
        Type = maps:get(<<"UpgradeType">>, maps:get(<<"upgrade">>, Shard)),
        Source = source(Owner, Item, maps:get(<<"slot_id">>, Shard), filename:basename(Type), Type),
        add_rule(Source, 0, wfcli_stat_rules:shard(Type), Buildset, Acc)
    end, State2, maps:get(<<"shard_slots">>, Item, [])),
    State4 = case maps:get(<<"class">>, Item) of
        <<"warframe">> -> frame(Owner, Item, Buildset, Target, Profile, State3);
        _ when Owner =:= Target -> State3;
        _ ->
            Type = maps:get(<<"definition_id">>, Item),
            case {wfcli_stat_profiles:weapon(Type), wfcli_stat_rules:melee(Type)} of
                {unsupported, unsupported} ->
                    gap(source(Owner, Item, <<"intrinsic">>, maps:get(<<"name">>, Item), Type),
                        <<"Intrinsic and ability effects of this contextual item are not reviewed">>, State3);
                _ -> State3
            end
    end,
    State5 = ability_override(Owner, Item, Buildset, State4),
    Gaps = case maps:get(<<"unknown_feature_flags">>, Item, 0) of
        0 -> [];
        _ -> [#{owner => Owner, reason => <<"Equipment has unknown feature flags">>}]
    end,
    State5#{excluded => maps:get(excluded, State5) ++ Gaps}.

ability_override(Owner, Item, Buildset, State) ->
    Config = maps:get(<<"config">>, Item),
    case maps:get(<<"ability_override">>, Config, null) of
        null -> State;
        [] -> State;
        #{<<"Ability">> := Type, <<"Index">> := Index}
          when is_binary(Type), is_integer(Index), Index >= 0, Index < 4 ->
            Source = source(Owner, Item, <<"ability-override">>, filename:basename(Type), Type),
            case {maps:get(<<"class">>, Item), wfcli_stat_abilities:definition(Type, helminth)} of
                {<<"warframe">>, Rule} when is_map(Rule) ->
                    ability_node(Source#{variant => helminth}, Item, Rule, Buildset, State);
                _ -> gap(Source, <<"Ability override is not reviewed for this equipment">>, State)
            end;
        Override -> State#{excluded => maps:get(excluded, State) ++
            [#{owner => Owner, reason => <<"Ability override is not evaluated">>, ability => Override}]}
    end.

native_abilities(Owner, #{<<"class">> := <<"warframe">>} = Item, Buildset, Catalog, State) ->
    Type = maps:get(<<"definition_id">>, Item),
    Definitions = [D || D <- Catalog, maps:get(<<"uniqueName">>, D, null) =:= Type],
    case Definitions of
        [#{<<"abilities">> := Abilities}] when is_list(Abilities) ->
            Override = maps:get(<<"ability_override">>, maps:get(<<"config">>, Item), #{}),
            Replaced = case Override of
                #{<<"Index">> := Index, <<"Ability">> := AbilityType}
                  when is_integer(Index), Index >= 0, Index < 4, is_binary(AbilityType) -> Index;
                _ -> -1
            end,
            lists:foldl(fun({Index, Ability}, Acc) ->
                case Index - 1 =:= Replaced of
                    true -> Acc;
                    false -> native_ability(Owner, Item, Buildset, Index, Ability, Acc)
                end
            end, State, lists:enumerate(Abilities));
        _ -> gap(source(Owner, Item, <<"abilities">>, <<"Native abilities">>, Type),
                 <<"Active ability catalog is unavailable">>, State)
    end;
native_abilities(_, _, _, _, State) -> State.

native_ability(Owner, Item, Buildset, Index,
               #{<<"abilityUniqueName">> := Type, <<"abilityName">> := Name}, State)
  when is_binary(Type), is_binary(Name) ->
    Source = (source(Owner, Item, <<"ability-", (integer_to_binary(Index))/binary>>, Name, Type))#{variant => native},
    case wfcli_stat_abilities:definition(Type, native) of
        unsupported -> gap(Source, <<"Ability is not evaluated; assumed inactive">>, State);
        Rule -> ability_node(Source, Item, Rule, Buildset, State)
    end;
native_ability(Owner, Item, _, Index, _, State) ->
    gap(source(Owner, Item, <<"ability-", (integer_to_binary(Index))/binary>>,
               <<"Native ability">>, maps:get(<<"definition_id">>, Item)),
        <<"Unrecognized ability catalog entry">>, State).

ability_node(Source, Item, Rule, Buildset, State) ->
    Type = maps:get(item_type, Source),
    Config = maps:get(<<"config">>, Item),
    Explicit = maps:get(Type, maps:get(<<"ability_ranks">>, Config, #{}), undefined),
    Rank = case {Explicit, maps:get(<<"rank">>, Item, undefined), maps:get(<<"xp">>, Item, 0)} of
        {undefined, 30, _} -> 3;
        {undefined, undefined, Xp} when is_integer(Xp), Xp >= 900000 -> 3;
        _ -> Explicit
    end,
    case is_integer(Rank) andalso Rank >= 0 andalso Rank =< maps:get(max_rank, Rule) of
        false -> gap(Source, <<"Ability rank is unknown; supply config.ability_ranks for this ability">>, State);
        true ->
            Targets = lists:append([class_slots(Class, Buildset) || Class <- maps:get(target_classes, Rule)]),
            Node = Source#{item => maps:get(name, Rule), kind => ability, rule => Rule,
                           mechanic => maps:get(mechanic, Rule), rank => Rank,
                           rank_source => case Explicit of undefined -> max_rank_equipment; _ -> explicit end,
                           targets => Targets, depends_on => [],
                           condition => condition(Rule, maps:get(owner, Source), Rank, Buildset)},
            State#{nodes => maps:get(nodes, State) ++ [Node]}
    end.

source(Owner, Item, Slot, Name, Type) ->
    #{id => <<Owner/binary, "/", Slot/binary>>, owner => Owner, slot => Slot,
      item => Name, item_type => Type, definition_id => maps:get(<<"definition_id">>, Item),
      instance_id => maps:get(<<"instance_id">>, Item, null)}.

add_rule(Source, _Rank, unsupported, _Buildset, State) ->
    gap(Source, <<"unsupported upgrade">>, State);
add_rule(Source, Rank, Rule = #{source_class := Class}, Buildset, State) ->
    Item = maps:get(maps:get(owner, Source), Buildset),
    case maps:get(<<"class">>, Item) =:= Class of
        true -> add_rule(Source, Rank, maps:remove(source_class, Rule), Buildset, State);
        false -> gap(Source, <<"Upgrade is incompatible with the equipment class">>, State)
    end;
add_rule(Source, Rank, Rule = #{family := Family}, Buildset, State) ->
    Item = maps:get(maps:get(owner, Source), Buildset),
    case wfcli_stat_profiles:weapon(maps:get(<<"definition_id">>, Item)) of
        #{family := Family, class := Class} ->
            case maps:get(<<"class">>, Item) =:= Class of
                true -> add_rule(Source, Rank, maps:remove(family, Rule), Buildset, State);
                false -> gap(Source, <<"Upgrade is incompatible with the equipment class">>, State)
            end;
        unsupported -> gap(Source, <<"Upgrade compatibility needs a reviewed weapon profile">>, State);
        _ -> gap(Source, <<"Upgrade is incompatible with the weapon family">>, State)
    end;
add_rule(Source, Rank, Rule, Buildset, State) ->
    Max = maps:get(max_rank, Rule),
    case is_integer(Rank) andalso Rank >= 0 andalso Rank =< Max of
        false -> gap(Source, <<"invalid or unknown rank">>, State);
        true ->
            Owner = maps:get(owner, Source),
            Targets = case maps:get(target, Rule, self) of
                self -> [Owner];
                Class -> class_slots(Class, Buildset)
            end,
            Condition = condition(Rule, Owner, Rank, Buildset),
            Node = Source#{rank => Rank, kind => effects, rule => Rule,
                           mechanic => maps:get(mechanic, Rule, <<"calculator.upgrades">>),
                           targets => Targets, condition => Condition, depends_on => []},
            State#{nodes => maps:get(nodes, State) ++ [Node],
                   excluded => maps:get(excluded, State) ++
                       [Source#{reason => Reason} || Reason <- maps:get(excluded, Rule, [])]}
    end.

condition(#{conditional := Condition}, Owner, Rank, Buildset) ->
    Id = maps:get(id, Condition),
    HasRequired = case maps:find(requires_class, Condition) of
        error -> true;
        {ok, Class} -> class_slots(Class, Buildset) =/= []
    end,
    HasForbidden = case maps:find(requires_absent, Condition) of
        error -> false;
        {ok, Absent} -> class_slots(Absent, Buildset) =/= []
    end,
    Cap = maps:get(max_stacks, Condition),
    Max = case maps:get(rank_scaled_cap, Condition, false) of
        true -> ceil(Cap / (Rank + 1));
        false -> Cap
    end,
    Condition#{id => <<Owner/binary, "/", Id/binary>>, max_stacks => Max,
               enabled => HasRequired andalso not HasForbidden};
condition(_, _, _, _) -> null.

evolutions(Owner, Item, Buildset, State) ->
    Type = maps:get(<<"definition_id">>, Item),
    Tree = maps:get(<<"skill_tree">>, Item, <<>>),
    Source = source(Owner, Item, <<"evolutions">>, <<"Incarnon evolutions">>, Type),
    case {Tree, wfcli_stat_rules:evolutions(Type)} of
        {<<>>, _} ->
            case maps:get(<<"incarnon_genesis">>, maps:get(<<"features">>, Item, #{}), false) of
                true -> gap(Source, <<"Incarnon evolutions are missing from the build">>, State);
                false -> State
            end;
        {_, unsupported} -> gap(Source, <<"Incarnon evolutions are not reviewed">>, State);
        {_, Tiers} ->
            Choices = binary_to_list(Tree),
            Valid = length(Choices) =< length(Tiers) andalso
                lists:all(fun({Choice, Tier}) -> Choice >= $0 andalso Choice - $0 < length(Tier) end,
                          lists:zip(Choices, lists:sublist(Tiers, length(Choices)))),
            case Valid of
                false -> gap(Source, <<"Unrecognized Incarnon skill tree">>, State);
                true ->
                    lists:foldl(fun({N, Choice}, Acc) ->
                        Rule = lists:nth(Choice - $0 + 1, lists:nth(N, Tiers)),
                        Slot = <<"evolution-", (integer_to_binary(N))/binary>>,
                        S = source(Owner, Item, Slot, maps:get(item, Rule), maps:get(item_type, Rule)),
                        add_rule(S, 0, Rule, Buildset, Acc)
                    end, State, lists:enumerate(Choices))
            end
    end.

frame(Owner, Item, Buildset, Target, Weapon, State) ->
    Source = source(Owner, Item, <<"passive">>, <<"Warframe passive">>, maps:get(<<"definition_id">>, Item)),
    case wfcli_stat_rules:frame(maps:get(<<"definition_id">>, Item)) of
        unsupported -> gap(Source, <<"Warframe passive is not evaluated">>, State);
        Profile ->
            Effects = [maps:remove(requires, E) || E <- maps:get(passive, Profile),
                          maps:get(maps:get(requires, E), Weapon, false)],
            Rule = #{max_rank => 0, effects => [effect_tuple(E) || E <- Effects]},
            Node = Source#{rank => 0, kind => effects, rule => Rule, targets => [Target],
                           mechanic => maps:get(mechanic, Profile),
                           condition => null, depends_on => []},
            WithPassive = State#{nodes => maps:get(nodes, State) ++ [Node]},
            case maps:find(no_melee_health, Profile) of
                error -> WithPassive;
                {ok, Health} ->
                    HealthSource = source(Owner, Item, <<"no-melee-health">>, <<"Mesa no-melee health">>,
                                          maps:get(<<"definition_id">>, Item)),
                    HealthRule = #{max_rank => 0, effects => [], mechanic => maps:get(mechanic, Profile),
                        conditional => #{id => <<"no-melee-health">>, max_stacks => 1, automatic => true,
                            requires_absent => <<"melee">>, effects => [{health, add, Health}],
                            activation => <<"No melee in the selected buildset">>}},
                    add_rule(HealthSource, 0, HealthRule, Buildset, WithPassive)
            end
    end.

combo_node(Owner, Item, Nodes) ->
    case wfcli_stat_rules:melee(maps:get(<<"definition_id">>, Item)) of
        unsupported -> unsupported;
        Profile ->
            Dependencies = [maps:get(id, N) || N <- Nodes, lists:member(Owner, maps:get(targets, N)),
                                               affects(initial_combo, maps:get(rule, N))],
            (source(Owner, Item, <<"combo">>, <<"Melee combo">>, maps:get(<<"definition_id">>, Item)))#{
                kind => combo, profile => Profile, depends_on => Dependencies,
                mechanic => <<"calculator.combo">>,
                condition => #{id => <<Owner/binary, "/combo">>,
                               max_stacks => maps:get(combo_cap, Profile), enabled => true,
                               default => initial_combo,
                               activation => <<"Combo hits at buff activation; omitted uses recovered initial combo">>}}
    end.

outburst_nodes(Nodes, Combo) ->
    lists:foldl(fun(Node, {Acc, Gaps}) ->
        case maps:get(derive, maps:get(rule, Node), none) of
            outburst ->
                case Combo of
                    [C] ->
                        Id = maps:get(id, C),
                        {[Node#{kind => outburst, depends_on => [Id]} | Acc], Gaps};
                    _ -> {Acc, [(maps:with([item, item_type, owner, slot], Node))#{
                        reason => <<"Outburst requires one selected melee with a reviewed combo profile">>} | Gaps]}
                end;
            _ -> {Acc, Gaps}
        end
    end, {[], []}, Nodes).

affects(Stat, Rule) ->
    lists:any(fun({S, _, _}) -> S =:= Stat end,
        maps:get(effects, Rule) ++ maps:get(effects, maps:get(conditional, Rule, #{}), [])).
class_slots(Class, Buildset) ->
    lists:sort([Slot || {Slot, Item} <- maps:to_list(Buildset), maps:get(<<"class">>, Item) =:= Class]).
gap(Source, Reason, State) ->
    State#{excluded => maps:get(excluded, State) ++ [Source#{reason => Reason}]}.

conditions(Graph, Inputs) ->
    Conditions = maps:from_list([{maps:get(id, C), maps:without([effects, rank_scaled_cap], C)}
        || #{condition := C} <- maps:get(nodes, Graph), C =/= null]),
    Unknown = maps:keys(Inputs) -- maps:keys(Conditions),
    ensure(Unknown =:= [], {unavailable_effects, Unknown}),
    [begin
        Input = maps:get(Id, Inputs, 0),
        ensure(Input =< maps:get(max_stacks, C), {invalid_effect_stacks, Id}),
        ensure(Input =:= 0 orelse maps:get(enabled, C), {unmet_effect_requirement, Id}),
        ensure(not (maps:get(automatic, C, false) andalso maps:is_key(Id, Inputs)),
               {automatic_effect, Id}),
        Value = case maps:get(default, C, zero) =:= initial_combo andalso not maps:is_key(Id, Inputs) of
            true -> null;
            false -> count(C, selected, Inputs)
        end,
        C#{active_stacks => Value}
     end || {Id, C} <- lists:sort(maps:to_list(Conditions))].

evaluate(Graph, Mode, Inputs) ->
    lists:foldl(fun(Node, State) ->
        Result = evaluate_node(Node, Mode, Inputs, State),
        Effects = lists:foldl(fun(E = #{target := Target}, Acc) ->
            maps:update_with(Target, fun(Existing) -> Existing ++ [E] end, [E], Acc)
        end, maps:get(effects, State), maps:get(effects, Result, [])),
        State#{effects => Effects, nodes => (maps:get(nodes, State))#{maps:get(id, Node) => Result}}
    end, #{effects => #{}, nodes => #{}}, maps:get(nodes, Graph)).

evaluate_node(#{kind := effects, rule := Rule, rank := Rank, targets := Targets,
                condition := Condition}, Mode, Inputs, _State) ->
    Count = count(Condition, Mode, Inputs),
    Fixed = rank_effects(maps:get(effects, Rule), Rank, maps:get(max_rank, Rule)),
    Conditional = case Condition of
        null -> [];
        _ -> [E#{value => maps:get(value, E) * Count}
               || E <- rank_effects(maps:get(effects, Condition), Rank, maps:get(max_rank, Rule)), Count > 0]
    end,
    #{effects => [E#{target => Target} || Target <- Targets, E <- Fixed ++ Conditional],
      active_stacks => Count};
evaluate_node(#{kind := ability, owner := Owner, rule := Rule, rank := Rank,
                targets := Targets, condition := Condition}, Mode, Inputs, State) ->
    Multipliers = wfcli_stat_frames:multipliers(maps:get(Owner, maps:get(effects, State), [])),
    Result = wfcli_stat_abilities:evaluate(Rule, Rank, Multipliers, count(Condition, Mode, Inputs)),
    Result#{effects => [E#{target => Target} || Target <- Targets, E <- maps:get(effects, Result)]};
evaluate_node(#{kind := combo, owner := Owner, profile := Profile,
                condition := Condition}, Mode, Inputs, State) ->
    Effects = maps:get(Owner, maps:get(effects, State), []),
    Cap = maps:get(combo_cap, Profile),
    Initial = min(Cap, max(0, wfcli_stat_math:scalar(maps:get(initial_combo, Profile),
        [{Op, Value} || #{stat := initial_combo, operation := Op, value := Value} <- Effects]))),
    Count = case Mode of
        selected -> maps:get(maps:get(id, Condition), Inputs, Initial);
        upper -> Cap;
        _ -> Initial
    end,
    Tier = trunc(Count / maps:get(combo_step, Profile)),
    #{effects => [], initial_combo => Initial, combo => Count,
      multiplier => Tier + 1, outburst_multiplier => case Tier of 0 -> 0; _ -> Tier + 1 end};
evaluate_node(#{kind := outburst, rule := Rule, rank := Rank, targets := Targets,
                depends_on := [ComboId], condition := Condition}, Mode, Inputs, State) ->
    Combo = maps:get(ComboId, maps:get(nodes, State)),
    Active = count(Condition, Mode, Inputs),
    Bonus = Active * maps:get(outburst_multiplier, Combo) *
        lists:nth(Rank + 1, maps:get(values_by_rank, Rule)),
    #{active_stacks => Active, combo_multiplier => maps:get(multiplier, Combo),
      effects => [#{target => Target, stat => Stat, operation => stacking_multiply, value => Bonus}
                   || Target <- Targets, Stat <- [critical_chance, critical_multiplier]]}.

count(null, _, _) -> 0;
count(#{enabled := false}, _, _) -> 0;
count(#{automatic := true, max_stacks := Max}, _, _) -> Max;
count(#{max_stacks := Max}, upper, _) -> Max;
count(Condition, lower, _) -> maps:get(lower_stacks, Condition, 0);
count(#{id := Id}, selected, Inputs) -> maps:get(Id, Inputs, 0);
count(_, _, _) -> 0.

rank_effects(Effects, Rank, Max) ->
    [#{stat => Stat, operation => Op, value => wfcli_stat_math:ranked(Op, Value, Rank, Max)}
     || {Stat, Op, Value} <- Effects].
effect_tuple(#{stat := Stat, operation := Op, value := Value}) -> {Stat, Op, Value}.

dependencies(Graph, Buildset) ->
    Nodes = maps:get(nodes, Graph),
    #{mechanics => lists:usort([maps:get(core, Graph) |
                               [maps:get(mechanic, N) || N <- Nodes]]),
      properties => lists:usort([maps:get(<<"definition_id">>, I) || I <- maps:values(Buildset)] ++
                               [maps:get(item_type, N) || N <- Nodes])}.

public(Graph, States) ->
    [begin
        Base = maps:without([rule, profile, targets, condition], Node),
        RuleInfo = maps:merge(maps:with([outside_scope], maps:get(rule, Node, #{})),
                             maps:with([profile], Node)),
        Views = maps:map(fun(_, State) ->
            Result = maps:get(maps:get(id, Node), maps:get(nodes, State)),
            Result#{effects => [public_effect(E) || E <- maps:get(effects, Result)]}
        end, States),
        (maps:merge(Base, RuleInfo))#{states => Views,
             condition => case maps:get(condition, Node) of null -> null; C -> maps:get(id, C) end}
     end || Node <- maps:get(nodes, Graph)].

public_effect(Effect = #{stat := {element, Type}}) -> Effect#{stat => element, element => Type};
public_effect(Effect) -> Effect.
ensure(true, _) -> ok;
ensure(false, Reason) -> throw({stats, Reason}).

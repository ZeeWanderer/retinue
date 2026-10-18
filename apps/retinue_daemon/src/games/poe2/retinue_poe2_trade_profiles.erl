-module(retinue_poe2_trade_profiles).
-export([ranges/1, prepare/1, category/1, base_required/1]).

prepare(Item0) ->
    Category = maps:get(<<"category">>, Item0, category(maps:get(<<"item_class">>, Item0, null))),
    Item = Item0#{<<"category">> => Category},
    Rows = maps:get(<<"modifiers">>, Item),
    Groups = lists:foldl(fun(Row, Acc) ->
        Key = group(Row),
        Acc#{Key => maps:get(Key, Acc, []) ++ [Row]}
    end, #{}, Rows),
    Ranked = case {maps:get(<<"rarity">>, Item), Category} of
        {<<"Unique">>, _} -> [];
        {_, <<"map", _/binary>>} -> [];
        _ -> [{group(Row), score(Group, Category)} || Row <- Rows,
            Group <- [maps:get(group(Row), Groups)], hd(Group) =:= Row,
            rankable(Group)]
    end,
    {Properties, PropertyRows} = retinue_poe2_trade_properties:extract(Item),
    PropertyIds = [maps:get(<<"id">>, Row) || Row <- PropertyRows],
    Merged = maps:filter(fun(_, Targets) -> Targets =/= [] end, maps:map(fun(_, Group) ->
        case maps:get(<<"rarity">>, Item) =/= <<"Unique">> andalso
             lists:all(fun(Row) -> maps:get(<<"supported">>, Row, false) andalso
                 maps:get(<<"section">>, Row) =/= <<"fractured">> end, Group) of
            true -> lists:usort([Id || Row <- Group, Id <- retinue_poe2_trade_properties:targets(Row),
                                      lists:member(Id, PropertyIds)]);
            false -> []
        end
    end, Groups)),
    PropertyRanked = [{Id, lists:max(Scores)} || Id <- PropertyIds,
        Scores <- [[S || {Key, S} <- Ranked, lists:member(Id, maps:get(Key, Merged, []))]], Scores =/= []],
    Candidates = PropertyRanked ++ [{K, S} || {K, S} <- Ranked, not maps:is_key(K, Merged)],
    Sorted = lists:keysort(1, [{-Score, Key} || {Key, Score} <- Candidates, Score >= 3]),
    Selected = [Key || {_, Key} <- lists:sublist(Sorted, 4)],
    Scores = maps:from_list(Ranked ++ PropertyRanked),
    Prepared = [begin
        Supported = maps:get(<<"supported">>, Row, false),
        Property = maps:get(<<"selected">>, Row, false),
        Base = lists:member(maps:get(<<"section">>, Row), [<<"implicit">>, <<"fractured">>, <<"skill">>]),
        Quick = Property orelse lists:member(group(Row), Selected),
        Exact = case maps:get(<<"section">>, Row) of
            <<"property">> -> Quick;
            _ -> not maps:is_key(group(Row), Merged)
        end,
        Row1 = Row#{<<"quick_selected">> => Supported andalso
                      Quick, <<"exact_selected">> => Supported andalso Exact,
                    <<"base_selected">> => Supported andalso (Property orelse Base)},
        case maps:find(group(Row), Scores) of
            {ok, Score} -> Row1#{<<"ranking_score">> => Score};
            error -> Row1
        end
    end || Row <- PropertyRows ++ Rows],
    Profiles = case profile_category(Item) of
        base -> [<<"bases">>, <<"nomods">>, <<"fuzzy">>];
        nonunique -> [<<"nomods">>, <<"fuzzy">>];
        exact -> [<<"allmods">>, <<"fuzzy">>]
    end,
    Item#{<<"properties">> => Properties,
          <<"modifiers">> => retinue_poe2_trade_item:merge_modifiers(Prepared), <<"profiles">> => Profiles,
          <<"auto_search">> => auto_search(Item),
          <<"default_profile">> => hd(Profiles), <<"base_required">> => base_required(Item)}.

auto_search(Item) ->
    Equipment = case maps:get(<<"category">>, Item, null) of
        Category when is_binary(Category) ->
            lists:any(fun(Prefix) -> string:prefix(Category, Prefix) =/= nomatch end,
                [<<"weapon">>, <<"armour">>, <<"accessory">>, <<"jewel">>, <<"map">>]);
        _ -> true
    end,
    Rarity = maps:get(<<"rarity">>, Item),
    not maps:is_key(<<"trade_unavailable">>, Item) andalso
        not (Rarity =:= <<"Unique">> andalso maps:get(<<"unidentified">>, Item, false)) andalso
        (not Equipment orelse Rarity =:= <<"Unique">> orelse Rarity =:= <<"Gem">> orelse
         Rarity =:= <<"Currency">> orelse profile_category(Item) =:= base).

group(#{<<"decorator">> := #{<<"group">> := Group}, <<"section">> := Section}) -> {Section, Group};
group(#{<<"key">> := Key}) -> Key.

rankable(Rows) ->
    lists:all(fun(Row) ->
        maps:get(<<"supported">>, Row, false) andalso
        lists:member(maps:get(<<"section">>, Row), [<<"explicit">>, <<"desecrated">>, <<"crafted">>])
    end, Rows).

score([First | _] = Rows, Category) ->
    Tier = case maps:get(<<"tier_info">>, First, #{}) of
        #{<<"current">> := Current, <<"count">> := Count, <<"max">> := Max}
          when Count >= Current, Current >= Max, Max > 0 -> (Count - Current + 1) / (Count - Max + 1) * 2;
        _ -> 1
    end,
    Tags = maps:get(<<"tags">>, maps:get(<<"decorator">>, First, #{}), []),
    Weights = weights(Category),
    Ranked = lists:reverse(lists:sort([maps:get(Tag, Weights, 0) || Tag <- Tags])),
    TagScore = lists:sum([W * Factor || {W, Factor} <- lists:zip(
        lists:sublist(Ranked, 3), lists:sublist([0.75, 0.5, 0.25], min(length(Ranked), 3)))]),
    Single = case Ranked of [W] when W >= 2 -> 0.5; _ -> 0 end,
    Rolls = lists:flatmap(fun(R) -> maps:get(<<"magnitudes">>, R, []) end, Rows),
    Bonuses = lists:sum([bonus(Id) || R <- Rows, [Id | _] <- [maps:get(<<"internal_ids">>, R, [])]]),
    Tier + TagScore + Single + roll_score(Rolls) + Bonuses.

roll_score([]) -> 0;
roll_score(Rolls) ->
    {Value, Min, Max} = lists:foldl(fun(#{<<"value">> := V, <<"min">> := L, <<"max">> := H}, {A, B, C}) ->
        {A + V, B + L, C + H}
    end, {0, 0, 0}, Rolls),
    case Min == Max of true -> 0.25; false -> 0.5 * (Value - Min) / (Max - Min) end.

bonus(Id) when Id =:= <<"melee_skill_gem_level_+">>; Id =:= <<"projectile_skill_gem_level_+">>;
               Id =:= <<"trap_skill_gem_level_+">>; Id =:= <<"local_spirit">> -> 3;
bonus(Id) when Id =:= <<"local_spirit_+%">>; Id =:= <<"local_jewel_display_radius_change">>;
               Id =:= <<"base_item_found_rarity_+%">> -> 2;
bonus(Id) when Id =:= <<"local_energy_shield">>; Id =:= <<"local_energy_shield_+%">> -> 0.5;
bonus(<<"base_life_regeneration_rate_per_minute">>) -> -2;
bonus(_) -> 0.

profile_category(Item) ->
    Blocked = lists:any(fun(#{<<"text">> := Text}) ->
        lists:member(Text, [<<"Corrupted">>, <<"Twice Corrupted">>, <<"Mirrored">>,
                           <<"Unmodifiable">>, <<"Sanctified">>])
    end, maps:get(<<"conditions">>, Item, [])),
    Fractured = lists:any(fun(R) -> maps:get(<<"section">>, R) =:= <<"fractured">> end,
                           maps:get(<<"modifiers">>, Item)) orelse
                lists:member(#{<<"text">> => <<"Fractured Item">>}, maps:get(<<"properties">>, Item, [])),
    Base = maps:get(<<"rarity">>, Item) =:= <<"Normal">> orelse Fractured orelse
           high_quality(Item) orelse extra_sockets(Item),
    case {Blocked, Base, maps:get(<<"rarity">>, Item), maps:get(<<"category">>, Item)} of
        {false, true, _, _} -> base;
        {_, _, <<"Unique">>, _} -> exact;
        {_, _, _, <<"flask", _/binary>>} -> exact;
        {_, _, _, <<"sanctum.relic">>} -> exact;
        {true, _, _, Category} when Category =/= <<"map.waystone">> -> exact;
        _ -> nonunique
    end.

extra_sockets(Item) ->
    Category = maps:get(<<"category">>, Item, null),
    Maximum = case {maps:get(<<"name">>, Item), Category} of
        {<<"Darkness Enthroned">>, _} -> 2;
        {<<"Mahuxotl's Machination">>, _} -> 2;
        {<<"Morior Invictus">>, _} -> 4;
        {_, <<"armour.chest">>} -> 2;
        {_, <<"armour.quiver">>} -> 0;
        {_, <<"armour", _/binary>>} -> 1;
        {_, <<"weapon", _/binary>>} ->
            case lists:member(Category, [<<"weapon.bow">>, <<"weapon.staff">>, <<"weapon.twosword">>,
                    <<"weapon.twoaxe">>, <<"weapon.twomace">>, <<"weapon.warstaff">>,
                    <<"weapon.talisman">>, <<"weapon.rod">>, <<"weapon.crossbow">>]) of
                true -> 2;
                false -> 1
            end;
        _ -> 0
    end,
    lists:any(fun
        (#{<<"text">> := <<"Sockets: ", Sockets/binary>>}) ->
            Tokens = string:lexemes(Sockets, " -"),
            Tokens =/= [] andalso lists:all(fun(T) -> T =:= <<"S">> end, Tokens) andalso
                length(Tokens) > Maximum;
        (_) -> false
    end, maps:get(<<"properties">>, Item, [])).

high_quality(Item) ->
    Maximum = case [N || #{<<"section">> := <<"implicit">>,
        <<"internal_ids">> := [<<"local_maximum_quality_is_%">>], <<"min">> := N}
        <- maps:get(<<"modifiers">>, Item), is_number(N)] of
        [N | _] -> N;
        _ -> 20
    end,
    lists:any(fun(#{<<"text">> := Text}) ->
        case re:run(Text, <<"^Quality: \\+?([0-9]+)%">>, [{capture, [1], binary}]) of
            {match, [Value]} -> binary_to_integer(Value) > Maximum;
            _ -> false
        end
    end, maps:get(<<"properties">>, Item, [])).

base_required(Item) ->
    lists:member(maps:get(<<"rarity">>, Item), [<<"Unique">>, <<"Gem">>, <<"Currency">>, <<"Quest">>])
    orelse case maps:get(<<"category">>, Item, null) of
        <<"map", _/binary>> -> true;
        <<"flask", _/binary>> -> true;
        <<"sanctum.relic">> -> true;
        _ -> false
    end.

category(Class) ->
    Classes = [{<<"Amulet">>, <<"accessory.amulet">>}, {<<"Belt">>, <<"accessory.belt">>},
        {<<"Ring">>, <<"accessory.ring">>}, {<<"Body Armour">>, <<"armour.chest">>},
        {<<"Boots">>, <<"armour.boots">>}, {<<"Gloves">>, <<"armour.gloves">>},
        {<<"Helmet">>, <<"armour.helmet">>}, {<<"Shield">>, <<"armour.shield">>},
        {<<"Buckler">>, <<"armour.buckler">>}, {<<"Focus">>, <<"armour.focus">>},
        {<<"Quiver">>, <<"armour.quiver">>}, {<<"Jewel">>, <<"jewel">>},
        {<<"Bow">>, <<"weapon.bow">>}, {<<"Claw">>, <<"weapon.claw">>},
        {<<"Crossbow">>, <<"weapon.crossbow">>}, {<<"Dagger">>, <<"weapon.dagger">>},
        {<<"Flail">>, <<"weapon.flail">>}, {<<"Spear">>, <<"weapon.spear">>},
        {<<"One Hand Axe">>, <<"weapon.oneaxe">>}, {<<"One Hand Mace">>, <<"weapon.onemace">>},
        {<<"One Hand Sword">>, <<"weapon.onesword">>}, {<<"Two Hand Axe">>, <<"weapon.twoaxe">>},
        {<<"Two Hand Mace">>, <<"weapon.twomace">>}, {<<"Two Hand Sword">>, <<"weapon.twosword">>},
        {<<"Sceptre">>, <<"weapon.sceptre">>}, {<<"Wand">>, <<"weapon.wand">>},
        {<<"Staff">>, <<"weapon.staff">>}, {<<"Warstaff">>, <<"weapon.warstaff">>},
        {<<"Talisman">>, <<"weapon.talisman">>}, {<<"FishingRod">>, <<"weapon.rod">>},
        {<<"LifeFlask">>, <<"flask.life">>}, {<<"ManaFlask">>, <<"flask.mana">>},
        {<<"Charm">>, <<"flask.charm">>},
        {<<"UtilityFlask">>, <<"flask">>}, {<<"Relic">>, <<"sanctum.relic">>},
        {<<"Map">>, <<"map.waystone">>}, {<<"Waystone">>, <<"map.waystone">>},
        {<<"TowerAugmentation">>, <<"map.tablet">>}, {<<"Tablet">>, <<"map.tablet">>},
        {<<"Active Skill Gem">>, <<"gem.activegem">>}, {<<"Meta Skill Gem">>, <<"gem.activegem">>},
        {<<"Support Skill Gem">>, <<"gem.supportgem">>}],
    Aliases = [{<<Name/binary, "s">>, Value} || {Name, Value} <- Classes] ++ [
        {<<"Body Armours">>, <<"armour.chest">>}, {<<"Foci">>, <<"armour.focus">>},
        {<<"Staves">>, <<"weapon.staff">>}, {<<"Warstaves">>, <<"weapon.warstaff">>},
        {<<"Life Flasks">>, <<"flask.life">>}, {<<"Mana Flasks">>, <<"flask.mana">>},
        {<<"Skill Gems">>, <<"gem.activegem">>},
        {<<"Support Gems">>, <<"gem.supportgem">>}],
    proplists:get_value(Class, Classes ++ Aliases, null).

weights(Category) ->
    Group = case Category of
        <<"accessory.amulet">> -> [{<<"Gem">>,3},{<<"Damage">>,3},{<<"Critical">>,2},{<<"Resistance">>,2},
            {<<"Defences">>,2},{<<"Attribute">>,1},{<<"Mana">>,-1}];
        <<"accessory.ring">> -> [{<<"Resistance">>,3},{<<"Life">>,3},{<<"Attribute">>,2},{<<"Critical">>,2},
            {<<"Damage">>,2},{<<"Speed">>,1},{<<"Mana">>,-1},{<<"Minion">>,-2}];
        <<"armour.boots">> -> [{<<"Speed">>,3},{<<"Resistance">>,2},{<<"Life">>,2},{<<"Defences">>,2},
            {<<"Attribute">>,1},{<<"Mana">>,-1},{<<"Minion">>,-2},{<<"Caster">>,-2}];
        <<"armour.gloves">> -> [{<<"Damage">>,3},{<<"Attack">>,3},{<<"Elemental">>,2},{<<"Critical">>,2},
            {<<"Speed">>,2},{<<"Life">>,1},{<<"Resistance">>,1},{<<"Defences">>,1},{<<"Mana">>,-1},{<<"Minion">>,-2}];
        <<"armour.quiver">> -> [{<<"Damage">>,3},{<<"Critical">>,3},{<<"Attack">>,2},{<<"Elemental">>,2},
            {<<"Speed">>,2},{<<"Life">>,2},{<<"Resistance">>,1},{<<"Mana">>,-1},{<<"Minion">>,-2}];
        <<"jewel", _/binary>> -> [{<<"Damage">>,3},{<<"Critical">>,3},{<<"Resistance">>,2},{<<"Life">>,2},
            {<<"Attribute">>,2},{<<"Attack">>,1},{<<"Speed">>,1},{<<"Mana">>,-1},{<<"Minion">>,-1}];
        <<"weapon.sceptre">> -> [{<<"Minion">>,3},{<<"Aura">>,3},{<<"Damage">>,2},{<<"Resistance">>,2},
            {<<"Attribute">>,1},{<<"Mana">>,-1},{<<"Attack">>,-2}];
        C when C =:= <<"armour.shield">>; C =:= <<"armour.buckler">> ->
            [{<<"Defences">>,3},{<<"Resistance">>,3},{<<"Life">>,2},{<<"Attribute">>,2},
             {<<"Mana">>,1},{<<"Critical">>,-1},{<<"Attack">>,-1},{<<"Minion">>,-2}];
        C when C =:= <<"armour.focus">>; C =:= <<"weapon.dagger">>; C =:= <<"weapon.rod">>;
               C =:= <<"weapon.staff">>; C =:= <<"weapon.wand">>; C =:= <<"weapon.warstaff">> ->
            [{<<"Caster">>,3},{<<"Critical">>,3},{<<"Elemental">>,2},{<<"Life">>,1},
             {<<"Defences">>,1},{<<"Mana">>,-1},{<<"Attack">>,-2},{<<"Minion">>,-2}];
        <<"weapon.", _/binary>> -> [{<<"Damage">>,3},{<<"Critical">>,3},{<<"Attack">>,2},{<<"Elemental">>,2},
            {<<"Physical">>,2},{<<"Caster">>,2},{<<"Speed">>,2},{<<"Life">>,1},{<<"Mana">>,-1},
            {<<"Defences">>,-1},{<<"Minion">>,-2}];
        C when C =:= <<"accessory.belt">>; C =:= <<"armour.chest">>; C =:= <<"armour.helmet">>; C =:= <<"flask">> ->
            [{<<"Resistance">>,3},{<<"Attribute">>,2},{<<"Life">>,2},{<<"Defences">>,2},
             {<<"Mana">>,-1},{<<"Critical">>,-1},{<<"Minion">>,-2}];
        _ -> []
    end,
    maps:from_list(Group).

ranges(Row) ->
    #{<<"allmods">> => range(Row, 0), <<"fuzzy">> => range(Row, 0.1)}.

range(#{<<"supported">> := true, <<"id">> := Id, <<"min">> := Value,
        <<"section">> := <<"property">>}, Margin) when is_number(Value) ->
    case Id of
        <<"property.gem_level">> -> #{<<"min">> => floor(Value - abs(Value) * Margin)};
        <<"property.gem_sockets">> -> #{<<"min">> => Value, <<"max">> => Value};
        <<"property.quality">> -> #{<<"min">> => Value};
        _ -> case retinue_poe2_trade_properties:filter(Id) of
            undefined -> #{};
            _ ->
                Bound = Value - abs(Value) * Margin,
                #{<<"min">> => case {Margin, Value == trunc(Value)} of
                    {0, _} -> Value;
                    {_, true} -> floor(Bound);
                    {_, false} -> floor(Bound * 100) / 100
                end}
        end
    end;
range(#{<<"supported">> := true, <<"id">> := Id, <<"min">> := Value} = Row, Margin)
  when is_binary(Id), is_number(Value), Value /= 0 ->
    Inverse = inverse(Id, Value),
    Rolls = maps:get(<<"magnitudes">>, Row, []),
    Fixed = Rolls =/= [] andalso lists:all(fun
        (#{<<"min">> := Min, <<"max">> := Max}) -> Min == Max;
        (_) -> false
    end, Rolls),
    Bound = case {Fixed, Inverse} of
        {true, _} -> Value;
        {false, true} -> Value + abs(Value) * Margin;
        {false, false} -> Value - abs(Value) * Margin
    end,
    Key = case Inverse of true -> <<"max">>; false -> <<"min">> end,
    #{Key => round_modifier(Bound, Value)};
range(_, _) -> #{}.

round_modifier(Bound, Value) when trunc(Value) == Value -> floor(Bound + 0.5);
round_modifier(Bound, _) -> floor(Bound * 100 + 0.5) / 100.

inverse(Id, Value) ->
    lists:member(Id, [
        <<"explicit.stat_2448633171">>, <<"explicit.stat_1692879867">>,
        <<"explicit.stat_388617051">>, <<"sanctum.stat_3096446459">>,
        <<"enchant.stat_3639275092">>, <<"explicit.stat_3639275092">>,
        <<"explicit.stat_2590797182">>, <<"implicit.stat_2590797182">>,
        <<"rune.stat_2590797182">>, <<"explicit.stat_2267564181">>,
        <<"sanctum.stat_3554921410">>, <<"explicit.stat_2282052746">>,
        <<"explicit.stat_1345835998">>, <<"explicit.stat_1228222525">>,
        <<"explicit.stat_2905515354">>, <<"rune.stat_2905515354">>,
        <<"explicit.stat_3694078435">>, <<"explicit.stat_1224838456">>
    ]) orelse (Value < 0 andalso lists:member(Id, [
        <<"explicit.stat_1389754388">>, <<"implicit.stat_1389754388">>,
        <<"explicit.stat_1181501418">>, <<"implicit.stat_1181501418">>,
        <<"enchant.stat_3917489142">>, <<"implicit.stat_3917489142">>,
        <<"explicit.stat_3917489142">>, <<"pseudo.pseudo_increased_item_found_rarity">>,
        <<"enchant.stat_924253255">>, <<"implicit.stat_924253255">>,
        <<"explicit.stat_924253255">>, <<"explicit.stat_2580617872">>,
        <<"explicit.stat_3441651621">>
    ])) orelse (Value > 0 andalso Id =:= <<"explicit.stat_2146799605">>).

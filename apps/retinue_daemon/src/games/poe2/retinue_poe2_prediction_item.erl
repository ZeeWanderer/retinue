-module(retinue_poe2_prediction_item).
-export([build/1]).

build(Item) ->
    Rarity = maps:get(<<"rarity">>, Item),
    Category = maps:get(<<"category">>, Item,
        retinue_poe2_trade_profiles:category(maps:get(<<"item_class">>, Item, null))),
    Rows = maps:get(<<"modifiers">>, Item, []),
    Supported = lists:member(Rarity, [<<"Normal">>, <<"Magic">>, <<"Rare">>, <<"Unique">>])
        andalso is_binary(Category) andalso not maps:is_key(<<"trade_unavailable">>, Item)
        andalso lists:all(fun supported/1, Rows),
    case Supported of
        false -> undefined;
        true ->
            Base = maps:get(<<"base">>, Item),
            Groups = lists:foldl(fun(Row, Acc) ->
                Section = section(maps:get(<<"section">>, Row)),
                Acc#{Section => maps:get(Section, Acc, []) ++ [Row]}
            end, #{}, Rows),
            {Mods, Hashes, Texts} = maps:fold(fun(S, Group, {M, H, T}) ->
                Affixes = affixes([R || R <- Group, is_binary(maps:get(<<"id">>, R, null))]),
                Entries = [#{<<"magnitudes">> => lists:flatmap(fun magnitudes/1, Affix)} || Affix <- Affixes],
                Display = retinue_poe2_trade_item:merge_modifiers(Group),
                Hash = [[maps:get(<<"id">>, R, null),
                    [I || {I, Affix} <- lists:enumerate(0, Affixes),
                          lists:any(fun(A) -> maps:get(<<"id">>, A, null) =:= maps:get(<<"id">>, R, null) end, Affix)]]
                    || R <- Display, is_binary(maps:get(<<"id">>, R, null))],
                {M#{S => Entries}, H#{S => Hash},
                 T#{<<S/binary, "Mods">> => [clean(maps:get(<<"text">>, R)) || R <- Display]}}
            end, {#{}, #{}, #{}}, Groups),
            Flags = lists:foldl(fun(C, Acc) ->
                maps:merge(Acc, maps:with([<<"mirrored">>, <<"sanctified">>, <<"split">>],
                    maps:get(<<"filters">>, C, #{})))
            end, #{}, maps:get(<<"conditions">>, Item, [])),
            Payload = maps:merge(Texts, Flags),
            Payload#{<<"name">> => maps:get(<<"name">>, Item), <<"typeLine">> => Base,
                <<"baseType">> => Base, <<"type">> => Base, <<"nameAlt">> => <<>>,
                <<"frameType">> => maps:get(Rarity, #{<<"Normal">> => 0, <<"Magic">> => 1,
                    <<"Rare">> => 2, <<"Unique">> => 3}),
                <<"frameTypeId">> => Rarity, <<"rarity">> => Rarity, <<"category">> => Category,
                <<"identified">> => not maps:get(<<"unidentified">>, Item, false),
                <<"corrupted">> => maps:get(<<"corrupted">>, Item, false),
                <<"ilvl">> => maps:get(<<"item_level">>, Item, 0),
                <<"itemLevel">> => maps:get(<<"item_level">>, Item, 0),
                <<"properties">> => lists:filtermap(fun property/1, maps:get(<<"properties">>, Item)),
                <<"extended">> => #{<<"mods">> => Mods, <<"hashes">> => Hashes}}
    end.

supported(#{<<"section">> := S} = Row) ->
    lists:member(S, [<<"explicit">>, <<"implicit">>, <<"fractured">>,
        <<"crafted">>, <<"enchant">>, <<"augment">>, <<"desecrated">>]) andalso
        lists:all(fun retinue_poe2_trade_item:sane_number/1, maps:get(<<"display_values">>, Row, []));
supported(_) -> false.

affixes(Rows) ->
    {Order, Groups} = lists:foldl(fun(Row, {Keys, Acc}) ->
        Key = case Row of
            #{<<"decorator">> := #{<<"group">> := Group}} -> Group;
            _ -> maps:get(<<"key">>, Row)
        end,
        case maps:find(Key, Acc) of
            error -> {[Key | Keys], Acc#{Key => [Row]}};
            {ok, GroupRows} -> {Keys, Acc#{Key => GroupRows ++ [Row]}}
        end
    end, {[], #{}}, Rows),
    [maps:get(Key, Groups) || Key <- lists:reverse(Order)].

section(<<"augment">>) -> <<"rune">>;
section(S) -> S.

magnitudes(#{<<"id">> := Id} = Row) when is_binary(Id) ->
    Negated = maps:get(<<"negated">>, Row, false),
    Base = case Negated of
        true -> #{<<"hash">> => Id, <<"mod">> => <<"negate">>};
        false -> #{<<"hash">> => Id}
    end,
    case maps:get(<<"display_values">>, Row, []) of
        [_ | _] = Values ->
            [Base#{<<"value">> => case Negated of true -> -N; false -> N end} || N <- Values];
        [] -> [Base]
    end;
magnitudes(_) -> [].

clean(Text) ->
    re:replace(Text, <<"\\([+-]?[0-9]+(?:\\.[0-9]+)?(?:-[+-]?[0-9]+(?:\\.[0-9]+)?)?\\)">>,
        <<>>, [global, {return, binary}]).

property(#{<<"text">> := Text}) ->
    case binary:split(Text, <<": ">>) of
        [Label, Values] ->
            Types = #{<<"Quality">> => 6, <<"Physical Damage">> => 9,
                <<"Elemental Damage">> => 10, <<"Chaos Damage">> => 11,
                <<"Critical Hit Chance">> => 12, <<"Critical Strike Chance">> => 12,
                <<"Attacks per Second">> => 13, <<"Weapon Range">> => 14,
                <<"Block Chance">> => 15, <<"Block chance">> => 15, <<"Chance to Block">> => 15,
                <<"Armour">> => 16, <<"Evasion Rating">> => 17, <<"Energy Shield">> => 18,
                <<"Spirit">> => 25, <<"Ward">> => 54, <<"Reload Time">> => 97},
            case maps:find(Label, Types) of
                {ok, Type} ->
                    Clean = binary:replace(Values, <<" (augmented)">>, <<>>, [global]),
                    Colour = case Clean =:= Values of true -> 0; false -> 1 end,
                    {true, #{<<"type">> => Type, <<"displayMode">> => 0,
                        <<"values">> => [[string:trim(V), Colour]
                            || V <- binary:split(Clean, <<",">>, [global])]}};
                error -> false
            end;
        _ -> false
    end.

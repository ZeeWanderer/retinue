-module(retinue_poe2_trade_properties).
-export([extract/1, targets/1, filter/1]).

extract(Item) ->
    Properties = maps:get(<<"properties">>, Item, []),
    Parsed0 = [parse(P) || P <- Properties],
    Counts = lists:foldl(fun
        ({K, _, _}, Acc) -> maps:update_with(K, fun(N) -> N + 1 end, 1, Acc);
        (_, Acc) -> Acc
    end, #{}, Parsed0),
    Parsed = [case P of
        {Key, _, Original} when map_get(Key, Counts) > 1 -> Original;
        _ -> P
    end || P <- Parsed0],
    Quality = value(<<"quality">>, Parsed, 0),
    Maximum = maximum_quality(Item),
    Blocked = maps:get(<<"corrupted">>, Item, false) orelse lists:any(fun(C) ->
        lists:member(maps:get(<<"text">>, C), [<<"Corrupted">>, <<"Twice Corrupted">>,
            <<"Mirrored">>, <<"Unmodifiable">>, <<"Sanctified">>])
    end, maps:get(<<"conditions">>, Item, [])),
    QualityKnown = not invalid_property(<<"quality">>, Parsed),
    DamageKnown = not lists:any(fun(K) -> invalid_property(K, Parsed) end,
        [<<"physical">>, <<"elemental">>, <<"chaos">>, <<"aps">>]),
    Normalized = [case P of
        {Key, _, Original} when not DamageKnown,
            (Key =:= <<"physical">> orelse Key =:= <<"elemental">> orelse Key =:= <<"chaos">>) -> Original;
        {Key, N, Original} when not Blocked, QualityKnown, Quality < Maximum ->
            case lists:member(Key, [<<"physical">>, <<"ar">>, <<"ev">>, <<"es">>, <<"ward">>]) of
                true -> {Key, floor(N / (1 + Quality / 100) * (1 + Maximum / 100)), Original};
                false -> P
            end;
        _ -> P
    end || P <- Parsed],
    Aps = value(<<"aps">>, Parsed, 1),
    Damage = [{Key, round2(N * Aps)} || {Key, N, _} <- Normalized,
        lists:member(Key, [<<"physical">>, <<"elemental">>, <<"chaos">>])],
    Total = round2(lists:sum([N || {_, N} <- Damage])),
    {Plain, Rows, _} = lists:foldl(fun
        ({Key, _, _}, {P, R, First}) when Key =:= <<"physical">>; Key =:= <<"elemental">>; Key =:= <<"chaos">> ->
            N = proplists:get_value(Key, Damage),
            R1 = case First of true -> R ++ [row(<<"dps">>, <<"DPS">>, Total)]; false -> R end,
            case Key of
                <<"physical">> -> {P, R1 ++ [row(<<"pdps">>, <<"Physical DPS">>, N)], false};
                <<"elemental">> -> {P, R1 ++ [row(<<"edps">>, <<"Elemental DPS">>, N)], false};
                <<"chaos">> -> {P ++ [#{<<"text">> => <<"Chaos DPS: ", (number(N))/binary>>}], R1, false}
            end;
        ({Key, N, #{<<"text">> := Text}}, {P, R, First}) ->
            [Label | _] = binary:split(Text, <<":">>),
            Row0 = row(Key, Label, N),
            Row = case proplists:get_value(Key, [{K, V} || {K, V, _} <- Parsed]) of
                N -> Row0#{<<"text">> => binary:replace(Text, <<" (augmented)">>, <<>>)};
                _ -> Row0
            end,
            Selected = Key =:= <<"quality">> andalso N > Maximum,
            {P, R ++ [Row#{<<"selected">> => Selected}], First};
        (Original, {P, R, First}) -> {P ++ [Original], R, First}
    end, {[], [], true}, Normalized),
    {Plain, Rows}.

parse(#{<<"text">> := Text} = Property) ->
    case binary:split(Text, <<": ">>) of
        [Label, Values] ->
            case spec(Label) of
                undefined -> Property;
                Key ->
                    case numeric(Values) of
                        {ok, N} -> {Key, N, Property};
                        error -> Property
                    end
            end;
        _ -> Property
    end.

numeric(Text) ->
    try numeric_value(Text) catch error:_ -> error end.

numeric_value(Text) ->
    Clean = string:trim(binary:replace(Text, <<" (augmented)">>, <<>>)),
    case re:run(Clean, <<"^\\+?[0-9]+(?:\\.[0-9]+)?(?:-[0-9]+(?:\\.[0-9]+)?)?(?:, ?[0-9]+(?:\\.[0-9]+)?-[0-9]+(?:\\.[0-9]+)?)*%?$">>) of
        nomatch -> error;
        _ ->
            Parts = binary:split(binary:replace(Clean, <<"%">>, <<>>), <<",">>, [global]),
            N = lists:sum([begin
                Values = [to_number(string:trim(V)) || V <- binary:split(Part, <<"-">>, [global])],
                lists:sum(Values) / length(Values)
            end || Part <- Parts]),
            case retinue_poe2_trade_item:sane_number(N) of true -> {ok, N}; false -> error end
    end.

to_number(Text) ->
    case string:to_float(Text) of
        {N, <<>>} -> N;
        _ -> binary_to_integer(Text)
    end.

value(Key, Parsed, Default) ->
    case [N || {K, N, _} <- Parsed, K =:= Key] of [N] -> N; _ -> Default end.

invalid_property(Key, Parsed) ->
    lists:any(fun
        (#{<<"text">> := Text}) -> spec(hd(binary:split(Text, <<":">>))) =:= Key;
        (_) -> false
    end, Parsed).

maximum_quality(Item) ->
    case [N || #{<<"section">> := <<"implicit">>,
        <<"internal_ids">> := [<<"local_maximum_quality_is_%">>], <<"min">> := N}
        <- maps:get(<<"modifiers">>, Item), is_number(N), N >= 0] of
        [N | _] -> N;
        _ -> 20
    end.

row(Key, Label, N) ->
    Id = <<"property.", Key/binary>>,
    Suffix = case Key of <<"quality">> -> <<"%">>; <<"crit">> -> <<"%">>; <<"block">> -> <<"%">>; _ -> <<>> end,
    Row = #{<<"key">> => Id, <<"id">> => Id, <<"section">> => <<"property">>,
        <<"text">> => <<Label/binary, ": ", (number(N))/binary, Suffix/binary>>,
        <<"min">> => N, <<"max">> => N, <<"supported">> => true},
    Row#{<<"ranges">> => retinue_poe2_trade_profiles:ranges(Row)}.

number(N) when N == trunc(N) -> integer_to_binary(trunc(N));
number(N) -> float_to_binary(N, [short]).
round2(N) -> floor(N * 100 + 0.5) / 100.

spec(Label) -> proplists:get_value(Label, [
    {<<"Quality">>, <<"quality">>}, {<<"Physical Damage">>, <<"physical">>},
    {<<"Elemental Damage">>, <<"elemental">>}, {<<"Chaos Damage">>, <<"chaos">>},
    {<<"Attacks per Second">>, <<"aps">>}, {<<"Critical Hit Chance">>, <<"crit">>},
    {<<"Critical Strike Chance">>, <<"crit">>}, {<<"Reload Time">>, <<"reload_time">>},
    {<<"Armour">>, <<"ar">>}, {<<"Evasion Rating">>, <<"ev">>},
    {<<"Energy Shield">>, <<"es">>}, {<<"Ward">>, <<"ward">>},
    {<<"Block chance">>, <<"block">>}, {<<"Block Chance">>, <<"block">>}, {<<"Spirit">>, <<"spirit">>}]).

filter(<<"property.quality">>) -> {<<"type_filters">>, <<"quality">>};
filter(<<"property.", Key/binary>>) ->
    case lists:member(Key, [<<"dps">>, <<"pdps">>, <<"edps">>, <<"aps">>, <<"crit">>,
        <<"reload_time">>, <<"ar">>, <<"ev">>, <<"es">>, <<"ward">>, <<"block">>, <<"spirit">>]) of
        true -> {<<"equipment_filters">>, Key};
        false -> undefined
    end;
filter(_) -> undefined.

targets(Row) ->
    Ids = maps:get(<<"internal_ids">>, Row, []),
    Keys = case Ids of
        [<<"local_attack_speed_+%">>] -> [<<"aps">>];
        [<<"local_critical_strike_chance">>] -> [<<"crit">>];
        [<<"local_minimum_added_physical_damage">>, <<"local_maximum_added_physical_damage">>] -> [<<"pdps">>];
        [<<"local_physical_damage_+%">>, <<"local_weapon_no_physical_damage">>] -> [<<"pdps">>];
        [<<"local_minimum_added_fire_damage">>, <<"local_maximum_added_fire_damage">>] -> [<<"edps">>];
        [<<"local_minimum_added_cold_damage">>, <<"local_maximum_added_cold_damage">>] -> [<<"edps">>];
        [<<"local_minimum_added_lightning_damage">>, <<"local_maximum_added_lightning_damage">>] -> [<<"edps">>];
        [<<"local_base_physical_damage_reduction_rating">>] -> [<<"ar">>];
        [<<"local_physical_damage_reduction_rating_+%">>] -> [<<"ar">>];
        [<<"local_base_evasion_rating">>] -> [<<"ev">>];
        [<<"local_evasion_rating_+%">>] -> [<<"ev">>];
        [<<"local_energy_shield">>] -> [<<"es">>];
        [<<"local_energy_shield_+%">>] -> [<<"es">>];
        [<<"local_armour_and_evasion_+%">>] -> [<<"ar">>, <<"ev">>];
        [<<"local_armour_and_energy_shield_+%">>] -> [<<"ar">>, <<"es">>];
        [<<"local_evasion_and_energy_shield_+%">>] -> [<<"ev">>, <<"es">>];
        [<<"local_armour_and_evasion_and_energy_shield_+%">>] -> [<<"ar">>, <<"ev">>, <<"es">>];
        [<<"base_maximum_ward">>] -> [<<"ward">>];
        [<<"local_ward_+%">>] -> [<<"ward">>];
        [<<"local_spirit">>] -> [<<"spirit">>];
        [<<"local_spirit_+%">>] -> [<<"spirit">>];
        _ -> []
    end,
    [<<"property.", Key/binary>> || Key <- Keys].

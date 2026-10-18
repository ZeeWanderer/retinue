-module(retinue_poe2_properties_eunit).
-include_lib("eunit/include/eunit.hrl").

source_properties_test_() ->
    {ok, Bytes} = file:read_file("apps/retinue_cli/test/fixtures/poe2_trade_properties.json"),
    [{binary_to_list(maps:get(<<"name">>, Case)), fun() ->
        Item = prepare(Case),
        Rows = maps:get(<<"modifiers">>, Item),
        Expected = maps:get(<<"expected">>, Case),
        lists:foreach(fun(P) ->
            Name = maps:get(<<"name">>, P),
            Text = <<Name/binary, ": ", (maps:get(<<"value">>, P))/binary>>,
            case maps:get(<<"quick">>, P) of
                null -> ?assert(lists:member(#{<<"text">> => Text}, maps:get(<<"properties">>, Item)));
                _ ->
                    [Row] = [R || R <- Rows, maps:get(<<"text">>, R) =:= Text],
                    ?assertEqual(maps:get(<<"quick">>, P), maps:get(<<"quick_selected">>, Row)),
                    ?assertEqual(maps:get(<<"exact">>, P), maps:get(<<"exact_selected">>, Row)),
                    ?assertEqual(maps:get(<<"score">>, P), maps:get(<<"ranking_score">>, Row, null)),
                    Ranges = maps:get(<<"ranges">>, Row),
                    ?assert(abs(maps:get(<<"min">>, P) - maps:get(<<"min">>, maps:get(<<"allmods">>, Ranges))) < 1.0e-8),
                    ?assert(abs(maps:get(<<"broad_min">>, P) - maps:get(<<"min">>, maps:get(<<"fuzzy">>, Ranges))) < 1.0e-8)
            end
        end, maps:get(<<"properties">>, Expected)),
        lists:foreach(fun({Name, Key}) ->
            Selected = [maps:get(<<"key">>, R) || R <- Rows, maps:get(<<"section">>, R) =/= <<"property">>,
                        maps:get(Key, R)],
            ?assertEqual(maps:get(Name, Expected), Selected)
        end, [{<<"quick">>, <<"quick_selected">>}, {<<"exact">>, <<"exact_selected">>}])
    end} || Case <- json:decode(Bytes)].

property_queries_test() ->
    Item = retinue_poe2_trade_profiles:prepare((item())#{<<"properties">> => properties([
        <<"Quality: +20%">>, <<"Physical Damage: 100-200">>, <<"Attacks per Second: 1.5">>,
        <<"Armour: 123">>])}),
    Request = #{<<"league">> => <<"Standard">>, <<"filters">> => [
        #{<<"id">> => <<"property.pdps">>, <<"min">> => 220},
        #{<<"id">> => <<"property.ar">>, <<"max">> => 200},
        #{<<"id">> => <<"property.quality">>, <<"min">> => 20}]},
    {ok, #{<<"query">> := Query}} = query(Request, Item),
    ?assertMatch(#{<<"stats">> := [#{<<"filters">> := []}], <<"filters">> := #{
        <<"equipment_filters">> := #{<<"filters">> := #{
            <<"pdps">> := #{<<"min">> := 220}, <<"ar">> := #{<<"max">> := 200}}},
        <<"type_filters">> := #{<<"filters">> := #{<<"quality">> := #{<<"min">> := 20}}}}}, Query),
    ?assertMatch({error, #{<<"reason">> := <<"unsupported_filter">>}},
        query(Request#{<<"filters">> => [#{<<"id">> => <<"property.edps">>, <<"min">> => 1}]}, Item)).

clipboard_properties_test() ->
    {ok, Text} = file:read_file("apps/retinue_cli/test/fixtures/poe2_property_weapon.txt"),
    {ok, Catalog} = retinue_poe2_trade_item:catalog(retinue_poe2_trade_eunit:fixture()),
    {ok, Parsed} = retinue_poe2_trade_item:parse(Text, Catalog),
    #{<<"modifiers">> := Rows} = retinue_poe2_trade_profiles:prepare(Parsed),
    ?assertMatch([#{<<"id">> := <<"property.quality">>, <<"min">> := 10.0},
                  #{<<"id">> := <<"property.dps">>, <<"min">> := 319.5},
                  #{<<"id">> := <<"property.pdps">>, <<"min">> := 244.5},
                  #{<<"id">> := <<"property.edps">>, <<"min">> := 75.0},
                  #{<<"id">> := <<"property.aps">>}, #{<<"id">> := <<"property.crit">>},
                  #{<<"id">> := <<"explicit.stat_3299347043">>}], Rows).

invalid_properties_preserve_text_test() ->
    lists:foreach(fun(Invalid) ->
        Props = properties([<<"Physical Damage: 100-200">>, Invalid]),
        #{<<"modifiers">> := Rows, <<"properties">> := Plain} =
            retinue_poe2_trade_profiles:prepare((item())#{<<"properties">> => Props}),
        ?assertEqual([], Rows),
        ?assertEqual(Props, Plain)
    end, [<<"Attacks per Second: ???">>, <<"Elemental Damage: Unknown">>,
          <<"Physical Damage: 200-300">>, <<"Attacks per Second: ", (binary:copy(<<"9">>, 1000))/binary>>]).

unknown_stat_does_not_hide_local_row_test() ->
    Row = row(#{<<"key">> => <<"unknown">>, <<"internal_ids">> => [<<"new_local_damage">>],
                <<"decorator">> => #{<<"tags">> => [<<"Damage">>]}}),
    Item = retinue_poe2_trade_profiles:prepare((item())#{
        <<"properties">> => properties([<<"Physical Damage: 100-200">>]), <<"modifiers">> => [Row]}),
    [Actual] = [R || #{<<"key">> := <<"unknown">>} = R <- maps:get(<<"modifiers">>, Item)],
    ?assertMatch(#{<<"quick_selected">> := true, <<"exact_selected">> := true}, Actual),
    ?assert(lists:all(fun(R) -> not maps:get(<<"quick_selected">>, R) end,
        [R || R <- maps:get(<<"modifiers">>, Item), maps:get(<<"section">>, R) =:= <<"property">>])).

prepare(Case) ->
    Props = [#{<<"text">> => <<(maps:get(<<"name">>, P))/binary, ": ",
        (iolist_to_binary(lists:join(<<", ">>, maps:get(<<"values">>, P))))/binary>>}
        || P <- maps:get(<<"properties">>, Case)],
    retinue_poe2_trade_profiles:prepare(maps:merge(item(), Case#{<<"properties">> => Props,
        <<"modifiers">> => [row(R) || R <- maps:get(<<"rows">>, Case)]})).

row(R) ->
    Key = maps:get(<<"key">>, R),
    maps:merge(#{<<"id">> => <<"explicit.", Key/binary>>, <<"text">> => Key,
        <<"supported">> => true, <<"section">> => <<"explicit">>, <<"min">> => 10,
        <<"magnitudes">> => [#{<<"min">> => 10, <<"max">> => 10, <<"value">> => 10}]}, R).

item() -> #{<<"rarity">> => <<"Rare">>, <<"name">> => <<"Test Bow">>, <<"base">> => <<"Test Bow">>,
    <<"category">> => <<"weapon.bow">>, <<"properties">> => [], <<"modifiers">> => [], <<"conditions">> => []}.

properties(Texts) -> [#{<<"text">> => Text} || Text <- Texts].

query(Request, Item) ->
    {ok, Bytes} = file:read_file("apps/retinue_cli/test/fixtures/poe2_trade_filters.json"),
    {ok, Options} = retinue_poe2_trade_query:options(json:decode(Bytes)),
    retinue_poe2_trade_query:build(Request, Item, [#{<<"id">> => <<"Standard">>}], Options).

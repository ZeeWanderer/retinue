%%%-------------------------------------------------------------------
%% EUnit tests for data extraction helpers.
%%%-------------------------------------------------------------------
-module(retinue_data_extract_eunit).

-include_lib("eunit/include/eunit.hrl").

extract_by_key_and_index_test() ->
    Data = #{<<"Foo">> => #{<<"Bar">> => [1, 2, 3]}},
    ?assertEqual([2], retinue_data_extract:extract_values(Data, "Foo.Bar.1")).

extract_wildcard_test() ->
    Data = #{<<"Map">> => #{<<"a">> => 1, <<"b">> => 2}},
    Values = retinue_data_extract:extract_values(Data, "Map.*"),
    ?assertEqual([1, 2], lists:sort(Values)).

extract_list_maps_test() ->
    Data = #{<<"List">> => [#{<<"Val">> => "a"}, #{<<"Val">> => "b"}]},
    ?assertEqual(["a", "b"], retinue_data_extract:extract_values(Data, "List.Val")).

extract_string_test() ->
    Data = #{<<"List">> => [#{<<"Val">> => "a"}, #{<<"Val">> => "b"}]},
    ?assertEqual("a, b", retinue_data_extract:extract_string(Data, "List.Val")).

extract_missing_key_test() ->
    Data = #{<<"Foo">> => 1},
    ?assertEqual([], retinue_data_extract:extract_values(Data, "Bar")).

structured_extraction_is_printable_test() ->
    Values = [#{<<"name">> => <<"Garuda">>},
              [#{<<"Choices">> => [<<"Garuda">>, <<"Baruuk">>]}],
              [[1, 2], #{<<"number">> => 3}]],
    lists:foreach(fun(Value) ->
        Data = #{<<"value">> => Value},
        Text = retinue_data_extract:extract_string(Data, "value"),
        ?assert(is_binary(unicode:characters_to_binary(Text))),
        ?assertEqual(Text, retinue_table_text:sanitize(Text)),
        ?assertEqual([Value], retinue_data_extract:extract_values(Data, "value"))
    end, Values).

extract_index_out_of_range_test() ->
    Data = #{<<"Foo">> => [1, 2]},
    ?assertEqual([], retinue_data_extract:extract_values(Data, "Foo.9")).

parse_path_test() ->
    ?assertEqual([{key, "Foo"}, {index, 2}, wildcard], retinue_data_extract:parse_path("Foo.2.*")).

parse_path_skips_empty_segments_test() ->
    ?assertEqual([{key, "Foo"}, {key, "Bar"}], retinue_data_extract:parse_path("Foo..Bar")).

-module(wfcli_catalog_json_eunit).
-include_lib("eunit/include/eunit.hrl").

catalog_strings_and_arrays_are_explicit_test() ->
    Stats = [#{<<"stats">> => [], <<"values">> => [65, 66]}],
    Mod = #{name => [16#03BB], compatName => "", baseDrain => undefined,
            description => [], effects => [], max_stats => ["+10%"], levelStats => Stats},
    Json = roundtrip(wfcli_catalog_json:record(mod, Mod)),
    ?assertEqual(unicode:characters_to_binary([16#03BB]), maps:get(<<"name">>, Json)),
    ?assertEqual(<<>>, maps:get(<<"compatName">>, Json)),
    ?assertEqual(null, maps:get(<<"baseDrain">>, Json)),
    ?assertEqual([], maps:get(<<"description">>, Json)),
    ?assertEqual([], maps:get(<<"effects">>, Json)),
    ?assertEqual([<<"+10%">>], maps:get(<<"max_stats">>, Json)),
    ?assertEqual(Stats, maps:get(<<"levelStats">>, Json)).

item_and_knowledge_arrays_remain_arrays_test() ->
    Item = roundtrip(wfcli_catalog_json:record(item,
                     #{name => "Test", description => "", abilities => [], sourceFiles => []})),
    ?assertEqual(<<>>, maps:get(<<"description">>, Item)),
    ?assertEqual([], maps:get(<<"abilities">>, Item)),
    Drops = [#{<<"location">> => <<"Test">>, <<"values">> => [65, 66]}],
    Enemy = roundtrip(wfcli_catalog_json:record(enemy, #{drops => Drops})),
    ?assertEqual(Drops, maps:get(<<"drops">>, Enemy)).

roundtrip(Value) -> jsone:decode(jsone:encode(Value)).

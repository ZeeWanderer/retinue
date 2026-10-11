-module(retinue_output_eunit).
-include_lib("eunit/include/eunit.hrl").

scoped_options_restore_after_error_test() ->
    ?assertEqual(#{}, retinue_output:options()),
    retinue_output:with(#{utc => true}, fun() ->
        ?assertError(test, retinue_output:with(#{output_format => json}, fun() -> error(test) end)),
        ?assertEqual(#{utc => true}, retinue_output:options())
    end),
    ?assertEqual(#{}, retinue_output:options()).

json_preserves_raw_arrays_and_types_test() ->
    Raw = #{<<"numbers">> => [65, 66, 67], <<"empty">> => [],
            <<"nested">> => [[100]], <<"name">> => <<"test">>},
    Entry = #{type => incidents, name => "event", haystack => "search text",
              data => Raw#{<<"timestamp">> => 1234}},
    {ok, Json} = retinue_json:decode(retinue_json:encode(retinue_output:entity(Entry))),
    ?assertEqual(<<"1970-01-01T00:00:01.234Z">>, maps:get(<<"timestamp">>, Json)),
    ?assertEqual(Raw#{<<"timestamp">> => 1234}, maps:get(<<"data">>, Json)),
    ?assertNot(maps:is_key(<<"haystack">>, Json)).

worldstate_extract_json_preserves_numbers_test() ->
    Result = #{entries => [#{id => "id", name => "name", data => #{<<"values">> => [65, 66]}}],
               opts => #{}, parsed_query => #{extracts => ["values.*"]}},
    {ok, #{<<"entries">> := [Entry]} = Json} = retinue_json:decode(
        retinue_json:encode(wf_worldstate_output:json_result(Result))),
    ?assertEqual(#{<<"values.*">> => [65, 66]}, maps:get(<<"extracts">>, Entry)),
    ?assertNot(maps:is_key(<<"opts">>, Json)).

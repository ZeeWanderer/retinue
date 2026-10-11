%%%-------------------------------------------------------------------
%% EUnit tests for text normalization helpers.
%%%-------------------------------------------------------------------
-module(wfcli_text_eunit).

-include_lib("eunit/include/eunit.hrl").

to_list_binary_test() ->
    ?assertEqual("test", retinue_text:to_list(<<"test">>)).

to_list_atom_test() ->
    ?assertEqual("alpha", retinue_text:to_list(alpha)).

to_list_list_test() ->
    ?assertEqual("already", retinue_text:to_list("already")).

to_list_number_test() ->
    ?assertEqual("42", retinue_text:to_list(42)).

to_list_float_test() ->
    ?assertEqual("3.14", retinue_text:to_list(3.14)).

value_present_test() ->
    ?assertEqual(false, retinue_text:value_present(undefined)),
    ?assertEqual(false, retinue_text:value_present([])),
    ?assertEqual(false, retinue_text:value_present(<<>>)),
    ?assertEqual(false, retinue_text:value_present(null)),
    ?assertEqual(true, retinue_text:value_present("x")).

join_list_test() ->
    ?assertEqual("a, b", retinue_text:join_list(["a", "", "b"], ", ")).

join_list_predicate_test() ->
    Pred = fun(V) -> V =:= 2 end,
    ?assertEqual("2", retinue_text:join_list([1, 2, 3], ", ", Pred)).

join_parts_test() ->
    ?assertEqual("a | b", retinue_text:join_parts(["a", "", "b"], " | ")).

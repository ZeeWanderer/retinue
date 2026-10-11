%%%-------------------------------------------------------------------
%% Common Test for text normalization helpers.
%%%-------------------------------------------------------------------
-module(wfcli_text_SUITE).

-export([all/0,
         to_list_binary/1,
         to_list_utf8_binary/1,
         to_list_atom/1,
         to_list_list/1,
         to_list_number/1,
         to_list_float/1,
         value_present/1,
         join_list/1,
         join_list_predicate/1,
         join_parts/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [to_list_binary,
     to_list_utf8_binary,
     to_list_atom,
     to_list_list,
     to_list_number,
     to_list_float,
     value_present,
     join_list,
     join_list_predicate,
     join_parts].

to_list_binary(_Config) ->
    ?assertEqual("test", retinue_text:to_list(<<"test">>)).

to_list_utf8_binary(_Config) ->
    Bin = <<"CALCOMAN", 16#C3, 16#8D, "AS">>,
    Expected = [$C, $A, $L, $C, $O, $M, $A, $N, 16#00CD, $A, $S],
    ?assertEqual(Expected, retinue_text:to_list(Bin)).

to_list_atom(_Config) ->
    ?assertEqual("alpha", retinue_text:to_list(alpha)).

to_list_list(_Config) ->
    ?assertEqual("already", retinue_text:to_list("already")).

to_list_number(_Config) ->
    ?assertEqual("42", retinue_text:to_list(42)).

to_list_float(_Config) ->
    ?assertEqual("3.14", retinue_text:to_list(3.14)).

value_present(_Config) ->
    ?assertEqual(false, retinue_text:value_present(undefined)),
    ?assertEqual(false, retinue_text:value_present([])),
    ?assertEqual(false, retinue_text:value_present(<<>>)),
    ?assertEqual(false, retinue_text:value_present(null)),
    ?assertEqual(true, retinue_text:value_present("x")).

join_list(_Config) ->
    ?assertEqual("a, b", retinue_text:join_list(["a", "", "b"], ", ")).

join_list_predicate(_Config) ->
    Pred = fun(V) -> V =:= 2 end,
    ?assertEqual("2", retinue_text:join_list([1, 2, 3], ", ", Pred)).

join_parts(_Config) ->
    ?assertEqual("a | b", retinue_text:join_parts(["a", "", "b"], " | ")).

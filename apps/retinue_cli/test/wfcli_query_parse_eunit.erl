%%%-------------------------------------------------------------------
%% EUnit tests for query parsing helpers.
%%%-------------------------------------------------------------------
-module(wfcli_query_parse_eunit).

-include_lib("eunit/include/eunit.hrl").

parse_op_equals_test() ->
    {ok, Key, Op, Val} = wfcli_query_parse:parse_op("type=alert"),
    ?assertEqual("type", Key),
    ?assertEqual(eq, Op),
    ?assertEqual("alert", Val).

parse_op_contains_test() ->
    {ok, Key, Op, Val} = wfcli_query_parse:parse_op("name~heat"),
    ?assertEqual("name", Key),
    ?assertEqual(contains, Op),
    ?assertEqual("heat", Val).

parse_op_default_test() ->
    {ok, Key, Op, Val} = wfcli_query_parse:parse_op("rarity:rare"),
    ?assertEqual("rarity", Key),
    ?assertEqual(default, Op),
    ?assertEqual("rare", Val).

split_vals_test() ->
    ?assertEqual(["A", "B"], wfcli_query_parse:split_vals("A|B")).

implicit_and_precedes_or_test() ->
    {ok, Ast} = wfcli_query_parse:parse("alpha beta OR gamma"),
    ?assertEqual({'or', [{'and', [{term, "alpha"}, {term, "beta"}]},
                         {term, "gamma"}]}, Ast).

parentheses_override_precedence_test() ->
    {ok, Ast} = wfcli_query_parse:parse("(alpha OR beta) gamma"),
    ?assertEqual({'and', [{'or', [{term, "alpha"}, {term, "beta"}]},
                          {term, "gamma"}]}, Ast).

not_is_unary_and_implicit_test() ->
    {ok, Ast} = wfcli_query_parse:parse("alpha NOT beta"),
    ?assertEqual({'and', [{term, "alpha"}, {'not', {term, "beta"}}]}, Ast).

quoted_phrase_and_escape_test() ->
    {ok, Ast} = wfcli_query_parse:parse("\"name = \\\"Lotus\\\"\""),
    ?assertEqual({term, "name = \"Lotus\""}, Ast).

quoted_pipe_is_literal_filter_value_test() ->
    {ok, Ast} = wfcli_query_parse:parse("name=\"A|B\""),
    ?assertEqual({filter, "name", eq, ["A|B"]}, Ast).

filter_pipe_remains_value_or_test() ->
    {ok, Ast} = wfcli_query_parse:parse("rarity=rare|uncommon"),
    ?assertEqual({filter, "rarity", eq, ["rare", "uncommon"]}, Ast).

argument_phrase_compatibility_test() ->
    {ok, Ast} = wfcli_query_parse:parse_arguments(["critical chance"]),
    ?assertEqual({term, "critical chance"}, Ast).

argument_filter_value_compatibility_test() ->
    {ok, Ast} = wfcli_query_parse:parse_arguments(["abilities~test ability"]),
    ?assertEqual({filter, "abilities", contains, ["test ability"]}, Ast).

argument_boolean_expression_test() ->
    {ok, Ast} = wfcli_query_parse:parse_arguments(["alpha OR beta"]),
    ?assertEqual({'or', [{term, "alpha"}, {term, "beta"}]}, Ast).

sort_control_must_be_top_level_test() ->
    {ok, Ast} = wfcli_query_parse:parse("alpha OR sort=name"),
    ?assertMatch({error, _}, wfcli_query_parse:extract_control(Ast, sort)).

malformed_expression_errors_test() ->
    ?assertMatch({error, _}, wfcli_query_parse:parse("alpha OR")),
    ?assertMatch({error, _}, wfcli_query_parse:parse("(alpha OR beta")),
    ?assertMatch({error, _}, wfcli_query_parse:parse("\"alpha")).

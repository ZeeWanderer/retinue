%%%-------------------------------------------------------------------
%% EUnit tests for shared entity query compilation and execution.
%%%-------------------------------------------------------------------
-module(wfcli_entity_query_eunit).

-include_lib("eunit/include/eunit.hrl").

boolean_precedence_matches_entities_test() ->
    Alpha = item("Alpha Rifle", 5),
    Beta = item("Beta Rifle", 3),
    Ast = compiled("alpha OR beta gamma"),
    ?assert(wfcli_entity_query:match(Alpha, Ast, wfcli_entity_exports, item)),
    ?assert(not wfcli_entity_query:match(Beta, Ast, wfcli_entity_exports, item)).

parentheses_and_not_match_entities_test() ->
    Alpha = item("Alpha Rifle", 5),
    Ast = compiled("(alpha OR beta) NOT shotgun"),
    ?assert(wfcli_entity_query:match(Alpha, Ast, wfcli_entity_exports, item)).

numeric_filter_is_compiled_once_test() ->
    Ast = compiled("masteryReq>=5"),
    ?assert(wfcli_entity_query:match(item("Five", 5), Ast, wfcli_entity_exports, item)),
    ?assert(not wfcli_entity_query:match(item("Four", 4), Ast, wfcli_entity_exports, item)).

unknown_field_is_schema_error_test() ->
    {ok, Ast} = wfcli_query_parse:parse("faction=Corpus"),
    ?assertMatch({error, [_]}, wfcli_entity_query:compile(Ast, wfcli_entity_exports, item)).

numeric_sort_and_paging_test() ->
    Entries = [item("Five", 5), item("Two", 2), item("Ten", 10)],
    {ok, Sorts} = wfcli_entity_query:compile_sorts(
      [#{key => masteryReq, dir => desc}], wfcli_entity_exports, item),
    Result = wfcli_entity_query:execute(
      Entries, match_all, Sorts, wfcli_entity_exports, item, 1, 1),
    [Entry] = maps:get(slice, Result),
    ?assertEqual("Five", maps:get(name, Entry)).

unlimited_page_returns_every_match_test() ->
    Entries = [item("One", 1), item("Two", 2), item("Three", 3)],
    Result = wfcli_entity_query:execute(
      Entries, match_all, [], wfcli_entity_exports, item, 0, infinity),
    ?assertEqual(3, maps:get(shown, Result)),
    ?assertEqual(Entries, maps:get(slice, Result)).

compiled(Query) ->
    {ok, Ast} = wfcli_query_parse:parse(Query),
    {ok, Compiled} = wfcli_entity_query:compile(Ast, wfcli_entity_exports, item),
    Compiled.

item(Name, Mastery) ->
    wfcli_entity_exports:build_item(
      #{name => Name, uniqueName => "/Test/" ++ Name,
        file => "ExportWeapons_en.json", masteryReq => Mastery,
        productCategory => "Primary"}, #{}).

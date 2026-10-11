%%%-------------------------------------------------------------------
%% EUnit tests for shared entity query compilation and execution.
%%%-------------------------------------------------------------------
-module(wf_entity_query_eunit).

-include_lib("eunit/include/eunit.hrl").

boolean_precedence_matches_entities_test() ->
    Alpha = item("Alpha Rifle", 5),
    Beta = item("Beta Rifle", 3),
    Ast = compiled("alpha OR beta gamma"),
    ?assert(wf_entity_query:match(Alpha, Ast, wf_entity_exports, item)),
    ?assert(not wf_entity_query:match(Beta, Ast, wf_entity_exports, item)).

parentheses_and_not_match_entities_test() ->
    Alpha = item("Alpha Rifle", 5),
    Ast = compiled("(alpha OR beta) NOT shotgun"),
    ?assert(wf_entity_query:match(Alpha, Ast, wf_entity_exports, item)).

numeric_filter_is_compiled_once_test() ->
    Ast = compiled("masteryReq>=5"),
    ?assert(wf_entity_query:match(item("Five", 5), Ast, wf_entity_exports, item)),
    ?assert(not wf_entity_query:match(item("Four", 4), Ast, wf_entity_exports, item)).

unknown_field_is_schema_error_test() ->
    {ok, Ast} = wf_query_parse:parse("faction=Corpus"),
    ?assertMatch({error, [_]}, wf_entity_query:compile(Ast, wf_entity_exports, item)).

numeric_sort_and_paging_test() ->
    Entries = [item("Five", 5), item("Two", 2), item("Ten", 10)],
    {ok, Sorts} = wf_entity_query:compile_sorts(
      [#{key => masteryReq, dir => desc}], wf_entity_exports, item),
    Result = wf_entity_query:execute(
      Entries, match_all, Sorts, wf_entity_exports, item, 1, 1),
    [Entry] = maps:get(slice, Result),
    ?assertEqual("Five", maps:get(name, Entry)).

unlimited_page_returns_every_match_test() ->
    Entries = [item("One", 1), item("Two", 2), item("Three", 3)],
    Result = wf_entity_query:execute(
      Entries, match_all, [], wf_entity_exports, item, 0, infinity),
    ?assertEqual(3, maps:get(shown, Result)),
    ?assertEqual(Entries, maps:get(slice, Result)).

compiled(Query) ->
    {ok, Ast} = wf_query_parse:parse(Query),
    {ok, Compiled} = wf_entity_query:compile(Ast, wf_entity_exports, item),
    Compiled.

item(Name, Mastery) ->
    wf_entity_exports:build_item(
      #{name => Name, uniqueName => "/Test/" ++ Name,
        file => "ExportWeapons_en.json", masteryReq => Mastery,
        productCategory => "Primary"}, #{}).

%%%-------------------------------------------------------------------
%% Unified-query adapter for daemon-owned player data.
%%%-------------------------------------------------------------------
-module(wfcli_player_query).

-export([execute/3]).

-doc "Compile and execute existing query AST against canonical player entities.".
-spec execute(term(), map(), map()) -> {ok, map()} | {error, term()}.
execute(Ast0, Request, Snapshot) ->
    case wfcli_query_parse:extract_control(Ast0, view) of
        {ok, Ast1, ViewControls} ->
            case compile_view(ViewControls) of
                {ok, View} -> execute_sorted(Ast1, View, Request, Snapshot);
                {error, Error} -> {error, {query_errors, [Error]}}
            end;
        {error, Error} -> {error, {query_errors, [Error]}}
    end.

execute_sorted(Ast0, View, Request, Snapshot) ->
    case wfcli_query_parse:extract_control(Ast0, sort) of
        {ok, Ast, SortControls} ->
            case compile_sorts(SortControls) of
                {ok, Sorts0} ->
                    Sorts = case Sorts0 of
                        [] -> wfcli_entity_player:default_sort(player);
                        _ -> Sorts0
                    end,
                    run(Ast, Sorts, View, Request, Snapshot);
                {error, Error} -> {error, {query_errors, [Error]}}
            end;
        {error, Error} -> {error, {query_errors, [Error]}}
    end.

run(Ast, Sorts, View, Request, Snapshot) ->
    case {wfcli_entity_query:compile(Ast, wfcli_entity_player, player),
          wfcli_entity_query:compile_sorts(Sorts, wfcli_entity_player, player)} of
        {{ok, Compiled}, {ok, CompiledSorts}} ->
            Entries = wfcli_entity_player:build_entries(
                        Snapshot, #{raw => maps:get(raw, Request, false), view => View}),
            Result = wfcli_entity_query:execute(
                       Entries, Compiled, CompiledSorts, wfcli_entity_player, player,
                       maps:get(offset, Request, 0), maps:get(limit, Request, infinity)),
            Query = #{query => Compiled, compiled_sort => CompiledSorts,
                      output_format => maps:get(output_format, Request, table),
                      raw => maps:get(raw, Request, false), view => View},
            {ok, #{query => Query,
                   results => Result#{revision => maps:get(revision, Snapshot),
                                     updated_at => maps:get(updated_at, Snapshot)}}};
        {{error, Errors}, _} -> {error, {query_errors, Errors}};
        {_, {error, Errors}} -> {error, {query_errors, Errors}}
    end.

compile_view([]) -> {ok, auto};
compile_view([#{op := Op, vals := [Value]}]) when Op =:= eq; Op =:= default ->
    case string:lowercase(Value) of
        "auto" -> {ok, auto};
        "raw" -> {ok, raw};
        "typed" -> {ok, typed};
        "both" -> {ok, both};
        _ -> {error, "view must be auto, raw, typed, or both"}
    end;
compile_view([#{op := Op}]) when Op =/= eq, Op =/= default ->
    {error, "view supports only '=' or ':'"};
compile_view(_) ->
    {error, "view selector may appear only once and requires one value"}.

compile_sorts(Controls) -> compile_sorts(Controls, []).

compile_sorts([], Acc) -> {ok, lists:reverse(Acc)};
compile_sorts([#{op := Op, vals := Values} | Rest], Acc)
  when Op =:= eq; Op =:= default ->
    Parsed = [wfcli_query_sort:parse(Value) || Value <- Values],
    compile_sorts(Rest, lists:reverse(Parsed) ++ Acc);
compile_sorts([_ | _], _Acc) ->
    {error, "sort supports only '=' or ':'"}.

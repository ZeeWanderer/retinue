%%%-------------------------------------------------------------------
%% Unified-query adapter for daemon-owned Warframe Market data.
%%%-------------------------------------------------------------------
-module(wfcli_market_query).

-export([execute/3]).

-doc "Compile and execute existing query AST against market entities.".
-spec execute(term(), map(), map()) -> {ok, map()} | {error, term()}.
execute(Ast0, Request, Snapshot) ->
    case wfcli_query_parse:extract_control(Ast0, sort) of
        {ok, Ast, SortControls} ->
            case compile_sorts(SortControls) of
                {ok, Sorts0} ->
                    Sorts = case Sorts0 of
                        [] -> wfcli_entity_market:default_sort(market);
                        _ -> Sorts0
                    end,
                    run(Ast, Sorts, Request, Snapshot);
                {error, Error} -> {error, {query_errors, [Error]}}
            end;
        {error, Error} -> {error, {query_errors, [Error]}}
    end.

run(Ast, Sorts, Request, Snapshot) ->
    case {wfcli_entity_query:compile(Ast, wfcli_entity_market, market),
          wfcli_entity_query:compile_sorts(Sorts, wfcli_entity_market, market)} of
        {{ok, Compiled}, {ok, CompiledSorts}} ->
            Entries = wfcli_entity_market:build_entries(
                        Snapshot, #{raw => maps:get(raw, Request, false)}),
            Result = wfcli_entity_query:execute(
                       Entries, Compiled, CompiledSorts, wfcli_entity_market, market,
                       maps:get(offset, Request, 0), maps:get(limit, Request, infinity)),
            Query = #{query => Compiled, compiled_sort => CompiledSorts,
                      output_format => maps:get(output_format, Request, table),
                      raw => maps:get(raw, Request, false)},
            {ok, #{query => Query,
                   results => Result#{updated_at => maps:get(updated_at, Snapshot, undefined),
                                     manifest_fetched_at => maps:get(manifest_fetched_at,
                                                                      Snapshot, undefined)}}};
        {{error, Errors}, _} -> {error, {query_errors, Errors}};
        {_, {error, Errors}} -> {error, {query_errors, Errors}}
    end.

compile_sorts(Controls) -> compile_sorts(Controls, []).

compile_sorts([], Acc) -> {ok, lists:reverse(Acc)};
compile_sorts([#{op := Op, vals := Values} | Rest], Acc)
  when Op =:= eq; Op =:= default ->
    Parsed = [wfcli_query_sort:parse(Value) || Value <- Values],
    compile_sorts(Rest, lists:reverse(Parsed) ++ Acc);
compile_sorts([_ | _], _Acc) ->
    {error, "sort supports only '=' or ':'"}.

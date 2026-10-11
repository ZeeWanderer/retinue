%%%-------------------------------------------------------------------
%% Query parsing and matching for worldstate entries.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_query).

-export([parse/1, match/2, extract/2, has_filters/1]).

-type entry() :: map().
-type parsed_query() :: map().
-type extract_result() :: [{string(), string()}].

-doc "Parse one worldstate expression and remove top-level extract/sort control clauses.".
-spec parse(string()) -> parsed_query().
parse(Query) ->
    case wfcli_query_parse:parse(Query) of
        {ok, Ast0} ->
            case wfcli_query_parse:extract_control(Ast0, extract) of
                {ok, Ast1, ExtractControls} ->
                    case wfcli_query_parse:extract_control(Ast1, sort) of
                        {ok, Ast, SortControls} ->
                            compile_query(Ast, ExtractControls, SortControls);
                        {error, Error} -> error_query(Error)
                    end;
                {error, Error} -> error_query(Error)
            end;
        {error, Error} -> error_query(Error)
    end.

-doc "Return true when parsed query contains matching, extraction, sorting, or syntax errors.".
-spec has_filters(parsed_query()) -> boolean().
has_filters(Parsed) ->
    maps:get(query, Parsed, match_all) =/= match_all orelse
        maps:get(extracts, Parsed, []) =/= [] orelse
        maps:get(sort, Parsed, []) =/= [] orelse
        maps:get(errors, Parsed, []) =/= [].

-doc "Evaluate one normalized worldstate entity through shared entity query semantics.".
-spec match(entry(), parsed_query()) -> boolean().
match(_Entry, #{errors := [_ | _]}) -> false;
match(Entry, Parsed) ->
    wfcli_entity_query:match(Entry, maps:get(query, Parsed, match_all),
                             wfcli_entity_worldstate, worldstate).

-doc "Extract display strings from raw entry data for extract control clauses.".
-spec extract(entry(), [string()]) -> extract_result().
extract(Entry, Extracts) ->
    lists:map(
      fun(Path) ->
          Data = maps:get(data, Entry, #{}),
          {Path, retinue_data_extract:extract_string(Data, Path)}
      end,
      Extracts).

compile_query(Ast, ExtractControls, SortControls) ->
    case {control_values(extract, ExtractControls), controls_to_sorts(SortControls)} of
        {{ok, ExtractValues}, {ok, Sorts}} ->
            case wfcli_entity_query:compile(Ast, wfcli_entity_worldstate, worldstate) of
                {ok, Compiled} ->
                    #{query => Compiled,
                      extracts => [extract_path(Path) || Path <- ExtractValues],
                      sort => Sorts, errors => [], filters => [], text => []};
                {error, Errors} -> error_query(Errors)
            end;
        {{error, Error}, _} -> error_query(Error);
        {_, {error, Error}} -> error_query(Error)
    end.

control_values(Name, Controls) ->
    control_values(Name, Controls, []).

control_values(_Name, [], Acc) -> {ok, lists:reverse(Acc)};
control_values(Name, [#{op := Op, vals := Values} | Rest], Acc)
  when Op =:= eq; Op =:= default ->
    control_values(Name, Rest, lists:reverse(Values) ++ Acc);
control_values(Name, [_ | _], _Acc) ->
    {error, lists:flatten(io_lib:format("~s supports only '=' or ':'", [atom_to_list(Name)]))}.

controls_to_sorts(Controls) ->
    case control_values(sort, Controls) of
        {ok, Values} -> {ok, [wfcli_query_sort:parse(Value) || Value <- Values]};
        Error -> Error
    end.

-doc "Normalize extract paths; data.foo.bar and foo.bar both read from raw entry data.".
extract_path(Path0) ->
    Path = string:trim(retinue_text:to_list(Path0)),
    case lists:prefix("data.", string:lowercase(Path)) of
        true -> string:slice(Path, 5);
        false -> Path
    end.

error_query(Errors) when is_list(Errors), Errors =/= [], is_list(hd(Errors)) ->
    #{query => match_all, extracts => [], sort => [], errors => Errors,
      filters => [], text => []};
error_query(Error) ->
    #{query => match_all, extracts => [], sort => [], errors => [lists:flatten(Error)],
      filters => [], text => []}.

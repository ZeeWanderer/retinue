%%%-------------------------------------------------------------------
%% Shared compiler and evaluator for normalized query entities.
%%%-------------------------------------------------------------------
-module(wfcli_entity_query).

-export([compile/3, compile_sorts/3, match/4, execute/7]).

-type schema() :: module().
-type kind() :: atom().
-type ast() :: term().

-doc "Resolve raw query fields and values against one entity schema.".
-spec compile(ast(), schema(), kind()) -> {ok, ast()} | {error, [string()]}.
compile(Ast, Schema, Kind) ->
    case compile_ast(Ast, Schema, Kind, erlang:system_time(millisecond)) of
        {ok, Compiled} -> {ok, Compiled};
        {error, Error} -> {error, [Error]}
    end.

-doc "Resolve sort keys once so cached entities can be sorted without query-key parsing.".
-spec compile_sorts([map()], schema(), kind()) -> {ok, [map()]} | {error, [string()]}.
compile_sorts(Sorts, Schema, Kind) ->
    compile_sorts(Sorts, Schema, Kind, []).

-doc "Evaluate one compiled AST against one normalized entity.".
-spec match(map(), ast(), schema(), kind()) -> boolean().
match(_Entry, match_all, _Schema, _Kind) -> true;
match(Entry, {term, Term}, _Schema, _Kind) ->
    contains(maps:get(haystack, Entry, ""), Term);
match(Entry, {filter, Spec, Op, Values}, Schema, Kind) ->
    case maps:get(match, Spec, undefined) of
        undefined -> match_field(Entry, Spec, Op, Values);
        Matcher -> Schema:query_match(Kind, Matcher, Op, Values, Entry)
    end;
match(Entry, {'not', Ast}, Schema, Kind) ->
    not match(Entry, Ast, Schema, Kind);
match(Entry, {'and', Asts}, Schema, Kind) ->
    lists:all(fun(Ast) -> match(Entry, Ast, Schema, Kind) end, Asts);
match(Entry, {'or', Asts}, Schema, Kind) ->
    lists:any(fun(Ast) -> match(Entry, Ast, Schema, Kind) end, Asts).

-doc "Filter, stable-sort, and page normalized entities through one shared execution path.".
-spec execute([map()], ast(), [map()], schema(), kind(), non_neg_integer(),
              non_neg_integer() | infinity) -> map().
execute(Entries, Ast, Sorts, Schema, Kind, Offset, Limit) ->
    Filtered = [Entry || Entry <- Entries, match(Entry, Ast, Schema, Kind)],
    Sorted = sort_entries(Filtered, Sorts),
    Slice = take(Limit, drop(Offset, Sorted)),
    #{kind => Kind, slice => Slice,
      total => length(Sorted), shown => length(Slice)}.

compile_ast(match_all, _Schema, _Kind, _Now) -> {ok, match_all};
compile_ast({term, Text}, _Schema, _Kind, _Now) ->
    {ok, {term, string:lowercase(Text)}};
compile_ast({filter, Key0, Op0, Values0}, Schema, Kind, Now) ->
    case Schema:query_field(Kind, Key0) of
        {ok, Spec} ->
            Op = case Op0 of default -> maps:get(default_op, Spec, eq); _ -> Op0 end,
            case validate_operator(Op, maps:get(kind, Spec, string)) of
                ok ->
                    case compile_values(Values0, maps:get(kind, Spec, string), Now) of
                        {ok, Values} -> {ok, {filter, Spec, Op, Values}};
                        {error, Error} -> {error, field_error(Key0, Error)}
                    end;
                {error, Error} -> {error, field_error(Key0, Error)}
            end;
        error ->
            {error, lists:flatten(io_lib:format("unknown query field for ~p: ~s", [Kind, key_text(Key0)]))}
    end;
compile_ast({'not', Ast}, Schema, Kind, Now) ->
    map_compiled(fun(Compiled) -> {'not', Compiled} end, compile_ast(Ast, Schema, Kind, Now));
compile_ast({'and', Asts}, Schema, Kind, Now) ->
    compile_children('and', Asts, Schema, Kind, Now, []);
compile_ast({'or', Asts}, Schema, Kind, Now) ->
    compile_children('or', Asts, Schema, Kind, Now, []).

compile_children(Tag, [], _Schema, _Kind, _Now, Acc) ->
    {ok, {Tag, lists:reverse(Acc)}};
compile_children(Tag, [Ast | Rest], Schema, Kind, Now, Acc) ->
    case compile_ast(Ast, Schema, Kind, Now) of
        {ok, Compiled} -> compile_children(Tag, Rest, Schema, Kind, Now, [Compiled | Acc]);
        Error -> Error
    end.

map_compiled(Fun, {ok, Value}) -> {ok, Fun(Value)};
map_compiled(_Fun, Error) -> Error.

compile_sorts([], _Schema, _Kind, Acc) -> {ok, lists:reverse(Acc)};
compile_sorts([Sort | Rest], Schema, Kind, Acc) ->
    Key0 = maps:get(key, Sort, ""),
    case Schema:query_sort_field(Kind, Key0) of
        {ok, Field} ->
            Compiled = Sort#{field => Field},
            compile_sorts(Rest, Schema, Kind, [Compiled | Acc]);
        error ->
            {error, [lists:flatten(io_lib:format("unknown sort field for ~p: ~s", [Kind, key_text(Key0)]))]}
    end.

validate_operator(Op, number) when Op =:= eq; Op =:= neq; Op =:= gt; Op =:= gte;
                                   Op =:= lt; Op =:= lte -> ok;
validate_operator(Op, number) -> operator_error(Op, number);
validate_operator(Op, time_point) -> validate_operator(Op, number);
validate_operator(Op, dynamic) when Op =:= eq; Op =:= neq; Op =:= contains; Op =:= gt;
                                    Op =:= gte; Op =:= lt; Op =:= lte -> ok;
validate_operator(Op, _Kind) when Op =:= eq; Op =:= neq; Op =:= contains -> ok;
validate_operator(Op, Kind) -> operator_error(Op, Kind).

operator_error(Op, Kind) ->
    {error, lists:flatten(io_lib:format("operator ~p is not valid for ~p values", [Op, Kind]))}.

compile_values(Values, number, _Now) ->
    compile_numbers(Values, []);
compile_values(Values, time_point, Now) -> compile_times(Values, Now, []);
compile_values(Values, _Kind, _Now) ->
    {ok, [string:lowercase(retinue_text:to_list(Value)) || Value <- Values]}.

compile_times([], _Now, Acc) -> {ok, lists:reverse(Acc)};
compile_times([Value | Rest], Now, Acc) ->
    case retinue_time:parse(Value, Now) of
        {ok, Millis} -> compile_times(Rest, Now, [Millis | Acc]);
        error -> {error, lists:flatten(io_lib:format("invalid timestamp: ~ts (use RFC 3339, epoch milliseconds or now-1h)", [Value]))}
    end.

compile_numbers([], Acc) -> {ok, lists:reverse(Acc)};
compile_numbers([Value | Rest], Acc) ->
    case parse_number(Value) of
        undefined -> {error, lists:flatten(io_lib:format("invalid number: ~s", [Value]))};
        Number -> compile_numbers(Rest, [Number | Acc])
    end.

field_error(Key, Error) ->
    lists:flatten(io_lib:format("invalid filter ~s: ~s", [key_text(Key), Error])).

match_field(Entry, Spec, Op, Expected) ->
    Values = field_values(Entry, maps:get(source, Spec, maps:get(key, Spec))),
    case Values of
        [] -> Op =:= neq;
        _ -> match_values(maps:get(kind, Spec, string), Op, Expected, Values)
    end.

field_values(Entry, haystack) ->
    present_values([maps:get(haystack, Entry, undefined)]);
field_values(Entry, {entry, Key}) ->
    present_values([maps:get(Key, Entry, undefined)]);
field_values(Entry, {entry_values, Key}) ->
    case maps:get(Key, Entry, undefined) of
        Values when is_list(Values) -> present_values(Values);
        Value -> present_values([Value])
    end;
field_values(Entry, {data, Key}) ->
    present_values([maps:get(Key, maps:get(data, Entry, #{}), undefined)]);
field_values(Entry, {row, Key}) ->
    present_values([maps:get(Key, maps:get(row_map, Entry, #{}), undefined)]);
field_values(Entry, {data_path, Path}) ->
    present_values(retinue_data_extract:extract_values(maps:get(data, Entry, #{}), Path));
field_values(Entry, {entry_path, Key, Path}) ->
    present_values(retinue_data_extract:extract_values(maps:get(Key, Entry, #{}), Path));
field_values(Entry, Key) ->
    Data = maps:get(data, Entry, #{}),
    Row = maps:get(row_map, Entry, #{}),
    present_values([maps:get(Key, Data, maps:get(Key, Row, maps:get(Key, Entry, undefined)))]).

present_values(Values) ->
    [Value || Value <- Values, Value =/= undefined, Value =/= null, Value =/= "", Value =/= <<>>].

match_values(time_point, Op, Expected, Actual) ->
    match_values(number, Op, Expected, [parse_time(Value) || Value <- Actual]);
match_values(number, neq, Expected, Actual) ->
    not lists:any(fun(A) -> lists:any(fun(E) -> numbers_equal(A, E) end, Expected) end, Actual);
match_values(number, Op, Expected, Actual) ->
    lists:any(fun(A) -> lists:any(fun(E) -> compare_number(Op, parse_number(A), E) end, Expected) end, Actual);
match_values(dynamic, Op, Expected, Actual) when Op =:= gt; Op =:= gte; Op =:= lt; Op =:= lte ->
    lists:any(fun(A) -> lists:any(fun(E) -> compare_number(Op, parse_number(A), parse_number(E)) end, Expected) end, Actual);
match_values(_Kind, neq, Expected, Actual) ->
    not lists:any(fun(A) -> lists:any(fun(E) -> equals(A, E) end, Expected) end, Actual);
match_values(_Kind, contains, Expected, Actual) ->
    lists:any(fun(A) -> lists:any(fun(E) -> contains(A, E) end, Expected) end, Actual);
match_values(_Kind, eq, Expected, Actual) ->
    lists:any(fun(A) -> lists:any(fun(E) -> equals(A, E) end, Expected) end, Actual).

numbers_equal(A, B) ->
    case parse_number(A) of
        undefined -> false;
        Number -> Number =:= B
    end.

equals(A, B) ->
    string:lowercase(retinue_text:to_list(A)) =:= string:lowercase(retinue_text:to_list(B)).

contains(A, B) ->
    string:find(string:lowercase(retinue_text:to_list(A)),
                string:lowercase(retinue_text:to_list(B))) =/= nomatch.

parse_number(Value) when is_integer(Value); is_float(Value) -> Value;
parse_number(undefined) -> undefined;
parse_number(Value) ->
    Text = string:trim(retinue_text:to_list(Value)),
    case string:to_integer(Text) of
        {Integer, ""} -> Integer;
        _ ->
            case string:to_float(Text) of
                {Float, ""} -> Float;
                _ -> undefined
            end
    end.

parse_time(Value) ->
    case retinue_time:parse(Value) of {ok, Millis} -> Millis; error -> undefined end.

compare_number(_Op, undefined, _B) -> false;
compare_number(_Op, _A, undefined) -> false;
compare_number(eq, A, B) -> A =:= B;
compare_number(neq, A, B) -> A =/= B;
compare_number(gt, A, B) -> A > B;
compare_number(gte, A, B) -> A >= B;
compare_number(lt, A, B) -> A < B;
compare_number(lte, A, B) -> A =< B.

sort_entries(Entries, []) -> Entries;
sort_entries(Entries, Sorts) ->
    Indexed = lists:zip(lists:seq(0, length(Entries) - 1), Entries),
    Sorted = lists:sort(
      fun({AIndex, A}, {BIndex, B}) ->
          case compare_entries(A, B, Sorts) of
              eq -> AIndex =< BIndex;
              lt -> true;
              gt -> false
          end
      end, Indexed),
    [Entry || {_Index, Entry} <- Sorted].

compare_entries(_A, _B, []) -> eq;
compare_entries(A, B, [Sort | Rest]) ->
    Dir = maps:get(dir, Sort, asc),
    Field = maps:get(field, Sort),
    VA = sort_value(A, Field),
    VB = sort_value(B, Field),
    case compare_sort_values(VA, VB, Dir) of
        eq -> compare_entries(A, B, Rest);
        Result -> Result
    end.

sort_value(Entry, Field) ->
    case field_values(Entry, maps:get(source, Field, maps:get(key, Field))) of
        [] -> missing;
        [Value | _] ->
            case maps:get(kind, Field, string) of
                number -> case parse_number(Value) of undefined -> missing; N -> {number, N} end;
                time_point -> case parse_time(Value) of undefined -> missing; N -> {number, N} end;
                _ -> {string, string:lowercase(retinue_text:to_list(Value))}
            end
    end.

compare_sort_values(missing, missing, _Dir) -> eq;
compare_sort_values(missing, _B, _Dir) -> gt;
compare_sort_values(_A, missing, _Dir) -> lt;
compare_sort_values(A, B, _Dir) when A =:= B -> eq;
compare_sort_values(A, B, asc) when A < B -> lt;
compare_sort_values(_A, _B, asc) -> gt;
compare_sort_values(A, B, desc) when A > B -> lt;
compare_sort_values(_A, _B, desc) -> gt.

drop(N, List) when N =< 0 -> List;
drop(_N, []) -> [];
drop(N, [_ | Rest]) -> drop(N - 1, Rest).

take(infinity, List) -> List;
take(Limit, List) -> lists:sublist(List, Limit).

key_text(Key) when is_atom(Key) -> atom_to_list(Key);
key_text(Key) -> retinue_text:to_list(Key).

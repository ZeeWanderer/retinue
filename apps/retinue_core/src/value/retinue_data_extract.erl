%%%-------------------------------------------------------------------
%% Shared extraction helpers for nested maps/lists.
%%%-------------------------------------------------------------------
-module(retinue_data_extract).

-export([extract_values/2, extract_string/2, parse_path/1]).

-type path() :: string() | [segment()].
-type segment() :: wildcard | {index, non_neg_integer()} | {key, string()}.

-doc "Extract a path and join all found values for CLI/watch display.".
-spec extract_string(term(), path()) -> string().
extract_string(Value, Path) ->
    Values = extract_values(Value, Path),
    string:join([display_value(V) || V <- Values], ", ").

display_value(Value) when is_map(Value) ->
    lists:flatten(io_lib:format("~tp", [Value]));
display_value(Value) when is_list(Value) ->
    case io_lib:printable_unicode_list(Value) of
        true -> Value;
        false -> lists:flatten(io_lib:format("~tp", [Value]))
    end;
display_value(Value) -> retinue_text:to_list(Value).

-doc "Extract all values under a dotted path; `*` fans out and numeric segments index lists.".
-spec extract_values(term(), path()) -> [term()].
extract_values(Value, Path) when is_list(Path) ->
    Segs = case Path of
        [] -> [];
        [H | _] when is_integer(H) -> parse_path(Path);
        _ -> Path
    end,
    extract_values(Value, Segs, []).

extract_values(Value, [], Acc) ->
    lists:reverse([Value | Acc]);
extract_values(Value, [Seg | Rest], Acc) ->
    case Seg of
        wildcard ->
            case Value of
                Map when is_map(Map) ->
                    lists:append([extract_values(V, Rest, Acc) || V <- maps:values(Map)]);
                List when is_list(List) ->
                    lists:append([extract_values(V, Rest, Acc) || V <- List]);
                _ -> []
            end;
        {index, Idx} ->
            case Value of
                List when is_list(List), Idx >= 0 ->
                    case Idx < length(List) of
                        true ->
                            case lists:nthtail(Idx, List) of
                                [V | _] -> extract_values(V, Rest, Acc);
                                [] -> []
                            end;
                        false -> []
                    end;
                _ -> []
            end;
        {key, Key} ->
            case Value of
                Map when is_map(Map) ->
                    case map_get_any(Key, Map) of
                        undefined -> [];
                        V -> extract_values(V, Rest, Acc)
                    end;
                List when is_list(List) ->
                    lists:append([extract_values(Elem, [Seg | Rest], Acc) || Elem <- List]);
                _ -> []
            end
    end.

-doc "Parse a dotted extraction path into key, index, and wildcard segments.".
-spec parse_path(string()) -> [segment()].
parse_path(Path0) ->
    Parts = string:split(Path0, ".", all),
    [parse_path_seg(P) || P <- Parts, P =/= ""].

parse_path_seg("*") -> wildcard;
parse_path_seg(Seg) ->
    case string:to_integer(Seg) of
        {Int, ""} -> {index, Int};
        _ -> {key, Seg}
    end.

map_get_any(Key, Map) ->
    BinKey = list_to_binary(Key),
    case maps:get(BinKey, Map, undefined) of
        undefined -> maps:get(Key, Map, undefined);
        Val -> Val
    end.

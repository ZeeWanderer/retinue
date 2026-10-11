%%%-------------------------------------------------------------------
%% Shared text normalization helpers.
%%%-------------------------------------------------------------------
-module(retinue_text).

-export([to_list/1, to_binary/1, value_present/1, join_list/2, join_list/3, join_parts/2]).

to_list(V) when is_binary(V) ->
    try unicode:characters_to_list(V) of
        List when is_list(List) -> List;
        _ -> binary_to_list(V)
    catch
        _:_ -> binary_to_list(V)
    end;
to_list(V) when is_list(V) -> V;
to_list(V) when is_atom(V) -> atom_to_list(V);
to_list(V) -> lists:flatten(io_lib:format("~p", [V])).

to_binary(V) when is_binary(V) -> V;
to_binary(V) when is_list(V) ->
    try unicode:characters_to_binary(V) of
        Bin when is_binary(Bin) -> Bin;
        _ -> list_to_binary(byte_list(V))
    catch
        _:_ -> list_to_binary(byte_list(V))
    end;
to_binary(V) when is_atom(V) ->
    to_binary(atom_to_list(V));
to_binary(V) ->
    to_binary(to_list(V)).

byte_list(List) ->
    case is_byte_list(List) of
        true -> List;
        false -> to_list(List)
    end.

is_byte_list([]) -> true;
is_byte_list([H | T]) when is_integer(H, 0, 255) -> is_byte_list(T);
is_byte_list(_) -> false.

value_present(undefined) -> false;
value_present([]) -> false;
value_present(<<>>) -> false;
value_present(null) -> false;
value_present(_) -> true.

join_list(List, Sep) ->
    join_list(List, Sep, fun value_present/1).

join_list(List, Sep, Pred) ->
    string:join([to_list(V) || V <- List, Pred(V)], Sep).

join_parts(Parts, Sep) ->
    Clean = [P || P <- Parts, value_present(P)],
    string:join([to_list(P) || P <- Clean], Sep).

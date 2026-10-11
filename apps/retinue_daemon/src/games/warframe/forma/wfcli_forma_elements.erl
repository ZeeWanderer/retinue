-module(wfcli_forma_elements).

-export([normalize/1, unique/1, signature/1, equivalent_orders/1]).

normalize(Values) when is_list(Values) ->
    Types = [element(Value) || Value <- Values],
    case lists:member(unknown, Types) of
        true -> {error, invalid_elements};
        false -> {ok, Types}
    end;
normalize(_) -> {error, invalid_elements}.

element(heat) -> heat;
element(cold) -> cold;
element(electricity) -> electricity;
element(toxin) -> toxin;
element(<<"heat">>) -> heat;
element(<<"cold">>) -> cold;
element(<<"electricity">>) -> electricity;
element(<<"toxin">>) -> toxin;
element(_) -> unknown.

unique(Elements) ->
    lists:foldl(fun(E, Acc) ->
        case lists:member(E, Acc) of true -> Acc; false -> Acc ++ [E] end
    end, [], Elements).

signature(Elements) -> lists:sort(pairs(unique(Elements))).

pairs([A, B | Rest]) -> [lists:sort([A, B]) | pairs(Rest)];
pairs([]) -> [];
pairs([A]) -> [[A]].

equivalent_orders(Elements) ->
    Unique = unique(Elements),
    Signature = signature(Unique),
    [Order || Order <- permutations(Unique), signature(Order) =:= Signature].

permutations([]) -> [[]];
permutations(Elements) ->
    [[E | Tail] || E <- Elements, Tail <- permutations(lists:delete(E, Elements))].

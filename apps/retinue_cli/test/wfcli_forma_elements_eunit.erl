-module(wfcli_forma_elements_eunit).
-include_lib("eunit/include/eunit.hrl").

pair_signature_test() ->
    Signature = wfcli_forma_elements:signature([cold, toxin, heat]),
    ?assertEqual(Signature, wfcli_forma_elements:signature([toxin, cold, cold, heat])),
    ?assertNotEqual(Signature, wfcli_forma_elements:signature([cold, heat, toxin])),
    ?assertEqual(wfcli_forma_elements:signature([heat, cold, electricity, toxin]),
                 wfcli_forma_elements:signature([toxin, electricity, cold, heat])).

innate_tail_stays_equivalent_test() ->
    Tails = subsets([heat, cold, electricity, toxin]),
    lists:foreach(fun(Elements) ->
        lists:foreach(fun(Order) ->
            lists:foreach(fun(Tail) ->
                ?assertEqual(wfcli_forma_elements:signature(Elements ++ Tail),
                             wfcli_forma_elements:signature(Order ++ Tail))
            end, Tails)
        end, wfcli_forma_elements:equivalent_orders(Elements))
    end, Tails).

assignment_matches_exhaustive_reference_test() ->
    %% Neutral/combined mods have no primary elements and may separate a pair.
    Mods = [mod(1, cold, vazarin), mod(2, toxin, naramon), mod(3, heat, madurai),
            mod(4, none, naramon), mod(5, cold, madurai)],
    ElementOrder = [cold, toxin, heat],
    Builds = [#{name => test, mods => Mods,
                elemental_orders => wfcli_forma_elements:equivalent_orders(ElementOrder)}],
    lists:foreach(fun(Polarities) ->
        Item = #{slots => Polarities, capacity => 100, reactor => false},
        Plan = wfcli_forma_rules:current_plan(Item),
        [Loadout] = wfcli_forma_assignment:loadouts(#{item => Item, builds => Builds}, Plan),
        Candidates = [Ordered || Ordered <- permutations(Mods),
            wfcli_forma_elements:signature(elements(Ordered)) =:=
                wfcli_forma_elements:signature(ElementOrder)],
        Expected = lists:min([lists:sum([wfcli_forma_model:mod_cost(maps:get(polarity, M), P, 10)
                             || {M, P} <- lists:zip(Ordered, Polarities)]) || Ordered <- Candidates]),
        ?assertEqual(Expected, maps:get(drain, Loadout)),
        Assignment = maps:get(assignments, Loadout),
        Sorted = lists:keysort(1, [{maps:get(N, Assignment), M}
                                  || {N, M} <- lists:enumerate(Mods)]),
        ?assertEqual(wfcli_forma_elements:signature(ElementOrder),
                     wfcli_forma_elements:signature(elements([M || {_, M} <- Sorted])))
    end, [[madurai, naramon, vazarin, madurai, naramon],
          [vazarin, madurai, naramon, none, vazarin],
          [omni, naramon, none, madurai, vazarin]]).

fixed_slots_cannot_break_pair_test() ->
    Mods = [(mod(1, cold, vazarin))#{slot => 1},
            (mod(2, toxin, naramon))#{slot => 3},
            (mod(3, heat, madurai))#{slot => 2}],
    Build = #{name => test, mods => Mods,
              elemental_orders => wfcli_forma_elements:equivalent_orders([cold, toxin, heat])},
    Item = #{slots => [omni, omni, omni], capacity => 100, reactor => false},
    ?assertNot(wfcli_forma_assignment:fits_all(
                 wfcli_forma_rules:current_plan(Item), Item, [Build])).

mod(Name, Element, Polarity) ->
    #{name => Name, slot => undefined, polarity => Polarity, cost => 10,
      elements => case Element of none -> []; _ -> [Element] end}.

elements(Mods) -> lists:append([maps:get(elements, M) || M <- Mods]).

subsets([]) -> [[]];
subsets([H | T]) -> S = subsets(T), S ++ [[H | Tail] || Tail <- S].

permutations([]) -> [[]];
permutations(Values) -> [[V | Tail] || V <- Values, Tail <- permutations(lists:delete(V, Values))].

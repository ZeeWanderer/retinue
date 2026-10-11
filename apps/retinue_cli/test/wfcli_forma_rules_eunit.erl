-module(wfcli_forma_rules_eunit).

-include_lib("eunit/include/eunit.hrl").

swap_locks_validate_input_test() ->
    Base = #{file => "test", item => #{<<"type">> => <<"weapon">>,
             <<"capacity">> => 30, <<"slots">> => [<<"none">>, <<"madurai">>]},
             builds => [], constraints => #{}},
    WithLocks = fun(Locks) ->
        Base#{item => (maps:get(item, Base))#{<<"swap_locked_slots">> => Locks}}
    end,
    ?assertMatch({ok, #{item := #{swap_locked_slots := [1, aura, exilus]}}},
                 wfcli_forma_model:normalize_config(WithLocks([<<"stance">>, <<"exilus">>, 1]))),
    lists:foreach(fun(Invalid) ->
        ?assertMatch({error, [_ | _]}, wfcli_forma_model:normalize_config(WithLocks(Invalid)))
    end, [null, #{}, [null], [3], [<<"unknown">>]]).

empty_slot_is_reusable_test() ->
    Item = item([madurai, none]),
    Plan = plan([none, madurai]),
    ?assertEqual(0, wfcli_forma_rules:cost(Plan, Item, #{})),
    ?assertEqual(0, wfcli_forma_rules:removal_cost(Plan, Item)),
    ?assertMatch([#{action := swap, slot := 1, other_slot := 2}],
                 wfcli_forma_rules:operations(Plan, Item)).

stance_cannot_supply_normal_slot_test() ->
    Item = (item([none]))#{aura_slot => madurai},
    Plan = plan([madurai]),
    ?assertEqual(2, wfcli_forma_rules:cost(Plan, Item, #{})),
    ?assertEqual(1, wfcli_forma_rules:removal_cost(Plan, Item)),
    ?assertEqual([polarize, polarize],
                 [maps:get(action, Op) || Op <- wfcli_forma_rules:operations(Plan, Item)]).

removing_umbral_uses_regular_forma_test() ->
    Item = item([umbral]),
    Plan = plan([none]),
    ?assertEqual(1, wfcli_forma_rules:cost(Plan, Item, #{})),
    ?assertMatch([#{forma := standard}], wfcli_forma_rules:operations(Plan, Item)).

operations_replay_and_match_cost_test() ->
    Values = [none, madurai, vazarin, omni],
    Plans = [plan([A, B, C]) || A <- Values, B <- Values, C <- Values],
    lists:foreach(fun({From, To, Unlocked}) ->
        Item = (item([maps:get(N, From) || N <- [1, 2, 3]]))#{swap_unlocked => Unlocked},
        Ops = wfcli_forma_rules:operations(To, Item),
        case {Unlocked, Ops} of
            {false, [First | _]} -> ?assertEqual(polarize, maps:get(action, First));
            _ -> ok
        end,
        ?assertEqual(To, lists:foldl(fun apply_operation/2, From, Ops)),
        Weight = lists:sum([wfcli_forma_model:forma_cost(P)
                             || #{action := polarize, polarity := P} <- Ops]),
        ?assertEqual(Weight, wfcli_forma_rules:cost(To, Item, #{allow_omni => true}))
    end, [{From, To, Unlocked} || From <- Plans, To <- Plans, Unlocked <- [true, false]]).

partial_capacity_bound_admits_valid_completion_test() ->
    Item = (item([madurai, vazarin]))#{capacity => 10, reactor => false},
    Mods = [#{name => N, slot => undefined, cost => 10, polarity => P}
            || {N, P} <- [{1, madurai}, {2, vazarin}]],
    Builds = [#{name => test, mods => Mods}],
    Plan = plan([madurai, vazarin]),
    ?assert(wfcli_forma_assignment:possibly_fits_all(#{}, Item, Builds)),
    ?assert(wfcli_forma_assignment:possibly_fits_all(maps:with([1], Plan), Item, Builds)),
    ?assert(wfcli_forma_assignment:possibly_fits_all(Plan, Item, Builds)),
    ?assertNot(wfcli_forma_assignment:possibly_fits_all(plan([none, none]), Item, Builds)).

existing_omni_can_move_without_allowing_more_test() ->
    Item = (item([omni, none]))#{capacity => 10, reactor => false},
    Mod = fun(Polarity) -> #{name => "Mod", slot => 2, cost => 20, polarity => Polarity} end,
    Builds = [#{name => "A", mods => [Mod(madurai)]},
              #{name => "B", mods => [Mod(vazarin)]}],
    Config = #{item => Item, builds => Builds, constraints => #{}},
    ?assertEqual({plan([none, omni]), 0}, wfcli_forma_search:plan(Config, #{})),
    ?assertEqual({error, forma_not_allowed},
                 wfcli_forma_rules:validate(plan([omni, omni]),
                                           #{item => Item, builds => Builds, flags => #{}})).

apply_operation(#{action := swap, slot := Slot, other_slot := Other,
                  before := Before, polarity := Polarity}, State) ->
    ?assert(is_integer(Slot) andalso is_integer(Other)),
    ?assertEqual(Before, maps:get(Slot, State)),
    ?assertEqual(Polarity, maps:get(Other, State)),
    State#{Slot => Polarity, Other => Before};
apply_operation(#{action := polarize, slot := Slot,
                  before := Before, polarity := Polarity}, State) ->
    ?assertEqual(Before, maps:get(Slot, State)),
    State#{Slot => Polarity}.

item(Polarities) ->
    #{slots => Polarities, aura_slot => none, exilus_slot => none,
      swap_locked_slots => [aura, exilus]}.

plan(Polarities) -> wfcli_forma_rules:current_plan(item(Polarities)).

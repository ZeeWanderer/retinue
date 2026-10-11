-module(wf_polarity_eunit).

-include_lib("eunit/include/eunit.hrl").

normalizes_game_and_overframe_values_test() ->
    ?assertEqual(madurai, wf_polarity:normalize(<<"AP_ATTACK">>)),
    ?assertEqual(vazarin, wf_polarity:normalize(2)),
    ?assertEqual(none, wf_polarity:normalize(<<"AP_UNIVERSAL">>)),
    ?assertEqual(omni, wf_polarity:normalize(<<"AP_ANY">>)),
    ?assertEqual(omni, wf_polarity:normalize(<<"omni">>)),
    ?assertEqual(none, wf_polarity:normalize(<<"none">>)),
    ?assertEqual(umbral, wf_polarity:normalize(8)).

classifies_slot_compatibility_test() ->
    ?assertEqual(matched, wf_polarity:compatibility(madurai, madurai)),
    ?assertEqual(matched, wf_polarity:compatibility(naramon, omni)),
    ?assertEqual(mismatched, wf_polarity:compatibility(umbral, omni)),
    ?assertEqual(mismatched, wf_polarity:compatibility(madurai, vazarin)),
    ?assertEqual(neutral, wf_polarity:compatibility(madurai, none)),
    ?assertEqual(unknown, wf_polarity:compatibility(unknown, madurai)).

applies_game_capacity_rounding_test() ->
    ?assertEqual(6, wf_polarity:mod_cost(madurai, madurai, 11)),
    ?assertEqual(11, wf_polarity:mod_cost(madurai, vazarin, 9)),
    ?assertEqual(9, wf_polarity:mod_cost(madurai, none, 9)),
    ?assertEqual(14, wf_polarity:aura_value(naramon, omni, 7)),
    ?assertEqual(5, wf_polarity:aura_value(naramon, madurai, 7)).

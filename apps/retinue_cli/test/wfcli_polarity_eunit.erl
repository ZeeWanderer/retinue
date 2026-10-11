-module(wfcli_polarity_eunit).

-include_lib("eunit/include/eunit.hrl").

normalizes_game_and_overframe_values_test() ->
    ?assertEqual(madurai, wfcli_polarity:normalize(<<"AP_ATTACK">>)),
    ?assertEqual(vazarin, wfcli_polarity:normalize(2)),
    ?assertEqual(none, wfcli_polarity:normalize(<<"AP_UNIVERSAL">>)),
    ?assertEqual(omni, wfcli_polarity:normalize(<<"AP_ANY">>)),
    ?assertEqual(omni, wfcli_polarity:normalize(<<"omni">>)),
    ?assertEqual(none, wfcli_polarity:normalize(<<"none">>)),
    ?assertEqual(umbral, wfcli_polarity:normalize(8)).

classifies_slot_compatibility_test() ->
    ?assertEqual(matched, wfcli_polarity:compatibility(madurai, madurai)),
    ?assertEqual(matched, wfcli_polarity:compatibility(naramon, omni)),
    ?assertEqual(mismatched, wfcli_polarity:compatibility(umbral, omni)),
    ?assertEqual(mismatched, wfcli_polarity:compatibility(madurai, vazarin)),
    ?assertEqual(neutral, wfcli_polarity:compatibility(madurai, none)),
    ?assertEqual(unknown, wfcli_polarity:compatibility(unknown, madurai)).

applies_game_capacity_rounding_test() ->
    ?assertEqual(6, wfcli_polarity:mod_cost(madurai, madurai, 11)),
    ?assertEqual(11, wfcli_polarity:mod_cost(madurai, vazarin, 9)),
    ?assertEqual(9, wfcli_polarity:mod_cost(madurai, none, 9)),
    ?assertEqual(14, wfcli_polarity:aura_value(naramon, omni, 7)),
    ?assertEqual(5, wfcli_polarity:aura_value(naramon, madurai, 7)).

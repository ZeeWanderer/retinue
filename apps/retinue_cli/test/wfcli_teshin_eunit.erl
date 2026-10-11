-module(wfcli_teshin_eunit).

-include_lib("eunit/include/eunit.hrl").

-define(START, 1605484800).
-define(WEEK, 604800).

rotation_starts_with_umbra_forma_test() ->
    {Reward, Activation, Expiry} = wfcli_teshin:current_reward(?START),
    ?assertEqual("Umbra Forma Blueprint", maps:get(name, Reward)),
    ?assertEqual(150, maps:get(cost, Reward)),
    ?assertEqual(?START, Activation),
    ?assertEqual(?START + ?WEEK - 1, Expiry).

rotation_wraps_after_eight_weeks_test() ->
    {WeekEight, _, _} = wfcli_teshin:current_reward(?START + 7 * ?WEEK),
    {Wrapped, _, _} = wfcli_teshin:current_reward(?START + 8 * ?WEEK),
    ?assertEqual("Shotgun Riven Mod", maps:get(name, WeekEight)),
    ?assertEqual("Umbra Forma Blueprint", maps:get(name, Wrapped)).

inventory_contains_current_and_evergreen_offerings_test() ->
    Entries = wfcli_teshin:inventory_at(?START + 7 * ?WEEK, #{raw => true}),
    ?assertEqual(20, length(Entries)),
    [Weekly, FirstEvergreen | _] = Entries,
    ?assertEqual("Shotgun Riven Mod", maps:get(name, Weekly)),
    ?assertEqual(75, maps:get(<<"Cost">>, maps:get(data, Weekly))),
    ?assertEqual(<<"Weekly">>, maps:get(<<"Availability">>, maps:get(data, Weekly))),
    ?assertEqual("Veiled Riven Cipher", maps:get(name, FirstEvergreen)),
    ?assertEqual(<<"Evergreen">>,
                 maps:get(<<"Availability">>, maps:get(data, FirstEvergreen))).

inventory_entries_are_searchable_test() ->
    Entries = wfcli_teshin:inventory_at(?START, #{}),
    Matches = wfcli_worldstate:search_entries(Entries, "arcane adapter"),
    ?assertEqual(2, length(Matches)).

block_output_avoids_normalized_field_duplicates_test() ->
    [Weekly | _] = wfcli_teshin:inventory_at(?START, #{}),
    Text = wfcli_worldstate_format:format(Weekly, #{}),
    ?assert(string:find(Text, "Steel Essence: 150") =/= nomatch),
    ?assertEqual(nomatch, string:find(Text, "details:")),
    ?assertEqual(nomatch, string:find(Text, "Cost:")),
    ?assertEqual(nomatch, string:find(Text, "id:")).

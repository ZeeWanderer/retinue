-module(wfcli_notification_service_eunit).

-include_lib("eunit/include/eunit.hrl").

initial_snapshot_is_suppressed_test() ->
    Fissure = fissure(<<"one">>, false),
    {[], Seen} = wfcli_notification_service:select_new([Fissure], #{}, true, 1000),
    ?assert(maps:is_key(<<"one">>, Seen)),
    {[Fissure], _} = wfcli_notification_service:select_new(
                       [Fissure], #{}, false, 1000).

only_new_fissures_are_selected_test() ->
    Old = fissure(<<"old">>, false),
    New = fissure(<<"new">>, true),
    {[New], Seen} = wfcli_notification_service:select_new(
                      [Old, New], #{<<"old">> => 900}, false, 1000),
    ?assertEqual(2, map_size(Seen)).

filter_matches_tier_mission_location_and_path_test() ->
    Filter = #{<<"type">> => <<"axi">>, <<"mode">> => <<"survival">>,
               <<"location">> => <<"void">>, <<"steelPath">> => <<"steelPath">>},
    ?assert(wfcli_notification_service:matches_filter(fissure(<<"one">>, true),
                                                       Filter)),
    ?assertNot(wfcli_notification_service:matches_filter(fissure(<<"one">>, false),
                                                          Filter)).

notification_text_identifies_fissure_test() ->
    Text = wfcli_notification_service:notification_text(fissure(<<"one">>, true)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Steel Path Axi">>)),
    ?assertNotEqual(nomatch, binary:match(Text, <<"Mot (Void)">>)).

notification_uses_desktop_identity_test() ->
    Args = wfcli_notification_service:notification_args(fissure(<<"one">>, true)),
    ?assert(lists:member("--icon=wfgui", Args)),
    ?assert(lists:member("--hint=string:desktop-entry:wfgui", Args)).

fissure(Id, Hard) ->
    #{<<"id">> => Id, <<"tier">> => <<"Axi">>,
      <<"mission">> => <<"Survival">>, <<"node">> => <<"Mot (Void)">>,
      <<"expiry">> => <<"2026-08-01T12:00:00Z">>, <<"hard">> => Hard}.

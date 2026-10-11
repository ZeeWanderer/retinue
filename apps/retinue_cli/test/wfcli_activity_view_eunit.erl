-module(wfcli_activity_view_eunit).

-include_lib("eunit/include/eunit.hrl").

projects_fissures_to_json_safe_view_test() ->
    Entry = #{type => fissure, id => <<"fissure-1">>,
              data => #{<<"Modifier">> => <<"VoidT1">>,
                        <<"MissionType">> => <<"MT_EXTERMINATION">>,
                        <<"Node">> => <<"SolNode1">>,
                        <<"Expiry">> => #{<<"$date">> =>
                                               #{<<"$numberLong">> => <<"1780000000000">>}},
                        <<"Hard">> => true}},
    Cetus = #{type => syndicate_mission,
              data => #{<<"Tag">> => <<"CetusSyndicate">>,
                        <<"Activation">> => #{<<"$date">> =>
                                                   #{<<"$numberLong">> => <<"0">>}}}},
    Resurgence = #{type => prime_vault,
                   data => #{<<"Node">> => <<"EarthHUB">>,
                             <<"Activation">> => #{<<"$date">> =>
                                                        #{<<"$numberLong">> => <<"1000">>}},
                             <<"Expiry">> => #{<<"$date">> =>
                                                    #{<<"$numberLong">> => <<"2000">>}},
                             <<"ScheduleInfo">> =>
                                 [#{<<"FeaturedItem">> => <<"/Lotus/StoreItems/Test">>}]}},
    {ok, View} = wfcli_activity_view:project(
                   #{entries => [Entry, Cetus, Resurgence],
                     opts => #{resolve_items => false},
                     source => memory, snapshot_origin => cached,
                     snapshot_age_ms => 12, stale => false, now_ms => 1000}),
    [Fissure] = maps:get(<<"fissures">>, View),
    ?assertEqual(<<"fissure-1">>, maps:get(<<"id">>, Fissure)),
    ?assertEqual(true, maps:get(<<"hard">>, Fissure)),
    ?assert(is_binary(maps:get(<<"tier">>, Fissure))),
    ?assert(is_binary(maps:get(<<"mission">>, Fissure))),
    ?assertEqual(<<"memory">>, maps:get(<<"source">>, View)),
    [Earth, CetusCycle | _] = maps:get(<<"cycles">>, View),
    ?assertEqual(<<"day">>, maps:get(<<"state">>, Earth)),
    ?assertEqual(<<"day">>, maps:get(<<"state">>, CetusCycle)),
    ?assert(is_integer(maps:get(<<"expires_at">>, Earth))),
    ?assertMatch(#{<<"name">> := <<"Prime Resurgence">>,
                   <<"featured">> := <<>>},
                 maps:get(<<"resurgence">>, View)).

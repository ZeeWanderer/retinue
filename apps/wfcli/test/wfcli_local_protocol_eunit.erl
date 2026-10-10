%%%-------------------------------------------------------------------
%% EUnit coverage for native companion JSON protocol.
%%%-------------------------------------------------------------------
-module(wfcli_local_protocol_eunit).

-include_lib("eunit/include/eunit.hrl").

round_trip_binary_keyed_message_test() ->
    Message = #{<<"op">> => <<"publish">>, <<"id">> => 7,
                <<"dataset">> => <<"player">>,
                <<"data">> => #{<<"phase">> => <<"launcher">>}},
    Encoded = iolist_to_binary(wfcli_local_protocol:encode(Message)),
    {ok, Decoded} = wfcli_local_protocol:decode(string:trim(Encoded)),
    ?assertEqual(Message, Decoded).

invalid_json_is_data_error_test() ->
    ?assertMatch({error, {invalid_json, _}}, wfcli_local_protocol:decode(<<"{">>)).

contract_exposes_versioned_interfaces_test() ->
    ?assertEqual(1, wfcli_local_protocol:envelope_version()),
    Interfaces = wfcli_local_protocol:interfaces(),
    ?assertEqual(12, map_size(Interfaces)),
    ?assertEqual(2, maps:get(<<"player">>, Interfaces)),
    ?assertEqual(1, maps:get(<<"build_stats">>, Interfaces)),
    ?assertEqual(2, maps:get(<<"game_metadata">>, Interfaces)),
    ?assertEqual(1, maps:get(<<"assets">>, Interfaces)),
    ?assert(lists:member(<<"companion.command">>,
                         wfcli_local_protocol:features())).

exact_contract_negotiates_optional_features_test() ->
    Required = (wfcli_local_protocol:contract())#{
        <<"features">> => [<<"diagnostics.report">>, <<"unknown.feature">>]},
    Reply = wfcli_local_protocol:negotiate(Required),
    ?assertEqual(true, maps:get(<<"compatible">>, Reply)),
    ?assertEqual([<<"diagnostics.report">>], maps:get(<<"features">>, Reply)).

interface_mismatch_is_rejected_test() ->
    Interfaces = (wfcli_local_protocol:interfaces())#{<<"assets">> => 2},
    Reply = wfcli_local_protocol:negotiate(
              (wfcli_local_protocol:contract())#{<<"interfaces">> => Interfaces}),
    ?assertEqual(false, maps:get(<<"compatible">>, Reply)),
    ?assertMatch([#{<<"interface">> := <<"assets">>}],
                 maps:get(<<"mismatches">>, Reply)).

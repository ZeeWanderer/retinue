%%%-------------------------------------------------------------------
%% EUnit coverage for native companion JSON protocol.
%%%-------------------------------------------------------------------
-module(retinue_local_protocol_eunit).

-include_lib("eunit/include/eunit.hrl").

round_trip_binary_keyed_message_test() ->
    Message = #{<<"op">> => <<"publish">>, <<"id">> => 7,
                <<"dataset">> => <<"player">>,
                <<"data">> => #{<<"phase">> => <<"launcher">>}},
    Encoded = iolist_to_binary(retinue_local_protocol:encode(Message)),
    {ok, Decoded} = retinue_local_protocol:decode(string:trim(Encoded)),
    ?assertEqual(Message, Decoded).

invalid_json_is_data_error_test() ->
    ?assertMatch({error, {invalid_json, _}}, retinue_local_protocol:decode(<<"{">>)).

contract_exposes_versioned_interfaces_test() ->
    ?assertEqual(2, retinue_local_protocol:envelope_version()),
    Interfaces = retinue_local_protocol:interfaces(),
    ?assertEqual(13, map_size(Interfaces)),
    ?assertEqual(1, maps:get(<<"checks">>, Interfaces)),
    ?assertEqual(2, maps:get(<<"player">>, Interfaces)),
    ?assertEqual(1, maps:get(<<"build_stats">>, Interfaces)),
    ?assertEqual(2, maps:get(<<"game_metadata">>, Interfaces)),
    ?assertEqual(1, maps:get(<<"assets">>, Interfaces)),
    ?assertEqual(2, maps:get(<<"market">>, Interfaces)),
    ?assert(lists:member(<<"companion.command">>,
                         retinue_local_protocol:features())).

exact_contract_negotiates_optional_features_test() ->
    Required = (retinue_local_protocol:contract())#{
        <<"features">> => [<<"diagnostics.report">>, <<"unknown.feature">>]},
    Reply = retinue_local_protocol:negotiate(Required),
    ?assertEqual(true, maps:get(<<"compatible">>, Reply)),
    ?assertEqual([<<"diagnostics.report">>], maps:get(<<"features">>, Reply)).

interface_mismatch_is_rejected_test() ->
    Interfaces = (retinue_local_protocol:interfaces())#{<<"assets">> => 2},
    Reply = retinue_local_protocol:negotiate(
              (retinue_local_protocol:contract())#{<<"interfaces">> => Interfaces}),
    ?assertEqual(false, maps:get(<<"compatible">>, Reply)),
    ?assertMatch([#{<<"interface">> := <<"assets">>}],
                 maps:get(<<"mismatches">>, Reply)).

game_is_required_and_interfaces_are_scoped_test() ->
    Contract = retinue_local_protocol:contract(<<"poe2">>),
    ?assertEqual(#{<<"session">> => 2, <<"trade">> => 5, <<"checks">> => 1}, maps:get(<<"interfaces">>, Contract)),
    ?assertEqual(true, maps:get(<<"compatible">>, retinue_local_protocol:negotiate(Contract))),
    lists:foreach(fun(Bad) ->
        ?assertEqual(false, maps:get(<<"compatible">>, retinue_local_protocol:negotiate(Bad)))
    end, [maps:remove(<<"game">>, Contract), Contract#{<<"game">> => <<"unknown">>},
           Contract#{<<"interfaces">> => retinue_local_protocol:interfaces()},
           Contract#{<<"interfaces">> => #{<<"trade">> => 1}},
           Contract#{<<"interfaces">> => #{<<"session">> => 1}}]).

trade_is_poe2_only_test() ->
    ?assertNot(maps:is_key(<<"trade">>, retinue_local_protocol:interfaces(<<"warframe">>))),
    ?assertMatch(#{<<"compatible">> := true}, retinue_local_protocol:negotiate(
        (retinue_local_protocol:contract(<<"poe2">>))#{<<"interfaces">> => #{<<"trade">> => 5}})).

native_session_rpc_contract_test() ->
    ?assertEqual(2, maps:get(poe2_session, retinue_protocol:interfaces())),
    ?assertEqual(5, maps:get(poe2_trade, retinue_protocol:interfaces())),
    Required = (retinue_protocol:contract())#{interfaces => #{poe2_session => 1}},
    ?assertMatch(#{compatible := false, mismatches := [#{interface := poe2_session}]},
                 retinue_protocol:negotiate(Required)).

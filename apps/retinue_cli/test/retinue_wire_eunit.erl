-module(retinue_wire_eunit).
-include_lib("eunit/include/eunit.hrl").

framing_test() ->
    Request = #{<<"id">> => 3, <<"op">> => <<"foundry_view">>},
    {ok, Type, Payload} = retinue_native:encode(wf_native_pb, 'WarframeRequest', Request),
    Bytes = iolist_to_binary(retinue_wire:request(3, Type, Payload)),
    [begin
         <<Prefix:Count/binary, _/binary>> = Bytes,
         ?assertEqual(more, retinue_wire:next(protobuf, Prefix, 4096))
     end || Count <- lists:seq(0, byte_size(Bytes) - 1)],
    {ok, Frame, Bytes} = retinue_wire:next(protobuf, <<Bytes/binary, Bytes/binary>>, 4096),
    {ok, Decoded} = retinue_wire:decode_request(Frame, wf_local_handler),
    ?assertEqual(Request, maps:with(maps:keys(Request), Decoded)),
    ?assertMatch({error, _}, retinue_wire:next(protobuf, <<0:32>>, 4096)),
    ?assertMatch({error, _}, retinue_wire:next(protobuf, <<4097:32>>, 4096)),
    ?assertMatch({error, _}, retinue_wire:decode_request(<<255>>, wf_local_handler)),
    <<_:32, Reply/binary>> = iolist_to_binary(retinue_wire:control(#{<<"id">> => 3, <<"ok">> => true})),
    ?assertEqual({error, expected_request}, retinue_wire:decode_request(Reply, wf_local_handler)),
    <<_:32, WrongType/binary>> = iolist_to_binary(retinue_wire:request(3, <<"unknown">>, Payload)),
    ?assertMatch({error, _}, retinue_wire:decode_request(WrongType, wf_local_handler)),
    ?assertEqual({ok, <<"abc">>, <<"def\n">>}, retinue_wire:next(handshake, <<"abc\ndef\n">>, 4)),
    ?assertMatch({error, _}, retinue_wire:next(handshake, <<"abcd">>, 4)).

typed_projection_preserves_presence_test() ->
    Data = #{<<"revision">> => 18446744073709551615,
             <<"profile">> => #{<<"player_level">> => 0},
             <<"items">> => [#{<<"name">> => <<"Test">>,
               <<"components">> => [#{<<"market_name">> => null, <<"required">> => 2},
                                    #{<<"market_name">> => <<>>, <<"required">> => 0}],
               <<"ignored_upstream_field">> => #{<<"anything">> => [1, 2]}}]},
    {ok, <<"retinue.warframe.v1.FoundrySnapshot">>, Bytes} = wf_native_views:encode(<<"foundry">>, Data),
    Decoded = wf_views_pb:decode_msg(Bytes, 'FoundrySnapshot'),
    ?assertEqual(18446744073709551615, maps:get(<<"revision">>, Decoded)),
    ?assertEqual(0, maps:get(<<"player_level">>, maps:get(<<"profile">>, Decoded))),
    [Item] = maps:get(<<"items">>, Decoded),
    [Absent, Empty] = maps:get(<<"components">>, Item),
    ?assertNot(maps:is_key(<<"market_name">>, Absent)),
    ?assertEqual(<<>>, maps:get(<<"market_name">>, Empty)),
    ?assertNot(maps:is_key(<<"ignored_upstream_field">>, Item)),
    ?assertEqual(dynamic, wf_native_views:encode(<<"query">>, Data)),
    ?assertError({invalid_native_view, 'FoundrySnapshot'}, wf_native_views:encode(<<"foundry">>, #{})).

native_transport_is_explicit_test() ->
    Contract = retinue_local_protocol:contract(),
    ?assertMatch(#{<<"compatible">> := true, <<"transport">> := <<"protobuf">>}, retinue_local_protocol:negotiate(Contract)),
    ?assertMatch(#{<<"compatible">> := true, <<"transport">> := <<"protobuf">>},
                 retinue_local_protocol:negotiate(Contract#{<<"transport">> => <<"protobuf">>})),
    ?assertMatch(#{<<"compatible">> := false},
                 retinue_local_protocol:negotiate(Contract#{<<"transport">> => <<"unknown">>})),
    ?assertMatch(#{<<"compatible">> := false}, retinue_local_protocol:negotiate(
        Contract#{<<"transport">> => <<"protobuf">>, <<"interfaces">> => #{}})).

unavailable_progress_is_not_zero_progress_test() ->
    Data = #{<<"items">> => [], <<"summary">> => #{<<"player_level">> => null,
        <<"player_name">> => null, <<"rank_progress">> => #{<<"available">> => false,
            <<"current">> => null, <<"total">> => null, <<"percent">> => null}}},
    {ok, _, Bytes} = wf_native_views:encode(<<"mastery">>, Data),
    #{<<"summary">> := Summary} = wf_views_pb:decode_msg(Bytes, 'ListingSnapshot'),
    ?assertNot(maps:is_key(<<"player_level">>, Summary)),
    Progress = maps:get(<<"rank_progress">>, Summary),
    ?assertNot(maps:is_key(<<"percent">>, Progress)),
    ?assertNot(maps:is_key(<<"current">>, Progress)).

native_updates_preserve_presence_test() ->
    lists:foreach(fun(Fissures) ->
        Request = #{<<"op">> => <<"notification_settings_set">>, <<"fissures">> => Fissures},
        {ok, Type, Bytes} = retinue_native:encode(wf_native_pb, 'WarframeRequest', Request),
        {ok, Decoded} = wf_native:decode_request(Type, Bytes),
        Expected = case maps:is_key(<<"filters">>, Fissures) of
            true -> Fissures#{<<"filters">> => []};
            false -> Fissures
        end,
        ?assertEqual(Expected, maps:get(<<"fissures">>, Decoded))
    end, [#{<<"mode">> => <<"gui">>}, #{<<"filters">> => #{<<"values">> => []}}]),
    Patch = #{<<"quantity">> => 0, <<"visible">> => false, <<"clear_instance">> => true},
    {ok, Type, Bytes} = retinue_native:encode(wf_native_pb, 'WarframeRequest',
        #{<<"op">> => <<"build_group_update">>, <<"patch">> => Patch}),
    {ok, Decoded} = wf_native:decode_request(Type, Bytes),
    ?assertEqual(#{<<"quantity">> => 0, <<"visible">> => false, <<"instance_id">> => null}, maps:get(<<"patch">>, Decoded)).

asset_requests_preserve_default_source_test() ->
    Asset = #{<<"id">> => <<"mod">>, <<"image_name">> => <<"mod.png">>},
    Request = #{<<"op">> => <<"asset_resolve">>, <<"assets">> => [Asset]},
    {ok, Type, Bytes} = retinue_native:encode(wf_native_pb, 'WarframeRequest', Request),
    {ok, Decoded} = wf_native:decode_request(Type, Bytes),
    ?assertEqual([Asset], maps:get(<<"assets">>, Decoded)).

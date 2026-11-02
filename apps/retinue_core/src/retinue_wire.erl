-module(retinue_wire).
-export([control/1, request/3, reply/4, event/3, decode_request/2, next/3]).

control(#{<<"id">> := Id, <<"ok">> := false, <<"error">> := Error}) ->
    frame(#{<<"body">> => {failure, #{<<"id">> => Id,
        <<"error">> => #{<<"json">> => jsone:encode(Error)}}}});
control(#{<<"id">> := Id, <<"ok">> := true} = Map) ->
    Ack = maps:with([<<"revision">>], Map),
    frame(#{<<"body">> => {ack, Ack#{<<"id">> => Id}}}).

request(Id, Type, Payload) ->
    frame(#{<<"body">> => {request, #{<<"id">> => Id, <<"type_name">> => Type,
                                      <<"payload">> => Payload}}}).

reply(Id, Dataset, Type, Payload) ->
    frame(#{<<"body">> => {reply, #{<<"id">> => Id, <<"type_name">> => Type,
                                    <<"dataset">> => Dataset, <<"payload">> => Payload}}}).

event(Map, Type, Payload) ->
    Headers = maps:with([<<"event">>, <<"subscription">>, <<"source">>, <<"action">>,
                        <<"request_id">>, <<"dataset">>, <<"game">>], Map),
    frame(#{<<"body">> => {event, Headers#{<<"type_name">> => Type, <<"payload">> => Payload}}}).

frame(Packet) ->
    Bytes = retinue_wire_pb:encode_msg(Packet, 'Packet'),
    [<<(byte_size(Bytes)):32/big>>, Bytes].

decode_request(Bytes, Adapter) ->
    try retinue_wire_pb:decode_msg(Bytes, 'Packet') of
        #{<<"body">> := {request, #{<<"id">> := Id, <<"type_name">> := Type,
                                   <<"payload">> := Payload}}} ->
            case Adapter:decode_request(Type, Payload) of
                {ok, Request} when is_map(Request) -> {ok, Request#{<<"id">> => Id}};
                Error -> Error
            end;
        _ -> {error, expected_request}
    catch error:_ -> {error, invalid_protobuf_packet}
    end.

next(protobuf, <<Length:32/big, Rest/binary>>, Limit) when Length > 0, Length =< Limit ->
    case Rest of
        <<Frame:Length/binary, Tail/binary>> -> {ok, Frame, Tail};
        _ -> more
    end;
next(protobuf, <<_:32, _/binary>>, _) -> {error, invalid_frame_length};
next(protobuf, _, _) -> more;
next(handshake, Buffer, Limit) ->
    case binary:match(Buffer, <<"\n">>) of
        {Position, 1} when Position < Limit ->
            <<Line:Position/binary, $\n, Rest/binary>> = Buffer,
            {ok, Line, Rest};
        nomatch when byte_size(Buffer) < Limit -> more;
        _ -> {error, message_too_large}
    end.

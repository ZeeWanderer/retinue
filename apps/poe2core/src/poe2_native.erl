-module(poe2_native).
-export([encode/2, encode_event/2, request/1, decode_request/2, encode_request/1, decode/2]).

encode(Dataset, Data) ->
    case message(Dataset) of
        undefined -> dynamic;
        Type ->
            try encode_value(Type, Data)
            catch error:_ -> error({invalid_native_view, Type}) end
    end.

encode_event(<<"session">>, Data) -> encode(<<"session">>, Data);
encode_event(_, _) -> dynamic.

request(#{<<"op">> := Op} = Data) -> request(request_key(Op, Data));
request(Op) ->
    case lists:keyfind(Op, 1, requests()) of
        {_, Type} -> poe2_native_pb:msg_name_to_fqbin(Type);
        false -> undefined
    end.

encode_request(#{<<"op">> := Op} = Data) ->
    Key = request_key(Op, Data),
    case lists:keyfind(Key, 1, requests()) of
        {_, Type} ->
            try encode_value(Type, normalize_request(Key, Data))
            catch error:_ -> {error, invalid_poe2_request} end;
        false -> {error, unsupported_poe2_request}
    end;
encode_request(_) -> {error, invalid_poe2_request}.

decode_request(TypeName, Bytes) ->
    case [{Op, Type} || {Op, Type} <- requests(),
                       poe2_native_pb:msg_name_to_fqbin(Type) =:= TypeName] of
        [{Op, Type}] ->
            try {ok, normalize_request(Op, decode_value(Type, Bytes))}
            catch error:_ -> {error, invalid_poe2_request} end;
        [] -> {error, unsupported_poe2_request}
    end.

decode(TypeName, Bytes) ->
    try
        Type = poe2_native_pb:fqbin_to_msg_name(TypeName),
        {ok, decode_value(Type, Bytes)}
    catch error:_ -> {error, invalid_poe2_message} end.

encode_value(Type, Data) ->
    Selected = poe2_native_pb:from_json(project(Type, Data, encode), Type),
    {ok, poe2_native_pb:msg_name_to_fqbin(Type), poe2_native_pb:encode_msg(Selected, Type)}.

decode_value(Type, Bytes) ->
    Selected = poe2_native_pb:decode_msg(Bytes, Type),
    project(Type, poe2_native_pb:to_json(Selected, Type), decode).

message(<<"session">>) -> 'Session';
message(<<"session_published">>) -> 'SessionPublished';
message(<<"trade_item">>) -> 'TradeEvaluation';
message(<<"trade_leagues">>) -> 'TradeLeagues';
message(<<"trade_search">>) -> 'TradeResults';
message(<<"trade_fetch">>) -> 'TradePage';
message(<<"trade_restore">>) -> 'TradeRestoration';
message(<<"trade_predict">>) -> 'TradePrediction';
message(<<"trade_error">>) -> 'TradeError';
message(<<"currency_asset">>) -> 'CurrencyAsset';
message(_) -> undefined.

requests() ->
    [{<<"get">>, 'GetSessionRequest'},
     {<<"subscribe">>, 'SubscribeSessionRequest'},
     {<<"unsubscribe">>, 'UnsubscribeSessionRequest'},
     {<<"publish">>, 'PublishSessionRequest'},
     {{<<"publish">>, <<"checks">>}, 'PublishChecksRequest'},
     {<<"trade_item">>, 'TradeItemRequest'},
     {<<"trade_search">>, 'TradeSearchRequest'},
     {<<"trade_fetch">>, 'TradeFetchRequest'},
     {<<"trade_restore">>, 'TradeRestoreRequest'},
     {<<"trade_leagues">>, 'TradeLeaguesRequest'},
     {<<"trade_predict">>, 'TradePredictRequest'},
     {<<"currency_asset">>, 'CurrencyAssetRequest'}].

request_key(<<"publish">>, #{<<"dataset">> := <<"checks">>}) -> {<<"publish">>, <<"checks">>};
request_key(Op, _) -> Op.

normalize_request({Op, Dataset}, Data) ->
    Op = maps:get(<<"op">>, Data, Op),
    Dataset = maps:get(<<"dataset">>, Data, Dataset),
    Data#{<<"op">> => Op, <<"dataset">> => Dataset};
normalize_request(Op, Data) ->
    Op = maps:get(<<"op">>, Data, Op),
    WithOp = Data#{<<"op">> => Op},
    case lists:member(Op, [<<"get">>, <<"subscribe">>, <<"publish">>]) of
        true ->
            <<"session">> = maps:get(<<"dataset">>, Data, <<"session">>),
            WithOp#{<<"dataset">> => <<"session">>};
        false -> WithOp
    end.

%% Wrappers keep the existing map API lossless without making known data opaque.
project('Document', Value, encode) -> #{<<"json">> => iolist_to_binary(json:encode(Value))};
project('Document', #{<<"json">> := Json}, decode) -> json:decode(Json);
project(Type, Value, Direction) ->
    case shape(Type) of
        nullable -> nullable(Type, Value, Direction);
        number -> number(Value, Direction);
        event_value -> event_value(Value, Direction);
        list -> list(Type, Value, Direction);
        object -> object(Type, Value, Direction)
    end.

shape('Number') -> number;
shape('EventValue') -> event_value;
shape(Type) ->
    case atom_to_binary(Type) of
        <<"Nullable", _/binary>> -> nullable;
        Name ->
            case binary:longest_common_suffix([Name, <<"List">>]) of
                4 -> list;
                _ -> object
            end
    end.

nullable(_, null, encode) -> #{<<"null">> => true};
nullable(Type, Value, encode) ->
    #{<<"value">> => field(named_field(Type, <<"value">>), Value, encode)};
nullable(_, #{<<"null">> := true} = Value, decode) when map_size(Value) =:= 1 -> null;
nullable(Type, #{<<"value">> := Value} = Map, decode) when map_size(Map) =:= 1 ->
    field(named_field(Type, <<"value">>), Value, decode);
nullable(_, _, _) -> error(invalid_nullable).

number(null, encode) -> #{<<"null">> => true};
number(Value, encode) when is_integer(Value), Value > 16#7fffffffffffffff ->
    #{<<"unsigned">> => scalar(uint64, Value, encode)};
number(Value, encode) when is_integer(Value) ->
    #{<<"integer">> => scalar(sint64, Value, encode)};
number(Value, encode) when is_float(Value) -> #{<<"real">> => Value};
number(#{<<"null">> := true} = Map, decode) when map_size(Map) =:= 1 -> null;
number(Map, decode) when is_map(Map), map_size(Map) =:= 1 ->
    [{Key, Value}] = maps:to_list(Map),
    true = lists:member(Key, [<<"integer">>, <<"unsigned">>, <<"real">>]),
    field(named_field('Number', Key), Value, decode);
number(_, _) -> error(invalid_number).

event_value(null, encode) -> #{<<"null">> => true};
event_value(Value, encode) when is_integer(Value) ->
    #{<<"integer">> => scalar(sint64, Value, encode)};
event_value(Value, encode) when is_boolean(Value) -> #{<<"boolean">> => Value};
event_value(#{<<"scene">> := _} = Value, encode) ->
    #{<<"state">> => project('SceneState', Value, encode)};
event_value(#{<<"in_town">> := _} = Value, encode) ->
    #{<<"area">> => project('Area', Value, encode)};
event_value(#{<<"class">> := _} = Value, encode) ->
    #{<<"character">> => project('Character', Value, encode)};
event_value(#{<<"null">> := true} = Map, decode) when map_size(Map) =:= 1 -> null;
event_value(Map, decode) when is_map(Map), map_size(Map) =:= 1 ->
    [{Key, Value}] = maps:to_list(Map),
    true = lists:member(Key, [<<"integer">>, <<"boolean">>, <<"state">>, <<"area">>, <<"character">>]),
    field(named_field('EventValue', Key), Value, decode);
event_value(_, _) -> error(invalid_event_value).

list(Type, Values, encode) when is_list(Values) ->
    #{<<"values">> => field(named_field(Type, <<"values">>), Values, encode)};
list(Type, Map, decode) when is_map(Map) ->
    maps:get(<<"values">>, object(Type, Map, decode), []);
list(_, _, _) -> error(invalid_list).

object(Type, Map, Direction) when is_map(Map) ->
    Fields = maps:from_list([{atom_to_binary(maps:get(<<"name">>, F)), F} || F <- fields(Type)]),
    maps:fold(fun(Key, Value, Acc) ->
        case maps:find(Key, Fields) of
            {ok, Field} -> Acc#{Key => field(Field, Value, Direction)};
            error -> error({unknown_field, Type, Key})
        end
    end, #{}, Map);
object(_, _, _) -> error(invalid_object).

named_field(Type, Key) ->
    [Field] = [F || F <- fields(Type), atom_to_binary(maps:get(<<"name">>, F)) =:= Key],
    Field.

fields(Type) ->
    lists:flatmap(fun
        (#{<<"fields">> := Fields}) -> Fields;
        (Field) -> [Field]
    end, poe2_native_pb:fetch_msg_def(Type)).

field(#{<<"occurrence">> := repeated, <<"type">> := Type}, Values, Direction)
  when is_list(Values) -> [value(Type, V, Direction) || V <- Values];
field(#{<<"occurrence">> := repeated}, _, _) -> error(invalid_list);
field(#{<<"type">> := Type}, Value, Direction) -> value(Type, Value, Direction).

value({msg, Type}, Value, Direction) -> project(Type, Value, Direction);
value(Type, Value, Direction) -> scalar(Type, Value, Direction).

%% gpb's protobuf-JSON representation quotes 64-bit integers. Only this bridge
%% sees those strings; both native APIs retain exact Erlang/Rust integers.
scalar(Type, Value, decode) when is_binary(Value),
                               (Type =:= uint64 orelse Type =:= sint64) ->
    scalar(Type, binary_to_integer(Value), encode);
scalar(string, Value, _) when is_binary(Value) -> Value;
scalar(bool, Value, _) when is_boolean(Value) -> Value;
scalar(uint32, Value, _) when is_integer(Value), Value >= 0, Value =< 16#ffffffff -> Value;
scalar(uint64, Value, _) when is_integer(Value), Value >= 0, Value =< 16#ffffffffffffffff -> Value;
scalar(sint32, Value, _) when is_integer(Value), Value >= -16#80000000, Value =< 16#7fffffff -> Value;
scalar(sint64, Value, _) when is_integer(Value), Value >= -16#8000000000000000,
                            Value =< 16#7fffffffffffffff -> Value;
scalar(double, Value, _) when is_number(Value) -> float(Value);
scalar(_, _, _) -> error(invalid_scalar).

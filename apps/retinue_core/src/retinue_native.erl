-module(retinue_native).
-export([encode/3, encode/4, decode/3]).

encode(Codec, Type, Value) ->
    encode(Codec, Type, Value, fun(_, V) -> V end).

encode(Codec, Type, Value, Normalize) ->
    Message = project(Codec, Type, Value, encode, Normalize),
    {ok, Codec:msg_name_to_fqbin(Type), Codec:encode_msg(Message, Type)}.

decode(Codec, Type, Bytes) ->
    project(Codec, Type, Codec:decode_msg(Bytes, Type), decode, fun(_, V) -> V end).

project(_Codec, 'Document', Value, encode, _) -> #{<<"json">> => jsone:encode(Value)};
project(_Codec, 'Document', #{<<"json">> := Json}, decode, _) -> jsone:decode(Json);
project(Codec, Type, Original, Direction, Normalize) when is_map(Original) ->
    Value = Normalize(Type, Original),
    lists:foldl(fun(Field, Acc) ->
        #{<<"name">> := Name, <<"type">> := FieldType,
          <<"occurrence">> := Occurrence, <<"opts">> := Opts} = Field,
        Key = atom_to_binary(Name),
        JsonKey = unicode:characters_to_binary(proplists:get_value(json_name, Opts, Key)),
        {Input, Output} = case Direction of encode -> {JsonKey, Key}; decode -> {Key, JsonKey} end,
        Found = case maps:find(Input, Value) of
            error -> maps:find(Name, Value);
            Result -> Result
        end,
        case Found of
            error -> Acc;
            {ok, null} -> Acc;
            {ok, undefined} -> Acc;
            {ok, Items} when Occurrence =:= repeated, is_list(Items) ->
                Acc#{Output => [field(Codec, FieldType, Item, Direction, Normalize) || Item <- Items]};
            {ok, Item} -> Acc#{Output => field(Codec, FieldType, Item, Direction, Normalize)}
        end
    end, #{}, Codec:find_msg_def(Type)).

field(Codec, {msg, Type}, Value, Direction, Normalize) -> project(Codec, Type, Value, Direction, Normalize);
field(Codec, {map, _Key, Type}, Values, Direction, Normalize) when is_map(Values) ->
    maps:map(fun(_, Value) -> field(Codec, Type, Value, Direction, Normalize) end, Values);
field(_Codec, string, Value, encode, _) when is_atom(Value) -> atom_to_binary(Value);
field(_Codec, _Type, Value, _Direction, _) -> Value.

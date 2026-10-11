%%%-------------------------------------------------------------------
%% JSON conversion for CLI and MCP output.
%%%-------------------------------------------------------------------
-module(retinue_json).

-export([decode/1, encode/1, normalize/1]).

-spec decode(binary() | string()) -> {ok, term()} | {error, term()}.
decode(Line) ->
    try jsone:decode(iolist_to_binary(Line), [{object_format, map}]) of
        Value -> {ok, Value}
    catch
        Class:Reason -> {error, {Class, Reason}}
    end.

-spec encode(term()) -> binary().
encode(Value) ->
    jsone:encode(normalize(Value)).

-doc "Convert Erlang service terms into JSON-safe values without creating atoms.".
-spec normalize(term()) -> term().
normalize({json, Value}) -> raw(Value);
normalize({timestamp, Value}) when is_integer(Value) ->
    list_to_binary(retinue_time:format_millis(Value, #{utc => true, precision => millisecond}));
normalize({timestamp, _}) -> null;
normalize(Value) when is_map(Value) ->
    maps:from_list([{normalize_key(Key), normalize(Item)}
                    || {Key, Item} <- maps:to_list(Value)]);
normalize([]) -> [];
normalize(Value) when is_list(Value) ->
    case io_lib:printable_unicode_list(Value) of
        true -> unicode:characters_to_binary(Value);
        false -> [normalize(Item) || Item <- Value]
    end;
normalize(Value) when is_tuple(Value) ->
    [normalize(Item) || Item <- tuple_to_list(Value)];
normalize(true) -> true;
normalize(false) -> false;
normalize(null) -> null;
normalize(undefined) -> null;
normalize(Value) when is_atom(Value) -> atom_to_binary(Value, utf8);
normalize(Value) when is_binary(Value); is_integer(Value); is_float(Value) -> Value;
normalize(Value) when is_pid(Value); is_reference(Value); is_port(Value); is_function(Value) ->
    iolist_to_binary(io_lib:format("~p", [Value]));
normalize(Value) ->
    iolist_to_binary(io_lib:format("~p", [Value])).

raw(Value) when is_map(Value) ->
    maps:from_list([{normalize_key(Key), raw(Item)} || {Key, Item} <- maps:to_list(Value)]);
raw(Value) when is_list(Value) -> [raw(Item) || Item <- Value];
raw(Value) -> normalize(Value).

normalize_key(Key) when is_binary(Key) -> Key;
normalize_key(Key) when is_atom(Key) -> atom_to_binary(Key, utf8);
normalize_key(Key) when is_integer(Key) -> integer_to_binary(Key);
normalize_key(Key) when is_list(Key) -> unicode:characters_to_binary(Key);
normalize_key(Key) -> iolist_to_binary(io_lib:format("~p", [Key])).

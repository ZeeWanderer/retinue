-module(wfcli_catalog_json).

-export([record/2]).

%% Catalog records use Erlang strings; nested source payloads retain JSON types.
record(Kind, Fields) ->
    maps:map(fun(Key, Value) -> field(Kind, Key, Value) end, Fields).

field(_, _, undefined) -> null;
field(_, levelStats, Value) -> Value;
field(_, drops, Value) -> Value;
field(mod, description, Values) -> strings(Values);
field(_, effects, Values) -> strings(Values);
field(_, max_stats, Values) -> strings(Values);
field(_, abilities, Values) -> strings(Values);
field(_, sourceFiles, Values) -> strings(Values);
field(_, _, Value) when is_list(Value) -> unicode:characters_to_binary(Value);
field(_, _, Value) -> Value.

strings(Values) -> [unicode:characters_to_binary(Value) || Value <- Values].

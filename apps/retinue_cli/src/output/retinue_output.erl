-module(retinue_output).

-export([with/2, options/0, emit/2, json/1, timestamp/1, entity/1, results/1]).

with(Options, Run) ->
    Previous = put(?MODULE, Options),
    try Run()
    after
        case Previous of undefined -> erase(?MODULE); _ -> put(?MODULE, Previous) end
    end.

options() ->
    case get(?MODULE) of undefined -> #{}; Options -> Options end.

emit(Data, Human) ->
    case maps:get(output_format, options(), table) of
        json when is_function(Data, 0) -> json(Data());
        json -> json(Data);
        _ -> Human()
    end.

json(Data) -> io:put_chars([retinue_json:encode(Data), $\n]).

timestamp(Time) -> retinue_time:format_millis(Time, (options())#{precision => millisecond}).

entity(Entry) ->
    Clean = maps:without([haystack, search_fields], Entry),
    Times = case maps:get(type, Entry, undefined) of
        incidents -> [timestamp];
        captures -> [timestamp, expires_at];
        resolution_issue -> [first_seen, last_seen];
        _ -> []
    end,
    Timed = lists:foldl(fun(Key, Acc) ->
        Value = maps:get(Key, Entry, maps:get(atom_to_binary(Key), maps:get(data, Entry, #{}), null)),
        Acc#{Key => {timestamp, Value}}
    end, Clean, Times),
    case maps:find(data, Clean) of
        {ok, Raw} -> Timed#{data := {json, Raw}};
        error -> Timed
    end.

results(#{slice := Entries} = Results) ->
    Results#{slice := [entity(Entry) || Entry <- Entries]}.

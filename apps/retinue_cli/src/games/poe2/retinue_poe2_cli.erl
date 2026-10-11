-module(retinue_poe2_cli).

-export([command/0]).

-ifdef(TEST).
-export([format/2]).
-endif.

command() ->
    #{help => "inspect Path of Exile 2 companion",
      commands => #{
        "status" => #{help => "show observer connection and last-observed state",
                      handler => fun(_) -> run(status) end},
        "events" => #{help => "show recent observed log events", handler => fun(_) -> run(events) end}}}.

run(Action) ->
    case retinue_client:call(poe2, Action) of
        {ok, {error, Reason}} -> retinue_cli_runtime:fail(retinue_client:format_error(Reason));
        {ok, Data} -> retinue_output:emit(Data, fun() -> io:put_chars(format(Action, Data)) end);
        {error, Reason} -> retinue_cli_runtime:fail(retinue_client:format_error(Reason))
    end.

format(status, Data) ->
    ["PoE2 log observer\n",
     field("connection", connection(maps:get(<<"observers">>, Data, undefined))),
     field("source", maps:get(<<"source">>, Data, null)),
     field("revision", maps:get(<<"revision">>, Data, 0)),
     field("last event received", retinue_output:timestamp(maps:get(<<"updated_at">>, Data, null))),
     current(maps:get(<<"current">>, Data, #{}))];
format(events, []) -> "No observed PoE2 log events.\n";
format(events, Events) ->
    ["PoE2 log events (last observed; timestamps are game-local)\n",
     [event(Event) || Event <- Events]].

connection(undefined) -> "unknown";
connection([]) -> "not connected";
connection(Observers) -> io_lib:format("connected (~p)", [length(Observers)]).

current(Current) when map_size(Current) =:= 0 -> "Last-observed state: no observations\n";
current(Current) ->
    ["Last-observed state\n",
     field("zone", maps:get(<<"area_name">>, Current, null)),
     field("area id", maps:get(<<"area_id">>, Current, null)),
     field("area level", maps:get(<<"area_level">>, Current, null)),
     field("seed", maps:get(<<"seed">>, Current, null)),
     field("log client pid", maps:get(<<"client_pid">>, Current, null)),
     actors(maps:get(<<"actors">>, Current, #{}))].

actors(Actors) when map_size(Actors) =:= 0 -> "  actors: none observed\n";
actors(Actors) ->
    ["  actors (observed names)\n",
     [io_lib:format("    ~ts: class ~ts; level ~ts; observed deaths ~ts~n",
                    [Name, value(maps:get(<<"character_class">>, Actor, null)),
                     value(maps:get(<<"character_level">>, Actor, null)),
                     value(maps:get(<<"observed_deaths">>, Actor, null))])
      || {Name, Actor} <- lists:sort(maps:to_list(Actors))]].

event(Event) ->
    [io_lib:format("  ~ts  ~ts~n", [maps:get(<<"timestamp_local">>, Event), event_details(Event)]),
     io_lib:format("    log pid: ~p; tick: ~p ms; offset: ~p~n",
                    [maps:get(<<"client_pid">>, Event), maps:get(<<"tick_ms">>, Event),
                     maps:get(<<"offset">>, Event)])].

event_details(#{<<"kind">> := <<"area_generated">>} = Event) ->
    io_lib:format("Area generated: ~ts; level ~p; seed ~p",
                   [maps:get(<<"area_id">>, Event), maps:get(<<"area_level">>, Event),
                    maps:get(<<"seed">>, Event)]);
event_details(#{<<"kind">> := <<"area_name">>, <<"area_name">> := Name}) ->
    io_lib:format("Zone: ~ts", [Name]);
event_details(#{<<"kind">> := <<"character_level">>} = Event) ->
    io_lib:format("Character level: ~ts; ~ts; level ~p",
                   [maps:get(<<"character">>, Event), maps:get(<<"character_class">>, Event),
                    maps:get(<<"character_level">>, Event)]);
event_details(#{<<"kind">> := <<"death">>, <<"character">> := Name}) ->
    io_lib:format("Death: ~ts", [Name]).

field(Label, Value) -> io_lib:format("  ~s: ~ts~n", [Label, value(Value)]).

value(null) -> "unknown";
value(Value) -> retinue_text:to_list(Value).

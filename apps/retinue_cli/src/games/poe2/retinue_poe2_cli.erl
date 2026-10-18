-module(retinue_poe2_cli).

-export([command/0]).

-ifdef(TEST).
-export([format/2]).
-endif.

command() ->
    #{help => "inspect Path of Exile 2 companion",
      commands => #{
        "status" => #{help => "show companion connection and native session state",
                      handler => fun(_) -> run(status) end},
        "events" => #{help => "show recent native session changes", handler => fun(_) -> run(events) end}}}.

run(Action) ->
    case retinue_client:call(poe2, Action) of
        {ok, {error, Reason}} -> retinue_cli_runtime:fail(retinue_client:format_error(Reason));
        {ok, Data} -> retinue_output:emit(Data, fun() -> io:put_chars(format(Action, Data)) end);
        {error, Reason} -> retinue_cli_runtime:fail(retinue_client:format_error(Reason))
    end.

format(status, Data) ->
    ["PoE2 native session\n",
     field("connection", connection(maps:get(<<"observers">>, Data, undefined))),
     field("status", maps:get(<<"status">>, Data, <<"waiting">>)),
     field("revision", maps:get(<<"revision">>, Data, 0)),
     field("last observed", retinue_output:timestamp(maps:get(<<"observed_at">>, Data, null))),
     field("last changed", retinue_output:timestamp(maps:get(<<"updated_at">>, Data, null))),
     process(maps:get(<<"process">>, Data, null)),
     current(maps:get(<<"current">>, Data, null)),
     case maps:get(<<"error">>, Data, null) of null -> []; Error -> field("error", Error) end];
format(events, []) -> "No observed PoE2 session events.\n";
format(events, Events) ->
    ["PoE2 session events\n", [event(Event) || Event <- Events]].

connection(undefined) -> "unknown";
connection([]) -> "not connected";
connection(Observers) -> io_lib:format("connected (~p)", [length(Observers)]).

process(null) -> [];
process(Process) ->
    [field("process", maps:get(<<"pid">>, Process)),
     field("executable", maps:get(<<"executable">>, Process))].

current(null) -> "Native state: no observation\n";
current(Current) ->
    ["Native state\n",
     field("scene", scene(maps:get(<<"scene">>, Current))),
     area(maps:get(<<"area">>, Current)),
     player(maps:get(<<"player">>, Current)),
     panels(maps:get(<<"panels">>, Current)),
     issues(maps:get(<<"issues">>, Current))].

area(null) -> field("zone", null);
area(Area) ->
    [field("zone", maps:get(<<"name">>, Area)),
     field("in town", case maps:get(<<"in_town">>, Area) of true -> "yes"; false -> "no" end)].

player(null) -> field("character", null);
player(Player) ->
    [field(Label, maps:get(Key, Player)) || {Label, Key} <-
        [{"character", <<"name">>}, {"class", <<"class">>}, {"level", <<"level">>},
         {"XP", <<"experience">>}, {"gold", <<"gold">>}, {"life", <<"life">>}]].

panels(Panels) when map_size(Panels) =:= 0 -> field("panels", null);
panels(Panels) ->
    ["  panels\n", [io_lib:format("    ~ts: ~ts~n", [label(Name), visible(Value)])
                    || {Name, Value} <- lists:sort(maps:to_list(Panels))]].

issues([]) -> [];
issues(Issues) ->
    ["  read issues\n", [io_lib:format("    ~ts: ~ts~n", [value(maps:get(<<"field">>, Issue)),
                                                        value(maps:get(<<"reason">>, Issue))])
                         || Issue <- Issues]].

event(Event) ->
    io_lib:format("  ~ts  ~ts~n", [retinue_output:timestamp(maps:get(<<"observed_at">>, Event)),
                                  event_details(Event)]).

event_details(#{<<"kind">> := <<"state">>, <<"to">> := State}) ->
    io_lib:format("Scene: ~ts (~ts)", [scene(maps:get(<<"scene">>, State)),
                                     string:join([value(S) || S <- maps:get(<<"states">>, State)], ", ")]);
event_details(#{<<"kind">> := <<"area">>, <<"to">> := Area}) ->
    io_lib:format("Zone: ~ts", [value(maps:get(<<"name">>, Area))]);
event_details(#{<<"kind">> := <<"character">>, <<"to">> := Character}) ->
    io_lib:format("Character: ~ts; class ~ts", [value(maps:get(<<"name">>, Character)),
                                              value(maps:get(<<"class">>, Character))]);
event_details(#{<<"kind">> := <<"panel">>, <<"panel">> := Panel, <<"to">> := Visible}) ->
    io_lib:format("Panel: ~ts; ~ts", [label(Panel), visible(Visible)]);
event_details(#{<<"kind">> := <<"death">>, <<"character">> := Name}) ->
    io_lib:format("Death: ~ts (observed life reached zero)", [value(Name)]);
event_details(#{<<"kind">> := Kind, <<"character">> := Name, <<"from">> := From, <<"to">> := To}) ->
    Label = case Kind of <<"xp">> -> "XP"; <<"level">> -> "Level"; <<"gold">> -> "Gold" end,
    io_lib:format("~s: ~ts; ~ts -> ~ts", [Label, value(Name), value(From), value(To)]).

scene(<<"select_character">>) -> "character selection";
scene(<<"in_game">>) -> "in game";
scene(<<"escape_menu">>) -> "escape menu";
scene(Scene) -> value(Scene).

label(Name) -> string:replace(value(Name), "_", " ", all).
visible(true) -> "open";
visible(false) -> "closed";
visible(null) -> "unknown".
field(Label, Value) -> io_lib:format("  ~s: ~ts~n", [Label, value(Value)]).

value(null) -> "unknown";
value(Value) ->
    [case C < 32 orelse (C >= 127 andalso C < 160) of true -> $ ; false -> C end
     || C <- lists:flatten(retinue_text:to_list(Value))].

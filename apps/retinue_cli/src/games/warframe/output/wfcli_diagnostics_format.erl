%%%-------------------------------------------------------------------
%% Terminal presentation for daemon resolution diagnostics.
%%%-------------------------------------------------------------------
-module(wfcli_diagnostics_format).

-export([print/2, print_query/2, print_logs/2, format_log/1, timestamp/1]).

print_logs(Reports, json) ->
    retinue_output:json(#{logs => [case Report of
        #{entries := Entries} -> Report#{entries := [json_entry(Entry) || Entry <- Entries]};
        _ -> Report
    end || Report <- Reports]});
print_logs(Reports, table) ->
    lists:foreach(fun(Report) -> io:put_chars(format_log(Report)) end, Reports).

format_log(#{application := App, path := Path} = Report) ->
    Header = io_lib:format("~s incidents (~ts)~n", [App, Path]),
    Body = case Report of
        #{error := Error} -> io_lib:format("  unavailable: ~ts~n", [Error]);
        #{missing := true} -> "  log not created\n";
        #{entries := []} -> "  no entries\n";
        #{entries := Entries} -> [format_entry(Entry) || Entry <- Entries]
    end,
    [Header, truncation_notice(Report), Body].

truncation_notice(#{truncated := true}) -> "  note: limited to the last 1 MiB of this log\n";
truncation_notice(_) -> [].

format_entry(#{<<"event">> := Event, <<"message">> := Message} = Entry) ->
    io_lib:format("  ~ts ~ts ~ts: ~ts~n",
                  [timestamp(maps:get(<<"timestamp_ms">>, Entry, undefined)),
                   text(maps:get(<<"level">>, Entry, <<"info">>)), Event, Message]);
format_entry(#{<<"message">> := Message}) -> io_lib:format("  ~ts~n", [Message]).

timestamp(Time) when is_integer(Time), Time >= 0, Time < 253402300800000 ->
    retinue_output:timestamp(Time);
timestamp(_) -> "unknown".

json_entry(Entry) ->
    Entry#{<<"timestamp">> => {timestamp, maps:get(<<"timestamp_ms">>, Entry, undefined)}}.

-doc "Render current unresolved metadata in table or JSON form.".
-spec print([map()], table | json) -> ok.
print(Issues, json) ->
    retinue_output:json(#{count => length(Issues), issues => [
        Issue#{<<"first_seen">> => {timestamp, maps:get(<<"first_seen">>, Issue, undefined)},
               <<"last_seen">> => {timestamp, maps:get(<<"last_seen">>, Issue, undefined)}}
        || Issue <- Issues]});
print([], table) ->
    io:format("No unresolved metadata.~n");
print(Issues, table) ->
    io:format("Unresolved issues: ~p~n~n", [length(Issues)]),
    print_table(Issues).

-doc "Render diagnostics returned through unified query.".
-spec print_query(map(), map()) -> ok.
print_query(Query, #{kind := Kind} = Results) when Kind =:= incidents; Kind =:= captures ->
    print_observations(Kind, Query, Results);
print_query(Query, Results) ->
    Entries = maps:get(slice, Results, []),
    io:format("Matches: ~p (showing ~p)~n~n",
              [maps:get(total, Results, 0), maps:get(shown, Results, 0)]),
    case maps:get(output_format, Query, table) of
        block -> lists:foreach(fun print_block/1, Entries);
        table -> print_table([maps:get(data, Entry, #{}) || Entry <- Entries])
    end.

print_observations(Kind, Query, Results) ->
    [io:format("~ts: ~ts", [maps:get(path, Log), truncation_notice(Log)])
     || #{truncated := true} = Log <- maps:get(logs, Results, [])],
    Fields = case Kind of
        incidents -> [{"Time", <<"timestamp">>}, {"App", <<"application">>},
                      {"Level", <<"level">>}, {"Event", <<"name">>}, {"Message", <<"message">>}];
        captures -> [{"Time", <<"timestamp">>}, {"Capture", <<"name">>},
                     {"State", <<"state">>}, {"Current", <<"current">>},
                     {"Output", <<"directory">>}, {"Error", <<"error">>}]
    end,
    Entries = maps:get(slice, Results),
    io:format("Matches: ~p (showing ~p)~n~n", [maps:get(total, Results), maps:get(shown, Results)]),
    case maps:get(output_format, Query, table) of
        block -> lists:foreach(fun(Entry) ->
            Data = maps:get(data, Entry),
            [io:format("  ~ts: ~ts~n", [Label, observation_value(Key, Data)]) || {Label, Key} <- Fields],
            io:nl()
        end, Entries);
        table ->
            Lines = retinue_table:render_lines([Label || {Label, _} <- Fields],
                [[observation_value(Key, maps:get(data, Entry)) || {_, Key} <- Fields]
                 || Entry <- Entries], #{}),
            [io:format("~ts~n", [Line]) || Line <- Lines], ok
    end.

observation_value(<<"timestamp">>, Data) -> timestamp(maps:get(<<"timestamp">>, Data, undefined));
observation_value(Key, Data) ->
    case maps:get(Key, Data, <<>>) of null -> ""; Value -> text(Value) end.

print_table([]) -> io:format("no entries~n");
print_table(Issues) ->
    Headers = ["Kind", "Name", "Identity", "Collection", "Reason"],
    Rows = [[text(maps:get(<<"kind">>, Issue, <<>>)),
             text(maps:get(<<"fallback">>, Issue, <<>>)),
             text(maps:get(<<"identity">>, Issue, <<>>)),
             text(maps:get(<<"collection">>, Issue, <<>>)),
             text(maps:get(<<"reason">>, Issue, <<>>))]
            || Issue <- Issues],
    lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end,
                  retinue_table:render_lines(Headers, Rows, #{})).

print_block(Entry) ->
    Issue = maps:get(data, Entry, #{}),
    io:format("~ts: ~ts~n", [maps:get(<<"kind">>, Issue, <<>>),
                             maps:get(<<"fallback">>, Issue,
                                      maps:get(<<"identity">>, Issue, <<>>))]),
    io:format("  identity: ~ts~n", [maps:get(<<"identity">>, Issue, <<>>)]),
    io:format("  reason: ~ts~n~n", [maps:get(<<"reason">>, Issue, <<>>)]).

text(Value) -> retinue_text:to_list(Value).

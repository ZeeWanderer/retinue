%%%-------------------------------------------------------------------
%% Terminal rendering for daemon-projected worldstate results.
%%%-------------------------------------------------------------------
-module(wf_worldstate_output).

-export([print_daemon_result/2, json_result/1, print_entries/4, print_query_extracts/2, print_daemon_source/1,
         daemon_source_text/1, maybe_print_stale/1, load_opts/1,
         default_watch_specs/1, print_watch_results/2, resolve_columns/2,
         columns_with_extras/3, maybe_sort_watch_entries/3]).
-ifdef(TEST).
-export([split_signature/2, removed_entries_table/2, removed_extract_rows/2,
         maybe_extra_columns/3, merge_inline_entries/5]).
-endif.

-type entry() :: map().
-type opts() :: map().
-type output_format() :: table | block.
-type columns() :: [atom() | {extra, string()}].

-doc "Render a complete daemon worldstate response using parsed CLI options.".
-spec print_daemon_result(map(), map()) -> ok.
print_daemon_result(Result, #{output_format := json}) ->
    retinue_output:json(json_result(Result));
print_daemon_result(Result, Parsed) ->
    print_daemon_source(Result),
    Format = maps:get(output_format, Parsed, block),
    Columns = resolve_columns(Format, Parsed),
    Opts = maps:get(opts, Result, load_opts(Parsed)),
    Entries = maps:get(entries, Result, []),
    Query = maps:get(search, Parsed, undefined),
    TypeFilter = maps:get(type_filter, Parsed, undefined),
    case maps:get(kind, Result) of
        summary ->
            io:format("Worldstate summary (counts):~n", []),
            print_summary(lists:sort(maps:to_list(maps:get(summary, Result, #{}))), Format);
        inventory when Query =:= undefined ->
            io:format("Inventory entries: ~p~n~n", [length(Entries)]),
            print_entries(Entries, Opts, Format, Columns);
        inventory ->
            case Entries of
                [] -> io:format("no matches for ~ts~n", [Query]);
                _ ->
                    io:format("Inventory matches for ~s: ~p~n~n", [Query, length(Entries)]),
                    print_entries(Entries, Opts, Format, Columns)
            end;
        entries when Query =:= undefined ->
            io:format("Entries for ~p: ~p~n~n", [TypeFilter, length(Entries)]),
            print_entries(Entries, Opts, Format, Columns);
        entries ->
            case Entries of
                [] -> io:format("no matches for ~ts~n", [Query]);
                _ ->
                    print_matches_header(Query, TypeFilter, length(Entries)),
                    print_entries(Entries, Opts, Format, Columns)
            end
    end.

-doc "Print worldstate source and stale-cache metadata.".
-spec print_daemon_source(map()) -> ok.
print_daemon_source(Result) ->
    io:format("~ts ~ts~n", [source_heading(Result), daemon_source_text(Result)]),
    maybe_print_stale(Result).

source_heading(#{inventory := teshin}) -> "Teshin data";
source_heading(_) -> "Worldstate".

-doc "Format daemon snapshot provenance without terminal side effects.".
-spec daemon_source_text(map()) -> string().
daemon_source_text(Result) ->
    Source = maps:get(source, Result, undefined),
    Origin = maps:get(snapshot_origin, Result, undefined),
    AgeMs = maps:get(snapshot_age_ms, Result,
                     maps:get(fetched_age_ms, Result, undefined)),
    SourcePart = lists:flatten(io_lib:format("source: ~p", [Source])),
    OriginParts = case Origin =/= undefined andalso Origin =/= Source of
        true -> [lists:flatten(io_lib:format("origin: ~p", [Origin]))];
        false -> []
    end,
    AgeParts = case AgeMs of
        Value when is_integer(Value) ->
            [lists:flatten(io_lib:format("age: ~ps", [max(0, Value) div 1000]))];
        _ -> []
    end,
    lists:flatten(lists:join(", ", [SourcePart | OriginParts ++ AgeParts])).

-doc "Print a warning when a daemon response used stale data.".
-spec maybe_print_stale(map()) -> ok.
maybe_print_stale(#{stale := true, fetch_error := Reason}) when Reason =/= undefined ->
    io:format("warning: using stale worldstate after fetch error: ~p~n", [Reason]);
maybe_print_stale(#{stale := true}) ->
    io:format("warning: using stale cached worldstate~n", []);
maybe_print_stale(_) -> ok.
-doc "Translate parsed worldstate CLI options into a daemon load-options map.".
-spec load_opts(map()) -> map().
load_opts(Parsed) ->
    Resolve = maps:get(resolve_items, Parsed, true),
    Opts0 = #{refresh => maps:get(refresh, Parsed, false),
              ttl => maps:get(ttl, Parsed, 60),
              resolve_items => Resolve,
              raw => maps:get(raw, Parsed, false),
              utc => maps:get(utc, Parsed, false) orelse maps:get(output_format, Parsed, table) =:= json,
              search_raw => maps:get(raw, Parsed, false),
              event_lang => maps:get(event_lang, Parsed, undefined)},
    case maps:get(cache, Parsed, undefined) of
        undefined -> Opts0;
        C -> Opts0#{cache => filename:absname(C)}
    end.

json_result(Result) ->
    Clean = maps:without([opts, parsed_query], Result),
    case maps:find(entries, Clean) of
        {ok, Entries} ->
            Extracts = maps:get(extracts, maps:get(parsed_query, Result, #{}), []),
            Clean#{entries := [json_entry(E, Extracts) || E <- Entries]};
        error -> Clean
    end.

json_entry(Entry, []) -> retinue_output:entity(Entry);
json_entry(Entry, Extracts) ->
    #{id => maps:get(id, Entry, undefined), name => maps:get(name, Entry, undefined),
      extracts => maps:from_list([{Path, {json, retinue_data_extract:extract_values(
                                                  maps:get(data, Entry, #{}), Path)}}
                                 || Path <- Extracts])}.

-spec print_entries([entry()], opts(), output_format(), columns()) -> ok.
print_entries([], _Opts, _Format, _Columns) ->
    io:format("no entries~n", []);
print_entries(List, Opts, block, _Columns) ->
    lists:foreach(
      fun(E) ->
          print_entry(E, Opts)
      end,
      List);
print_entries(List, Opts, table, Columns) ->
    print_entries_table(List, Opts, Columns).

-doc "Render explicit query extracts from normalized or raw-root worldstate entities.".
-spec print_query_extracts([entry()], [string()]) -> ok.
print_query_extracts(Entries, Extracts) ->
    print_extracts(Entries, Extracts, none, #{}, #{}, true).

print_entries(List, Opts, block, _Columns, Style, Diff, Initial) ->
    Statuses = row_statuses(List, Diff, Initial),
    lists:foreach(
      fun({Entry, Status}) ->
          Text = wf_worldstate_format:format(Entry, Opts),
          Line = case Style of
              inline -> wf_worldstate_watch:color_line(Text, Status);
              _ -> Text
          end,
          case maps:get(id, Entry, undefined) of
              undefined -> io:format("~ts~n~n", [Line]);
              Id -> io:format("~ts~n  id: ~ts~n~n", [Line, Id])
          end
      end,
      lists:zip(List, Statuses));
print_entries(List, Opts, table, Columns, Style, Diff, Initial) ->
    print_entries_table(List, Opts, Columns, Style, Diff, Initial).

print_entry(#{id := Id} = Entry, Opts) ->
    Text = wf_worldstate_format:format(Entry, Opts),
    io:format("~ts~n  id: ~ts~n~n", [Text, Id]);
print_entry(Entry, Opts) ->
    Text = wf_worldstate_format:format(Entry, Opts),
    io:format("~ts~n~n", [Text]).

print_entries_table(List, Opts, Columns) ->
    RowMaps = [entry_row_map(E, Opts) || E <- List],
    {Columns1, Rows} = columns_with_extras_rows_maps(RowMaps, Columns, Opts),
    Headers = [column_label(C) || C <- Columns1],
    TableOpts = table_opts(Opts, Columns1, RowMaps),
    print_table(Headers, Rows, [], TableOpts).

print_entries_table(List, Opts, Columns, Style, Diff, Initial) ->
    RowMaps = [entry_row_map(E, Opts) || E <- List],
    {Columns1, Rows} = columns_with_extras_rows_maps(RowMaps, Columns, Opts),
    Headers = [column_label(C) || C <- Columns1],
    Statuses = row_statuses(List, Diff, Initial),
    TableOpts = table_opts(Opts, Columns1, RowMaps),
    print_table(Headers, Rows, statuses_for_style(Style, Statuses, Initial), TableOpts).

-doc "Build the implicit watch spec for a command-specific watch invocation.".
-spec default_watch_specs(map()) -> [map()].
default_watch_specs(Parsed) ->
    case maps:get(type_filter, Parsed, undefined) of
        undefined -> [];
        Type ->
            Query = maps:get(search, Parsed, undefined),
            Label = case Query of
                undefined -> atom_to_list(Type);
                _ -> atom_to_list(Type) ++ " (" ++ retinue_text:to_list(Query) ++ ")"
            end,
            [#{label => Label, type_filter => Type, query => Query}]
    end.

-doc "Render one decorated watch-spec result.".
-spec print_watch_results(map(), map()) -> ok.
print_watch_results(Data, Parsed) ->
    Entries = maps:get(entries, Data),
    Format = maps:get(format, Data),
    Columns = maps:get(columns, Data),
    Opts = maps:get(opts, Data),
    Extracts = maps:get(extracts, Data),
    Diff = maps:get(diff, Data, #{}),
    Snapshot = maps:get(snapshot, Data, #{}),
    PrevSnap = maps:get(prev_snapshot, Data, undefined),
    Initial = maps:get(initial, Data, false),
    Style = maps:get(diff_style, Parsed, inline),
    case Style of
        diff ->
            print_diff_lines_min(Diff);
        list ->
            print_diff_lines(Diff);
        inline ->
            print_watch_entries(Entries, Format, Columns, Opts, Extracts, inline, Diff, Snapshot, PrevSnap, Initial);
        none ->
            print_watch_entries(Entries, Format, Columns, Opts, Extracts, none, Diff, Snapshot, PrevSnap, Initial)
    end.

print_watch_entries(Entries, Format, Columns, Opts, Extracts, Style, Diff, Snapshot, PrevSnap, Initial) ->
    case Extracts of
        [] ->
            Entries1 = maybe_inline_removed_entries(Entries, Format, Columns, Style, Diff, PrevSnap),
            print_entries(Entries1, Opts, Format, Columns, Style, Diff, Initial),
            maybe_print_removed_block(Format, Style, Diff, Snapshot);
        _ ->
            print_extracts(Entries, Extracts, Style, Diff, Snapshot, Initial)
    end.

print_extracts(Entries, Extracts, Style, Diff, Snapshot, Initial) ->
    Headers = ["Name" | Extracts],
    Rows0 = [
        [maps:get(name, E, "") |
         [retinue_data_extract:extract_string(maps:get(data, E, #{}), Path)
          || Path <- Extracts]]
        || E <- Entries
    ],
    Statuses0 = row_statuses(Entries, Diff, Initial),
    {Rows1, Statuses1} = case Style of
        inline ->
            RemovedRows = removed_extract_rows(Diff, Extracts),
            {Rows0 ++ RemovedRows, Statuses0 ++ lists:duplicate(length(RemovedRows), remove)};
        _ ->
            {Rows0, Statuses0}
    end,
    _ = Snapshot,
    print_table(Headers, Rows1, statuses_for_style(Style, Statuses1, Initial)).

print_diff_lines(Diff) ->
    Lines = wf_worldstate_watch:format_diff_lines(Diff, true),
    case Lines of
        [] -> ok;
        _ ->
            io:format("Changes: ~p~n", [length(Lines)]),
            lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end, Lines)
    end.
print_diff_lines_min(Diff) ->
    Lines = wf_worldstate_watch:format_diff_lines_min(Diff, true),
    case Lines of
        [] -> ok;
        _ ->
            io:format("Changes: ~p~n", [length(Lines)]),
            lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end, Lines)
    end.

maybe_inline_removed_entries(Entries, Format, Columns, inline, Diff, PrevSnap) ->
    case Format of
        table -> merge_inline_entries(Entries, Columns, Diff, PrevSnap, #{columns => Columns});
        _ -> Entries
    end;
maybe_inline_removed_entries(Entries, _Format, _Columns, _Style, _Diff, _PrevSnap) ->
    Entries.

maybe_print_removed_block(block, inline, Diff, _Snapshot) ->
    Removed = maps:get(removed, Diff, []),
    lists:foreach(
      fun({_Key, Prev}) ->
          Value = retinue_text:to_list(maps:get(value, Prev, "")),
          Line = wf_worldstate_watch:color_line(Value, remove),
          case Line of
              "" -> ok;
              _ -> io:format("~ts~n~n", [Line])
          end
      end,
      Removed);
maybe_print_removed_block(_Format, _Style, _Diff, _Snapshot) ->
    ok.

-ifdef(TEST).
removed_entries_table(Diff, Columns) ->
    Removed = maps:get(removed, Diff, []),
    [removed_entry_table(Key, Prev, Columns) || {Key, Prev} <- Removed].
-endif.

removed_entry_table(Key, Prev, Columns) ->
    Name = retinue_text:to_list(maps:get(name, Prev, "")),
    RowMap0 = maps:get(row_map, Prev, undefined),
    Extra = maps:get(extra_fields, Prev, #{}),
    RowMap = case RowMap0 of
        undefined ->
            Value = retinue_text:to_list(maps:get(value, Prev, "")),
            Values = split_signature(Value, length(Columns)),
            maps:from_list(lists:zip(Columns, Values));
        _ -> RowMap0
    end,
    #{id => retinue_text:to_list(Key), name => Name, row_map => RowMap, extra_fields => Extra}.

removed_extract_rows(Diff, Extracts) ->
    Removed = maps:get(removed, Diff, []),
    Count = length(Extracts),
    [
        [retinue_text:to_list(maps:get(name, Prev, "")) | split_signature(maps:get(value, Prev, ""), Count)]
        || {_Key, Prev} <- Removed
    ].

split_signature(Value, Count) ->
    Str = retinue_text:to_list(Value),
    Parts = case Str of
        "" -> [];
        _ -> string:split(Str, " | ", all)
    end,
    case Count of
        0 -> Parts;
        _ when length(Parts) =:= Count -> Parts;
        _ when length(Parts) < Count ->
            Parts ++ lists:duplicate(Count - length(Parts), "");
        _ ->
            {Head, Tail} = lists:split(Count - 1, Parts),
            Head ++ [string:join(Tail, " | ")]
    end.

print_summary(List, block) ->
    Width = max_label_width([atom_to_list(Type) || {Type, _} <- List], 0),
    lists:foreach(
      fun({Type, Count}) ->
          Label = pad_right(atom_to_list(Type), Width),
          io:format("  ~s  ~p~n", [Label, Count])
      end,
      List);
print_summary(List, table) ->
    Headers = ["Type", "Count"],
    Rows = [[atom_to_list(Type), integer_to_list(Count)] || {Type, Count} <- List],
    print_table(Headers, Rows).

print_matches_header(Query, undefined, Count) ->
    io:format("Matches for ~ts: ~p~n~n", [Query, Count]);
print_matches_header(Query, TypeFilter, Count) ->
    io:format("Matches for ~ts (type ~p): ~p~n~n", [Query, TypeFilter, Count]).

max_label_width([], Width) -> Width;
max_label_width([Label | Rest], Width) ->
    NewWidth = max(Width, length(Label)),
    max_label_width(Rest, NewWidth).

pad_right(Str, Width) ->
    retinue_tty:pad_right(Str, Width).

-doc "Select terminal table columns for parsed worldstate CLI options.".
-spec resolve_columns(output_format(), map()) -> columns().
resolve_columns(table, Parsed) ->
    case maps:get(inventory, Parsed, false) of
        true -> wf_worldstate_schema:columns_for_inventory(maps:get(type_filter, Parsed, undefined));
        false ->
            case maps:get(type_filter, Parsed, undefined) of
                undefined -> wf_worldstate_schema:default_table_columns();
                Type -> wf_worldstate_schema:columns_for_type(Type)
            end
    end;
resolve_columns(_, _Parsed) ->
    [].

table_row_from_map(Map, Columns) ->
    [column_value(Map, Col) || Col <- Columns].

normalize_cell(Value) ->
    lists:flatten(retinue_text:to_list(Value)).

prune_columns(Columns, RowMaps) ->
    Keep = [Col || Col <- Columns, column_has_value(Col, RowMaps)],
    {Keep, [table_row_from_map(Map, Keep) || Map <- RowMaps]}.

column_has_value({extra, Key}, RowMaps) ->
    lists:any(
      fun(Map) ->
          Extra = maps:get(extra_fields, Map, #{}),
          cell_present(maps:get(Key, Extra, ""))
      end,
      RowMaps);
column_has_value(Col, RowMaps) ->
    lists:any(fun(Map) -> cell_present(maps:get(Col, Map, "")) end, RowMaps).

cell_present(Value) ->
    Str = string:trim(normalize_cell(Value)),
    Str =/= "" andalso Str =/= "null".

column_label({extra, Key}) -> Key;
column_label(Column) ->
    Spec = wf_worldstate_schema:column_spec(Column),
    maps:get(label, Spec, atom_to_list(Column)).

print_table(Headers, Rows) ->
    print_table(Headers, Rows, [], #{}).

print_table(Headers, Rows, Statuses) ->
    print_table(Headers, Rows, Statuses, #{}).

print_table(Headers, Rows, Statuses, TableOpts) ->
    ColorFun = fun(Line, Status) ->
        case Status of
            add -> wf_worldstate_watch:color_line(Line, add);
            remove -> wf_worldstate_watch:color_line(Line, remove);
            change -> wf_worldstate_watch:color_line(Line, change);
            _ -> Line
        end
    end,
    Lines = retinue_table:render_lines(
        Headers,
        Rows,
        TableOpts#{row_statuses => Statuses, status_color_fun => ColorFun}
    ),
    lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end, Lines),
    io:format("~n", []).

statuses_for_style(inline, Statuses, false) -> Statuses;
statuses_for_style(_, _Statuses, _Initial) -> [].

table_opts(Opts, Columns, RowMaps) ->
    #{column_specs => [wf_worldstate_schema:column_spec(C) || C <- Columns],
      row_maps => RowMaps,
      intent => table_intent(Opts, Columns),
      group_resolver => fun group_resolver/4}.

table_intent(_Opts, _Columns) ->
    #{}.

group_resolver(Label, _Rows, RowMaps, Opts) ->
    case wf_worldstate_schema:type_from_label(Label) of
        undefined ->
            {[], [], Opts};
        Type ->
            Columns = wf_worldstate_schema:columns_for_type(Type),
            {Columns1, Rows1} = columns_with_extras_rows_maps(RowMaps, Columns, Opts),
            Headers1 = [column_label(C) || C <- Columns1],
            Opts1 = table_opts(Opts, Columns1, RowMaps),
            {Headers1, Rows1, Opts1}
    end.

entry_row_map(Entry, _Opts) ->
    Row0 = maps:get(row_map, Entry, #{}),
    Extra = maps:get(extra_fields, Entry, #{}),
    case map_size(Extra) of
        0 -> Row0;
        _ -> Row0#{extra_fields => Extra}
    end.

-doc "Add useful sparse columns found in projected entries.".
-spec columns_with_extras([entry()], columns(), opts()) -> columns().
columns_with_extras(Entries, Columns, Opts) ->
    {Columns1, _Rows} = columns_with_extras_rows(Entries, Columns, Opts),
    Columns1.

columns_with_extras_rows(Entries, Columns, Opts) when is_list(Entries) ->
    RowMaps = [entry_row_map(E, Opts) || E <- Entries],
    columns_with_extras_rows_maps(RowMaps, Columns, Opts);
columns_with_extras_rows(RowMaps, Columns, Opts) ->
    columns_with_extras_rows_maps(RowMaps, Columns, Opts).

columns_with_extras_rows_maps(RowMaps, Columns, Opts) ->
    Columns0 = Columns ++ maybe_extra_columns(RowMaps, Columns, Opts),
    prune_columns(Columns0, RowMaps).

column_value(Map, {extra, Key}) ->
    Extra = maps:get(extra_fields, Map, #{}),
    normalize_cell(maps:get(Key, Extra, ""));
column_value(Map, Col) ->
    normalize_cell(maps:get(Col, Map, "")).

extra_columns(RowMaps, Columns) ->
    Existing = [string:lowercase(column_label(C)) || C <- Columns],
    Counts = extra_key_counts(RowMaps, #{}),
    Candidates = [
        {Key, Count} || {Key, Count} <- maps:to_list(Counts),
        not lists:member(string:lowercase(Key), Existing)
    ],
    Sorted = lists:sort(fun extra_key_order/2, Candidates),
    [ {extra, Key} || {Key, _} <- lists:sublist(Sorted, 3) ].

maybe_extra_columns(RowMaps, Columns, #{watch_table := true}) ->
    extra_columns(RowMaps, Columns);
maybe_extra_columns(RowMaps, Columns, _Opts) ->
    extra_columns(RowMaps, Columns).

-doc "Keep time-oriented watch rows stable across refreshes.".
-spec maybe_sort_watch_entries([entry()], columns(), opts()) -> [entry()].
maybe_sort_watch_entries(Entries, Columns, Opts) ->
    case lists:any(fun(C) -> C =:= expiry orelse C =:= window end, Columns) of
        false -> Entries;
        true ->
            Key = case lists:member(expiry, Columns) of
                true -> expiry;
                false -> window
            end,
            Indexed = lists:zip(lists:seq(0, length(Entries) - 1), Entries),
            Sorted = lists:sort(
              fun({AIdx, A}, {BIdx, B}) ->
                  KA = sort_value(Key, A, Opts),
                  KB = sort_value(Key, B, Opts),
                  case KA =:= KB of
                      true -> AIdx =< BIdx;
                      false -> KA =< KB
                  end
              end,
              Indexed),
            [Entry || {_Idx, Entry} <- Sorted]
    end.

sort_value(Key, Entry, Opts) ->
    RowMap = entry_row_map(Entry, Opts),
    Value = retinue_text:to_list(maps:get(Key, RowMap, "")),
    case string:trim(Value) of
        "" -> "zzzz";
        Val -> Val
    end.

extra_key_counts([], Acc) -> Acc;
extra_key_counts([Map | Rest], Acc) ->
    Extra = maps:get(extra_fields, Map, #{}),
    Acc1 = lists:foldl(
      fun(Key, A) -> maps:update_with(Key, fun(C) -> C + 1 end, 1, A) end,
      Acc,
      maps:keys(Extra)),
    extra_key_counts(Rest, Acc1).

extra_key_order({K1, C1}, {K2, C2}) ->
    case C1 =:= C2 of
        true -> K1 =< K2;
        false -> C1 > C2
    end.

row_statuses(Entries, _Diff, true) ->
    lists:duplicate(length(Entries), none);
row_statuses(Entries, Diff, false) ->
    StatusMap = wf_worldstate_watch:diff_status_map(Diff),
    lists:map(
      fun({Idx, Entry}) ->
          Key = wf_worldstate_watch:entry_key(Entry, Idx),
          maps:get(Key, StatusMap, none)
      end,
      lists:zip(lists:seq(0, length(Entries) - 1), Entries)).

merge_inline_entries(Entries, Columns, Diff, _PrevSnap, Opts) ->
    Removed = maps:get(removed, Diff, []),
    CurrTuples = [
        {entry_sort_key(Entry, Columns, Opts), 0, Idx, Entry}
        || {Idx, Entry} <- with_index(Entries)
    ],
    RemovedTuples = [
        {removed_sort_key(Prev, Columns), 1, Idx, removed_entry_table(Key, Prev, Columns)}
        || {Idx, {Key, Prev}} <- with_index(Removed)
    ],
    Combined = lists:sort(
      fun({K1, Kind1, I1, _}, {K2, Kind2, I2, _}) ->
          {K1, Kind1, I1} =< {K2, Kind2, I2}
      end,
      CurrTuples ++ RemovedTuples),
    [Entry || {_Key, _Kind, _Idx, Entry} <- Combined].

entry_sort_key(Entry, Columns, Opts) ->
    RowMap = entry_row_map(Entry, Opts),
    sort_key_from_row(RowMap, Columns).

removed_sort_key(Prev, Columns) ->
    RowMap = maps:get(row_map, Prev, #{}),
    sort_key_from_row(RowMap, Columns).

sort_key_from_row(RowMap, Columns) ->
    Key = case lists:member(expiry, Columns) of
        true -> expiry;
        false ->
            case lists:member(window, Columns) of
                true -> window;
                false -> undefined
            end
    end,
    case Key of
        undefined -> "zzzz";
        _ ->
            Value = retinue_text:to_list(maps:get(Key, RowMap, "")),
            case string:trim(Value) of
                "" -> "zzzz";
                Val -> Val
            end
    end.

with_index(List) ->
    lists:zip(lists:seq(0, length(List) - 1), List).

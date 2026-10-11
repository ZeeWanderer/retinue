%%%-------------------------------------------------------------------
%% Helpers for worldstate watch snapshots and diffs.
%%%-------------------------------------------------------------------
-module(wf_worldstate_watch).

-export([build_snapshot/5, diff/2, has_changes/1, format_diff_lines/2,
         format_diff_lines_min/2, diff_status_map/1, entry_key/2, color_line/2,
         order_map/1]).

build_snapshot(Entries, Format, Columns, Opts, Extracts) ->
    lists:foldl(
      fun({Idx, Entry}, Acc) ->
          Key = entry_key(Entry, Idx),
          Value = entry_signature(Entry, Format, Columns, Opts, Extracts),
          Name = maps:get(name, Entry, ""),
          Meta0 = #{name => Name, value => Value, order => Idx, columns => Columns},
          Meta = case {Format, Extracts} of
              {table, []} ->
                  RowMap = maps:get(row_map, Entry, #{}),
                  Extra = maps:get(extra_fields, Entry, #{}),
                  Meta0#{row_map => RowMap, extra_fields => Extra};
              _ -> Meta0
          end,
          Acc#{Key => Meta}
      end,
      #{},
      with_index(Entries)).

diff(undefined, Curr) ->
    wf_worldstate_diff:diff(undefined, Curr);
diff(Prev, Curr) ->
    wf_worldstate_diff:diff(Prev, Curr).

has_changes(#{added := Added, removed := Removed, changed := Changed}) ->
    wf_worldstate_diff:has_changes(#{added => Added, removed => Removed,
                                        changed => Changed}).

diff_status_map(Diff) ->
    Added = maps:get(added, Diff, []),
    Removed = maps:get(removed, Diff, []),
    Changed = maps:get(changed, Diff, []),
    AddedMap = maps:from_list([{K, add} || {K, _} <- Added]),
    RemovedMap = maps:from_list([{K, remove} || {K, _} <- Removed]),
    ChangedMap = maps:from_list([{K, change} || {K, _Prev, _Curr} <- Changed]),
    maps:merge(AddedMap, maps:merge(RemovedMap, ChangedMap)).

format_diff_lines(Diff, UseColor) ->
    Added = maps:get(added, Diff, []),
    Removed = maps:get(removed, Diff, []),
    Changed = maps:get(changed, Diff, []),
    Lines0 = [format_added(A, UseColor) || A <- Added] ++
             [format_removed(R, UseColor) || R <- Removed] ++
             [format_changed(C, UseColor) || C <- Changed],
    [L || L <- Lines0, L =/= "" ].
format_diff_lines_min(Diff, UseColor) ->
    Added = maps:get(added, Diff, []),
    Removed = maps:get(removed, Diff, []),
    Changed = maps:get(changed, Diff, []),
    Lines0 = [format_added_min(A, UseColor) || A <- Added] ++
             [format_removed_min(R, UseColor) || R <- Removed] ++
             [format_changed_min(C, UseColor) || C <- Changed],
    [L || L <- Lines0, L =/= "" ].

format_added({Key, #{name := Name, value := Value}}, UseColor) ->
    Line = "added " ++ entry_label(Key, Name) ++ ": " ++ retinue_text:to_list(Value),
    color_line(Line, add, UseColor);
format_added(_, _UseColor) -> "".

format_removed({Key, #{name := Name, value := Value}}, UseColor) ->
    Line = "removed " ++ entry_label(Key, Name) ++ ": " ++ retinue_text:to_list(Value),
    color_line(Line, remove, UseColor);
format_removed(_, _UseColor) -> "".

format_changed({Key, Prev, Curr}, UseColor) ->
    PrevVal = retinue_text:to_list(maps:get(value, Prev, "")),
    CurrVal = retinue_text:to_list(maps:get(value, Curr, "")),
    Name = maps:get(name, Curr, maps:get(name, Prev, "")),
    Line = "changed " ++ entry_label(Key, Name) ++ ": " ++ PrevVal ++ " -> " ++ CurrVal,
    color_line(Line, change, UseColor);
format_changed(_, _UseColor) -> "".

format_added_min({Key, #{name := Name, value := Value}}, UseColor) ->
    Line = entry_label(Key, Name) ++ ": " ++ retinue_text:to_list(Value),
    color_line(Line, add, UseColor);
format_added_min(_, _UseColor) -> "".

format_removed_min({Key, #{name := Name, value := Value}}, UseColor) ->
    Line = entry_label(Key, Name) ++ ": " ++ retinue_text:to_list(Value),
    color_line(Line, remove, UseColor);
format_removed_min(_, _UseColor) -> "".

format_changed_min({Key, Prev, Curr}, UseColor) ->
    PrevVal = retinue_text:to_list(maps:get(value, Prev, "")),
    CurrVal = retinue_text:to_list(maps:get(value, Curr, "")),
    Name = maps:get(name, Curr, maps:get(name, Prev, "")),
    Line = entry_label(Key, Name) ++ ": " ++ PrevVal ++ " -> " ++ CurrVal,
    color_line(Line, change, UseColor);
format_changed_min(_, _UseColor) -> "".

entry_label(Key0, Name0) ->
    Key = retinue_text:to_list(Key0),
    Name = retinue_text:to_list(Name0),
    case {Name, Key} of
        {"", _} -> Key;
        {Key, _} -> Key;
        _ -> Name ++ " (" ++ Key ++ ")"
    end.

color_line(Line, _Type, false) -> Line;
color_line(Line, add, true) -> retinue_tty:colorize(Line, green);
color_line(Line, remove, true) -> retinue_tty:colorize(Line, red);
color_line(Line, change, true) -> retinue_tty:colorize(Line, yellow);
color_line(Line, _, true) -> Line.

color_line(Line, Type) ->
    color_line(Line, Type, true).

entry_key(Entry, Idx) ->
    wf_worldstate_diff:entry_key(Entry, Idx).

entry_signature(Entry, _Format, _Columns, _Opts, Extracts) when Extracts =/= [] ->
    Data = maps:get(data, Entry, #{}),
    lists:flatten(string:join([retinue_data_extract:extract_string(Data, Path)
                               || Path <- Extracts], " | "));
entry_signature(Entry, block, _Columns, Opts, _Extracts) ->
    wf_worldstate_format:format(Entry, Opts);
entry_signature(Entry, table, Columns, _Opts, _Extracts) ->
    RowMap0 = maps:get(row_map, Entry, #{}),
    Extra = maps:get(extra_fields, Entry, #{}),
    RowMap = RowMap0#{extra_fields => Extra},
    Row = [column_value(RowMap, Col) || Col <- Columns],
    string:join(Row, " | ");
entry_signature(Entry, _, Columns, Opts, Extracts) ->
    entry_signature(Entry, table, Columns, Opts, Extracts).

normalize_cell(Value) ->
    lists:flatten(retinue_text:to_list(Value)).

column_value(Map, {extra, Key}) ->
    Extra = maps:get(extra_fields, Map, #{}),
    normalize_cell(maps:get(Key, Extra, ""));
column_value(Map, Col) ->
    normalize_cell(maps:get(Col, Map, "")).

with_index(List) ->
    lists:zip(lists:seq(0, length(List) - 1), List).

order_map(Snapshot) when is_map(Snapshot) ->
    maps:fold(
      fun(Key, Meta, Acc) ->
          Name = retinue_text:to_list(maps:get(name, Meta, "")),
          case maps:get(order, Meta, undefined) of
              undefined -> Acc;
              Order ->
                  Acc1 = Acc#{Key => Order},
                  case Name of
                      "" -> Acc1;
                      _ -> Acc1#{{name, Name} => Order}
                  end
          end
      end,
      #{},
      Snapshot);
order_map(_Snapshot) ->
    #{}.

%%%-------------------------------------------------------------------
%% Rendering for official export catalog query results.
%%%-------------------------------------------------------------------
-module(wf_exports_format).

-export([print/3]).

-import(retinue_text, [to_list/1]).

-doc "Render one prepared mods/items query result.".
-spec print(string(), map(), map()) -> ok.
print("mods", Query, Results) -> print_mods(Results, Query);
print("items", Query, Results) -> print_items(Results, Query).

print_mods(#{slice := Slice, total := Total, shown := Shown}, Query) ->
    case maps:get(output_format, Query, table) of
        json -> print_json(mod, Total, Slice);
        _ ->
            io:format("Matches: ~p (showing ~p)~n~n", [Total, Shown]),
            case Slice of
                [] -> io:format("no entries~n", []);
                _ ->
                    case maps:get(output_format, Query, table) of
                        block -> lists:foreach(
                                   fun(E) -> print_mod_block(E, maps:get(raw, Query, false)) end,
                                   Slice);
                        table -> print_mod_table(Slice, maps:get(raw, Query, false))
                    end
            end
    end.

print_items(#{slice := Slice, total := Total, shown := Shown}, Query) ->
    case maps:get(output_format, Query, table) of
        json -> print_json(item, Total, Slice);
        _ ->
            io:format("Matches: ~p (showing ~p)~n~n", [Total, Shown]),
            case Slice of
                [] -> io:format("no entries~n", []);
                _ ->
                    case maps:get(output_format, Query, table) of
                        block -> lists:foreach(fun print_item_block/1, Slice);
                        table -> print_item_table(Slice)
                    end
            end
    end.

print_mod_block(Entry, Raw) ->
    io:format("~ts~n~n", [render_block(Entry, mod, Raw)]).

print_item_block(Entry) ->
    io:format("~ts~n~n", [render_block(Entry, item, false)]).

render_block(Entry, Kind, Raw) ->
    RowMap0 = maps:get(row_map, Entry, entry_data(Entry)),
    RowMap = block_row_map(RowMap0, Kind, Raw),
    Spec = wf_exports_presentation:block_spec(Kind),
    Title = render_block_title(maps:get(title, Spec, "Entry"), RowMap),
    Fields = block_fields_from_spec(Spec, RowMap, Kind, Raw),
    Extras = maps:get(extra_fields, Entry, #{}),
    format_block(Title, Fields, Extras).

block_row_map(RowMap, mod, Raw) ->
    RowMap#{
        effects => maybe_colorize(maps:get(effects, RowMap, ""), Raw),
        description => maybe_colorize(maps:get(description, RowMap, ""), Raw),
        max_stats => maybe_colorize(maps:get(max_stats, RowMap, ""), Raw),
        polarity => wf_exports_schema:polarity_display(maps:get(polarity, RowMap, ""))
    };
block_row_map(RowMap, _Kind, _Raw) -> RowMap.

maybe_colorize(Value, true) -> Value;
maybe_colorize(Value, false) -> retinue_tty:colorize_dt_tags(Value).

render_block_title(Template, RowMap) ->
    render_block_title(to_list(Template), RowMap, []).

render_block_title([], _RowMap, Acc) -> lists:reverse(Acc);
render_block_title([${ | Rest], RowMap, Acc) ->
    {Key, Tail} = take_template_key(Rest, []),
    Value = to_list(template_value(Key, RowMap)),
    render_block_title(Tail, RowMap, lists:reverse(Value) ++ Acc);
render_block_title([C | Rest], RowMap, Acc) ->
    render_block_title(Rest, RowMap, [C | Acc]).

take_template_key([], Acc) -> {lists:reverse(Acc), []};
take_template_key([$} | Rest], Acc) -> {lists:reverse(Acc), Rest};
take_template_key([C | Rest], Acc) -> take_template_key(Rest, [C | Acc]).

template_value(Key, RowMap) ->
    case [Value || {MapKey, Value} <- maps:to_list(RowMap),
                   is_atom(MapKey), atom_to_list(MapKey) =:= Key] of
        [Value | _] -> Value;
        [] -> ""
    end.

block_fields_from_spec(Spec, RowMap, Kind, Raw) ->
    Fields = maps:get(fields, Spec, []),
    Used = [Key || {_Label, Key} <- Fields],
    Fields1 = [{Label, block_value_for_key(Key, RowMap, Kind, Raw)}
               || {Label, Key} <- Fields],
    Fields1 ++ block_extra_fields(RowMap, Used, Kind, Raw).

block_extra_fields(RowMap, Used, Kind, Raw) ->
    Keys = [K || K <- maps:keys(RowMap), not lists:member(K, Used)],
    Sorted = lists:sort(fun(A, B) -> atom_to_list(A) =< atom_to_list(B) end, Keys),
    [{block_label_for_key(Key), block_value_for_key(Key, RowMap, Kind, Raw)} || Key <- Sorted].

block_label_for_key(Key) ->
    maps:get(label, wf_exports_schema:column_spec(Key), atom_to_list(Key)).

block_value_for_key(Key, RowMap, mod, Raw) ->
    Value = maps:get(Key, RowMap, ""),
    case Key of
        effects -> maybe_colorize(Value, Raw);
        description -> maybe_colorize(Value, Raw);
        max_stats -> maybe_colorize(Value, Raw);
        _ -> Value
    end;
block_value_for_key(Key, RowMap, _Kind, _Raw) -> maps:get(Key, RowMap, "").

format_block(Title, Fields, Extras) ->
    lists:flatten(lists:join("\n", [Title | format_fields(Fields ++ maps:to_list(Extras))])).

format_fields(Fields) ->
    lists:reverse(lists:foldl(
      fun({Label, Value}, Acc) ->
          case retinue_text:value_present(Value) of
              true -> [lists:flatten(io_lib:format("  ~s: ~s", [Label, to_list(Value)])) | Acc];
              false -> Acc
          end
      end, [], Fields)).

print_mod_table(Entries, Raw) ->
    Columns = mod_columns(Raw),
    RowMaps = [entry_row_map(E, mod) || E <- Entries],
    Keep = Columns ++ extra_columns(RowMaps, Columns),
    render_table([column_label(C) || C <- Keep],
                 [row_from_map(Map, Keep) || Map <- RowMaps],
                 #{column_specs => [wf_exports_schema:column_spec(C) || C <- Keep],
                   row_maps => RowMaps, intent => #{}}).

mod_columns(false) -> wf_exports_schema:columns_for_kind(mod);
mod_columns(true) -> wf_exports_schema:columns_for_kind(mod) ++ [uniqueName].

print_item_table(Entries) ->
    Columns = wf_exports_schema:columns_for_kind(item),
    RowMaps = [entry_row_map(E, item) || E <- Entries],
    {Keep, Rows} = prune_columns(Columns ++ extra_columns(RowMaps, Columns), RowMaps),
    render_table([column_label(C) || C <- Keep], Rows,
                 #{column_specs => [wf_exports_schema:column_spec(C) || C <- Keep],
                   row_maps => RowMaps, intent => #{}}).

entry_row_map(Entry, Kind) ->
    Row0 = maps:get(row_map, Entry, entry_data(Entry)),
    Row1 = case Kind of mod -> mod_display_row_map(Row0); _ -> Row0 end,
    case maps:get(extra_fields, Entry, #{}) of
        Extra when map_size(Extra) =:= 0 -> Row1;
        Extra -> Row1#{extra_fields => Extra}
    end.

mod_display_row_map(RowMap) ->
    RowMap#{effects => retinue_tty:colorize_dt_tags(maps:get(effects, RowMap, "")),
            polarity => wf_exports_schema:polarity_symbol(maps:get(polarity, RowMap, ""))}.

extra_columns(RowMaps, Columns) ->
    Existing = [string:lowercase(column_label(C)) || C <- Columns],
    Counts = extra_key_counts(RowMaps, #{}),
    Candidates = [{Key, Count} || {Key, Count} <- maps:to_list(Counts),
                                  not lists:member(string:lowercase(Key), Existing)],
    Sorted = lists:sort(fun extra_key_order/2, Candidates),
    [{extra, Key} || {Key, _} <- lists:sublist(Sorted, 3)].

extra_key_counts([], Acc) -> Acc;
extra_key_counts([Map | Rest], Acc) ->
    Extra = maps:get(extra_fields, Map, #{}),
    Acc1 = lists:foldl(
      fun(Key, A) -> maps:update_with(Key, fun(C) -> C + 1 end, 1, A) end,
      Acc, maps:keys(Extra)),
    extra_key_counts(Rest, Acc1).

extra_key_order({K1, C1}, {K2, C2}) when C1 =:= C2 -> K1 =< K2;
extra_key_order({_K1, C1}, {_K2, C2}) -> C1 > C2.

render_table(Headers, Rows, Opts) ->
    Lines = retinue_table:render_lines(Headers, Rows, Opts),
    lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end, Lines),
    io:format("~n", []).

prune_columns(Columns, RowMaps) ->
    Keep = [Col || Col <- Columns, column_has_value(Col, RowMaps)],
    {Keep, [row_from_map(Map, Keep) || Map <- RowMaps]}.

column_has_value({extra, Key}, RowMaps) ->
    lists:any(fun(Map) -> cell_present(maps:get(Key, maps:get(extra_fields, Map, #{}), "")) end,
              RowMaps);
column_has_value(Col, RowMaps) ->
    lists:any(fun(Map) -> cell_present(maps:get(Col, Map, "")) end, RowMaps).

row_from_map(Map, Columns) -> [column_value(Map, Col) || Col <- Columns].

column_value(Map, {extra, Key}) ->
    normalize_cell(maps:get(Key, maps:get(extra_fields, Map, #{}), ""));
column_value(Map, Col) -> normalize_cell(maps:get(Col, Map, "")).

normalize_cell(undefined) -> "";
normalize_cell(null) -> "";
normalize_cell(Value) -> to_list(Value).

cell_present(undefined) -> false;
cell_present(null) -> false;
cell_present(Value) ->
    Str = string:trim(to_list(Value)),
    Str =/= "" andalso Str =/= "null".

column_label({extra, Key}) -> Key;
column_label(Col) ->
    maps:get(label, wf_exports_schema:column_spec(Col), atom_to_list(Col)).

entry_data(Entry) -> maps:get(data, Entry, Entry).
print_json(Kind, Total, Slice) ->
    Data = [wf_catalog_json:record(Kind, entry_data(E)) || E <- Slice],
    Payload = #{count => Total, shown => length(Slice), results => Data},
    io:format("~ts~n", [jsone:encode(Payload)]).

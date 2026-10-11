%%%-------------------------------------------------------------------
%% Terminal block rendering for daemon-projected worldstate entities.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_format).

-export([format/1, format/2]).

-doc "Render one normalized worldstate entity as a terminal block.".
-spec format(map()) -> iodata().
format(Entry) -> format(Entry, #{}).

-doc "Render one normalized worldstate entity using its daemon-projected row map.".
-spec format(map(), map()) -> iodata().
format(#{type := Type} = Entry, _Opts) ->
    RowMap = maps:get(row_map, Entry, #{}),
    Spec = wfcli_worldstate_presentation:block_spec(Type),
    Title = render_title(maps:get(title, Spec, "Entry"), RowMap),
    Fields = block_fields(Spec, RowMap),
    append_extras(Entry, format_block(Title, Fields));
format(Entry, _Opts) ->
    lists:flatten(io_lib:format("~p", [Entry])).

render_title(Template, RowMap) ->
    render_title(retinue_text:to_list(Template), RowMap, []).

render_title([], _RowMap, Acc) -> lists:reverse(Acc);
render_title([${ | Rest], RowMap, Acc) ->
    {Key, Tail} = take_key(Rest, []),
    Value = retinue_text:to_list(template_value(Key, RowMap)),
    render_title(Tail, RowMap, lists:reverse(Value) ++ Acc);
render_title([Char | Rest], RowMap, Acc) ->
    render_title(Rest, RowMap, [Char | Acc]).

take_key([], Acc) -> {lists:reverse(Acc), []};
take_key([$} | Rest], Acc) -> {lists:reverse(Acc), Rest};
take_key([Char | Rest], Acc) -> take_key(Rest, [Char | Acc]).

template_value(Key, RowMap) ->
    case [Value || {MapKey, Value} <- maps:to_list(RowMap),
                   is_atom(MapKey), atom_to_list(MapKey) =:= Key] of
        [Value | _] -> Value;
        [] -> ""
    end.

block_fields(Spec, RowMap) ->
    Fields = maps:get(fields, Spec, []),
    Used = [Key || {_Label, Key} <- Fields],
    Skip = maps:get(skip_fields, Spec, [window_start, window_end]),
    Main = [{Label, maps:get(Key, RowMap, "")} || {Label, Key} <- Fields],
    ExtraKeys = [Key || Key <- maps:keys(RowMap),
                        not lists:member(Key, Used), not lists:member(Key, Skip)],
    Main ++ [{atom_to_list(Key), maps:get(Key, RowMap, "")}
             || Key <- lists:sort(ExtraKeys)].

format_block(Title, Fields) ->
    lists:flatten(lists:join("\n", [Title | format_fields(Fields)])).

append_extras(Entry, Text) ->
    ExtraLines = format_fields(lists:sort(maps:to_list(maps:get(extra_fields, Entry, #{})))),
    case ExtraLines of
        [] -> Text;
        _ -> Text ++ "\n" ++ lists:flatten(lists:join("\n", ExtraLines))
    end.

format_fields(Fields) ->
    [lists:flatten(io_lib:format("  ~s: ~s", [retinue_text:to_list(Label),
                                                retinue_text:to_list(Value)]))
     || {Label, Value} <- Fields, retinue_text:value_present(Value)].

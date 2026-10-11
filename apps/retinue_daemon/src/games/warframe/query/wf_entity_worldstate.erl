%%%-------------------------------------------------------------------
%% Entity builder for worldstate entries (sparse fields).
%%%-------------------------------------------------------------------
-module(wf_entity_worldstate).

-export([build/5, table_spec/1, columns_spec/0, column_spec/1,
         columns_for_type/1, default_table_columns/0, columns_for_inventory/1,
         type_from_label/1, normalize_query_key/1, query_field/2,
         default_sort/1]).

-type entry() :: map().
-type opts() :: map().
-type type_atom() :: atom().
-type column() :: atom() | {extra, string()}.
-type column_spec() :: map().
-type sort_spec() :: map().

-doc "Build one normalized worldstate entity; all per-type data stays in the raw `data` map.".
-spec build(type_atom(), term(), term(), term(), opts()) -> entry().
build(Type, Id, Name, Data, Opts) ->
    Spec = #{
        row_map_fun => fun(Entry, Opts1) -> wf_worldstate_projector:table_row_map(Entry, Opts1) end,
        to_list_fun => fun wf_worldstate_projector:to_list/1,
        extra_field_fun => fun wf_worldstate_projector:resolve_extra_field/3,
        resolve_strings_fun => fun resolve_worldstate_string/2
    },
    wf_entity:build(Type, Id, Name, Data, Opts, Spec).

resolve_worldstate_string(Str, Opts) ->
    case maps:get(resolve_items, Opts, false) of
        true ->
            wf_resolve:resolve("item", list_to_binary(Str), Opts);
        false -> ""
    end.

-doc "Return table columns and shared column specs for one worldstate type.".
-spec table_spec(type_atom()) -> #{columns := [column()], specs := [column_spec()]}.
table_spec(Type) ->
    #{columns => columns_for_type(Type), specs => columns_spec()}.

-doc "Map query field aliases to entity keys; `data.foo.bar` stays a raw extraction path.".
-spec normalize_query_key(string()) -> name | id | type | extract | sort |
                                      {data, string()} | {column, atom()} | undefined.
normalize_query_key(Key0) ->
    KeyText = retinue_text:to_list(Key0),
    Key = string:lowercase(KeyText),
    case Key of
        "name" -> name;
        "id" -> id;
        "type" -> type;
        "extract" -> extract;
        "sort" -> sort;
        _ ->
            case lists:prefix("data.", Key) of
                true -> {data, string:slice(KeyText, 5)};
                false ->
                    case wf_worldstate_schema:query_column_spec(KeyText) of
                        {ok, Spec} -> {column, maps:get(key, Spec)};
                        error -> undefined
                    end
            end
    end.

-doc "Resolve a worldstate query key, including dynamic raw data paths, to an entity field.".
-spec query_field(term(), string() | atom()) -> {ok, map()} | error.
query_field(_Kind, Key0) ->
    case normalize_query_key(Key0) of
        name -> {ok, #{key => name, source => {entry, name}, kind => string, default_op => contains}};
        id -> {ok, #{key => id, source => {entry, id}, kind => string, default_op => contains}};
        type -> {ok, #{key => type, source => {entry, type}, kind => string, default_op => eq}};
        {data, Path} -> {ok, #{key => {data, Path}, source => {data_path, Path},
                              kind => dynamic, default_op => contains}};
        {column, Key} ->
            {ok, Spec} = wf_worldstate_schema:query_column_spec(Key),
            {ok, #{key => Key, source => {row, Key},
                   kind => maps:get(kind, Spec, string),
                   default_op => maps:get(default_op, Spec, contains)}};
        _ -> error
    end.

-doc "Return default sort specs for a type; empty keeps source/index order.".
-spec default_sort(type_atom()) -> [sort_spec()].
default_sort(_Type) ->
    [].

-spec column_spec(column()) -> column_spec().
column_spec(Column) -> wf_worldstate_schema:column_spec(Column).

default_table_columns() -> wf_worldstate_schema:default_table_columns().

columns_for_inventory(Type) -> wf_worldstate_schema:columns_for_inventory(Type).

columns_for_type(Type) -> wf_worldstate_schema:columns_for_type(Type).

type_from_label(Label) -> wf_worldstate_schema:type_from_label(Label).

columns_spec() -> wf_worldstate_schema:columns_spec().

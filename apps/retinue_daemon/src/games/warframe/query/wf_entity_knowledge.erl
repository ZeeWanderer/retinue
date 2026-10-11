%%%-------------------------------------------------------------------
%% Entity schemas for official Codex and WFCD knowledge.
%%%-------------------------------------------------------------------
-module(wf_entity_knowledge).

-export([build_codex/2, build_enemy/2, build_drop/2,
         table_spec/1, columns_spec/0, column_spec/1, columns_for_kind/1,
         normalize_query_key/1, query_field/2, query_sort_field/2,
         default_sort/1]).

-doc "Build one searchable official Codex entity.".
-spec build_codex(map(), map()) -> map().
build_codex(Item, Opts) ->
    build(codex, Item, maps:get(name, Item, ""), Opts, fun codex_row/2).

-doc "Build one searchable WFCD enemy entity.".
-spec build_enemy(map(), map()) -> map().
build_enemy(Enemy, Opts) ->
    build(enemy, Enemy, maps:get(name, Enemy, ""), Opts, fun enemy_row/2).

-doc "Build one reverse-searchable WFCD drop entity.".
-spec build_drop(map(), map()) -> map().
build_drop(Drop, Opts) ->
    Name = maps:get(item, Drop, maps:get(name, Drop, "")),
    Id = string:join([maps:get(enemyUniqueName, Drop, ""), Name,
                      integer_to_list(maps:get(index, Drop, 0))], "|"),
    Spec = #{row_map_fun => fun drop_row/2},
    wf_entity:build(drop, Id, Name, Drop, Opts, Spec).

build(Kind, Data, Name, Opts, RowFun) ->
    Id = case maps:get(uniqueName, Data, "") of
        "" -> Name;
        Unique -> Unique
    end,
    wf_entity:build(Kind, Id, Name, Data, Opts, #{row_map_fun => RowFun}).

codex_row(Entry, _Opts) ->
    D = maps:get(data, Entry),
    #{name => maps:get(name, D, ""),
      category => maps:get(category, D, ""),
      description => maps:get(description, D, ""),
      secret => maps:get(codexSecret, D, false),
      excluded => maps:get(excludeFromCodex, D, false),
      file => string:join(maps:get(sourceFiles, D, []), ", "),
      uniqueName => maps:get(uniqueName, D, "")}.

enemy_row(Entry, _Opts) ->
    D = maps:get(data, Entry),
    #{name => maps:get(name, D, ""), faction => maps:get(faction, D, ""),
      health => maps:get(health, D, ""), shield => maps:get(shield, D, ""),
      armor => maps:get(armor, D, ""), resistances => maps:get(resistances, D, ""),
      dropCount => maps:get(dropCount, D, 0), description => maps:get(description, D, ""),
      uniqueName => maps:get(uniqueName, D, "")}.

drop_row(Entry, _Opts) ->
    D = maps:get(data, Entry),
    #{item => maps:get(item, D, ""), enemy => maps:get(enemy, D, ""),
      chance => maps:get(chance, D, ""), rarity => maps:get(rarity, D, ""),
      table => maps:get(table, D, ""), sourceVersion => maps:get(sourceVersion, D, "")}.

normalize_query_key(Key0) ->
    case string:lowercase(retinue_text:to_list(Key0)) of
        "name" -> name;
        "text" -> text;
        "sort" -> sort;
        "category" -> category;
        "file" -> file;
        "secret" -> secret;
        "excluded" -> excluded;
        "faction" -> faction;
        "health" -> health;
        "shield" -> shield;
        "armor" -> armor;
        "resistances" -> resistances;
        "dropcount" -> dropCount;
        "item" -> item;
        "enemy" -> enemy;
        "chance" -> chance;
        "rarity" -> rarity;
        "table" -> table;
        _ -> undefined
    end.

-doc "Resolve one knowledge query key to a field available on this entity kind.".
-spec query_field(codex | enemy | drop, string() | atom()) -> {ok, map()} | error.
query_field(Kind, Key0) ->
    Key = normalize_query_key(Key0),
    case lists:member(Key, query_fields(Kind)) of
        true -> {ok, query_field_spec(Key)};
        false -> error
    end.

-doc "Resolve query aliases or displayed labels to a sortable knowledge field.".
-spec query_sort_field(codex | enemy | drop, string() | atom()) -> {ok, map()} | error.
query_sort_field(Kind, Key0) ->
    case query_field(Kind, Key0) of
        {ok, Spec} -> {ok, Spec};
        error -> sort_column_field(Kind, Key0)
    end.

query_fields(codex) -> [name, text, category, file, secret, excluded];
query_fields(enemy) -> [name, text, faction, health, shield, armor, resistances, dropCount];
query_fields(drop) -> [text, item, enemy, chance, rarity, table].

query_field_spec(text) -> #{key => text, source => haystack, kind => string, default_op => contains};
query_field_spec(Key) ->
    #{key => Key, source => {row, Key}, kind => field_kind(Key), default_op => default_op(Key)}.

field_kind(Key) ->
    case lists:member(Key, [health, shield, armor, dropCount, chance]) of
        true -> number;
        false -> string
    end.

default_op(Key) ->
    case lists:member(Key, [name, text, file, enemy, item, resistances]) of
        true -> contains;
        false -> eq
    end.

sort_column_field(Kind, Key0) ->
    KeyText = string:lowercase(retinue_text:to_list(Key0)),
    case lists:dropwhile(
           fun(Spec) ->
               SpecKey = maps:get(key, Spec, undefined),
               Label = string:lowercase(retinue_text:to_list(maps:get(label, Spec, ""))),
               (SpecKey =:= undefined orelse string:lowercase(atom_to_list(SpecKey)) =/= KeyText)
                   andalso Label =/= KeyText
           end, columns_spec()) of
        [#{key := Key} = Column | _] ->
            case lists:member(Key, columns_for_kind(Kind)) of
                true -> {ok, #{key => Key, source => {row, Key},
                               kind => maps:get(kind, Column, field_kind(Key)), default_op => eq}};
                false -> error
            end;
        [] -> error
    end.

default_sort(drop) -> [#{key => item, dir => asc}, #{key => enemy, dir => asc}];
default_sort(_) -> [#{key => name, dir => asc}].

columns_for_kind(Kind) -> wf_knowledge_schema:columns_for_kind(Kind).

table_spec(Kind) -> wf_knowledge_schema:table_spec(Kind).

columns_spec() -> wf_knowledge_schema:columns_spec().

column_spec(Key) -> wf_knowledge_schema:column_spec(Key).

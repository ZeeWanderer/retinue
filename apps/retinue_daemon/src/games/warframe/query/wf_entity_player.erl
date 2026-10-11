%%%-------------------------------------------------------------------
%% Searchable entities projected from canonical player data.
%%%-------------------------------------------------------------------
-module(wf_entity_player).

-export([build_entries/2, query_field/2, query_sort_field/2, default_sort/1]).

-doc "Build raw source entries and typed projections from the inventory observation.".
-spec build_entries(map(), map()) -> [map()].
build_entries(Snapshot, Opts) ->
    Data = maps:get(data, Snapshot, #{}),
    View = maps:get(view, Opts, auto),
    Containers = case View of
        typed -> [];
        _ -> container_entries(Data, Opts)
    end,
    Projection = wf_player_projection:build(Snapshot),
    Containers ++ projection_entries(Projection, Opts).

container_entries(Data, Opts) ->
    Root = build_container(player, <<"player">>, <<"Player">>, <<"all">>,
                           Data, false, Opts),
    Sources = [
        build_container(player_source, Source, Source, Source, SourceData,
                        search_source(Source, maps:get(view, Opts, auto)), Opts)
        || {Source, SourceData} <- lists:sort(maps:to_list(Data))
    ],
    [Root | Sources].

search_source(<<"inventory">>, _) -> false;
search_source(Source, auto) when Source =:= <<"inventory_http">>; Source =:= <<"inventory_native">> -> false;
search_source(_, _) -> true.

build_container(Type, Id, Name, Source, Data, SearchRaw, Opts) ->
    Spec = #{
        row_map_fun => fun(Entry) ->
            #{type => maps:get(type, Entry), id => maps:get(id, Entry),
              name => maps:get(name, Entry), source => Source}
        end
    },
    (wf_entity:build(Type, Id, Name, Data,
                        Opts#{search_raw => SearchRaw}, Spec))#{source => Source,
                                                               representation => <<"raw">>}.

projection_entries(Projection, Opts) ->
    Typed = maps:get(entities, Projection, []),
    Raw = maps:get(raw_entities, Projection, []),
    Records = select_records(maps:get(view, Opts, auto), Typed, Raw),
    [record_entry(Record, Opts) || Record <- Records].

select_records(auto, Typed, Raw) ->
    Covered = maps:from_keys(
      lists:append([maps:get(origins, Record, []) || Record <- Typed]), true),
    Typed ++ [Record || Record <- Raw,
                        not maps:is_key(maps:get(origin, Record), Covered)];
select_records(raw, _Typed, Raw) -> Raw;
select_records(typed, Typed, _Raw) -> Typed;
select_records(both, Typed, Raw) -> Typed ++ Raw.

record_entry(Record, Opts) ->
    Type = maps:get(entity_type, Record),
    Id = maps:get(<<"id">>, Record, origin_text(maps:get(origin, Record))),
    Name = record_name(Type, Record, Id, Opts),
    Data = maps:get(raw, Record, #{}),
    Projection = maps:without([entity_type, origin, origins, raw], Record),
    Representation = case Type of
        player_raw -> <<"raw">>;
        _ -> <<"typed">>
    end,
    Origin = origin_text(maps:get(origin, Record)),
    Origins = [origin_text(Value) || Value <- maps:get(origins, Record, [])],
    Fields = Projection#{<<"representation">> => Representation,
                         <<"origin">> => Origin,
                         <<"origins">> => Origins},
    typed_build(Type, Id, Name, Data, Fields, Origin, Origins,
                Representation, Opts).

typed_build(Type, Id, Name, Data, Fields, Origin, Origins, Representation, Opts) ->
    Source = <<"inventory">>,
    Scalars = scalar_fields(Fields),
    Spec = #{
        row_map_fun => fun(Entry) ->
            maps:merge(
              #{type => maps:get(type, Entry), id => maps:get(id, Entry),
                name => maps:get(name, Entry), source => Source},
              Scalars)
        end
    },
    Entry = wf_entity:build(Type, Id, Name, Data,
                               Opts#{search_raw => true}, Spec),
    Haystack = append_projection_haystack(maps:get(haystack, Entry, ""), Fields),
    maps:merge(Entry#{source => Source, origin => Origin, origins => Origins,
                      representation => Representation,
                      projection => Fields, haystack => Haystack}, Scalars).

append_projection_haystack(Haystack, Fields) ->
    Values = [string:lowercase(Value)
              || Value <- wf_entity:collect_strings(Fields)],
    string:join([Haystack | Values], "|").

record_name(player_profile, _Record, _Id, _Opts) -> <<"Profile">>;
record_name(player_mission, Record, Id, Opts) ->
    resolved_name(node, maps:get(<<"tag">>, Record, Id), Opts);
record_name(player_skill, Record, Id, _Opts) -> maps:get(<<"skill">>, Record, Id);
record_name(player_affiliation, Record, Id, _Opts) -> maps:get(<<"tag">>, Record, Id);
record_name(player_challenge, Record, Id, _Opts) -> maps:get(<<"name">>, Record, Id);
record_name(player_loadout, Record, Id, _Opts) -> maps:get(<<"name">>, Record, Id);
record_name(player_config, Record, Id, Opts) ->
    case maps:get(<<"name">>, Record, undefined) of
        Name when is_binary(Name), byte_size(Name) > 0 -> Name;
        _ -> resolved_name(item, maps:get(<<"item_type">>, Record, Id), Opts)
    end;
record_name(player_raw, Record, Id, _Opts) -> maps:get(<<"name">>, Record, Id);
record_name(_Type, Record, Id, Opts) -> entry_name(Record, Id, Opts).

origin_text({Source, Path}) ->
    Segments = [Source | [path_segment(Value) || Value <- Path]],
    iolist_to_binary(lists:join($., Segments)).

path_segment(Value) when is_binary(Value) -> Value;
path_segment(Value) when is_integer(Value) -> integer_to_binary(Value);
path_segment(Value) -> retinue_text:to_binary(Value).

scalar_fields(Data) ->
    maps:fold(
      fun(Key, Value, Acc) when is_binary(Key),
                                (is_binary(Value) orelse is_integer(Value)
                                 orelse is_float(Value) orelse is_boolean(Value)) ->
              case field_atom(Key) of
                  undefined -> Acc;
                  Field -> Acc#{Field => Value}
              end;
         (_Key, _Value, Acc) -> Acc
      end,
      #{},
      Data).

field_atom(<<"item_type">>) -> item_type;
field_atom(<<"instance_id">>) -> instance_id;
field_atom(<<"collection">>) -> collection;
field_atom(<<"count">>) -> count;
field_atom(<<"xp">>) -> xp;
field_atom(<<"item_name">>) -> item_name;
field_atom(<<"rank">>) -> rank;
field_atom(<<"skill">>) -> skill;
field_atom(<<"representation">>) -> representation;
field_atom(<<"origin">>) -> origin;
field_atom(<<"path">>) -> path;
field_atom(<<"kind">>) -> item_kind;
field_atom(<<"config_index">>) -> config_index;
field_atom(<<"customization_index">>) -> customization_index;
field_atom(<<"upgrade_count">>) -> upgrade_count;
field_atom(<<"upgrade_version">>) -> upgrade_version;
field_atom(<<"forma_count">>) -> forma_count;
field_atom(<<"feature_flags">>) -> feature_flags;
field_atom(<<"equipment_id">>) -> equipment_id;
field_atom(<<"loadout_id">>) -> loadout_id;
field_atom(<<"group">>) -> group;
field_atom(<<"active">>) -> active;
field_atom(<<"favorite">>) -> favorite;
field_atom(<<"equipped">>) -> equipped;
field_atom(<<"focus_school">>) -> focus_school;
field_atom(<<"slot">>) -> slot;
field_atom(<<"hidden">>) -> hidden;
field_atom(<<"standing">>) -> standing;
field_atom(<<"title">>) -> title;
field_atom(<<"free_favors_earned">>) -> free_favors_earned;
field_atom(<<"free_favors_used">>) -> free_favors_used;
field_atom(<<"progress">>) -> progress;
field_atom(<<"completed">>) -> completed;
field_atom(<<"level">>) -> level;
field_atom(<<"is_universal">>) -> is_universal;
field_atom(<<"school">>) -> school;
field_atom(<<"tag">>) -> tag;
field_atom(<<"completes">>) -> completes;
field_atom(<<"tier">>) -> tier;
field_atom(<<"Completes">>) -> completes;
field_atom(<<"Tier">>) -> tier;
field_atom(<<"Tag">>) -> tag;
field_atom(<<"player_level">>) -> player_level;
field_atom(<<"regular_credits">>) -> regular_credits;
field_atom(<<"premium_credits">>) -> premium_credits;
field_atom(<<"premium_credits_free">>) -> premium_credits_free;
field_atom(<<"fusion_points">>) -> fusion_points;
field_atom(<<"trades_remaining">>) -> trades_remaining;
field_atom(<<"daily_focus">>) -> daily_focus;
field_atom(<<"focus_capacity">>) -> focus_capacity;
field_atom(<<"last_region_played">>) -> last_region_played;
field_atom(_Key) -> undefined.

entry_name(Data, Id, Opts) ->
    case maps:get(<<"item_name">>, Data, undefined) of
        Name when is_binary(Name), byte_size(Name) > 0 -> Name;
        _ ->
            ItemType = maps:get(<<"item_type">>, Data, Id),
            resolved_name(item, ItemType, Opts)
    end.

resolved_name(Kind, Value, Opts) ->
    wf_resolve:resolve(Kind, Value, Opts#{resolve_items => true}).

-doc "Resolve player query fields; arbitrary source data remains available through data.<path>.".
-spec query_field(term(), string() | atom()) -> {ok, map()} | error.
query_field(_Kind, Key0) ->
    KeyText = retinue_text:to_list(Key0),
    Key = string:lowercase(KeyText),
    case Key of
        "name" -> field(name, {entry, name}, string, contains);
        "id" -> field(id, {entry, id}, string, contains);
        "type" -> field(type, {entry, type}, string, eq);
        "source" -> field(source, {entry, source}, string, eq);
        "item_type" -> field(item_type, {entry, item_type}, string, contains);
        "item_name" -> field(item_name, {entry, item_name}, string, contains);
        "instance_id" -> field(instance_id, {entry, instance_id}, string, contains);
        "collection" -> field(collection, {entry, collection}, string, eq);
        "representation" -> field(representation, {entry, representation}, string, eq);
        "origin" -> field(origin, {entry_values, origins}, string, eq);
        "path" -> field(path, {entry, path}, string, contains);
        "kind" -> field(item_kind, {entry, item_kind}, string, eq);
        "count" -> field(count, {entry, count}, number, eq);
        "xp" -> field(xp, {entry, xp}, number, eq);
        "rank" -> field(rank, {entry, rank}, number, eq);
        "skill" -> field(skill, {entry, skill}, string, contains);
        "config_index" -> field(config_index, {entry, config_index}, number, eq);
        "customization_index" -> field(customization_index, {entry, customization_index}, number, eq);
        "upgrade_count" -> field(upgrade_count, {entry, upgrade_count}, number, eq);
        "upgrade_version" -> field(upgrade_version, {entry, upgrade_version}, number, eq);
        "forma_count" -> field(forma_count, {entry, forma_count}, number, eq);
        "feature_flags" -> field(feature_flags, {entry, feature_flags}, number, eq);
        "equipment_id" -> field(equipment_id, {entry, equipment_id}, string, contains);
        "loadout_id" -> field(loadout_id, {entry, loadout_id}, string, contains);
        "group" -> field(group, {entry, group}, string, eq);
        "active" -> field(active, {entry, active}, string, eq);
        "favorite" -> field(favorite, {entry, favorite}, string, eq);
        "equipped" -> field(equipped, {entry, equipped}, string, eq);
        "completed" -> field(completed, {entry, completed}, string, eq);
        "hidden" -> field(hidden, {entry, hidden}, string, eq);
        "is_universal" -> field(is_universal, {entry, is_universal}, string, eq);
        "focus_school" -> field(focus_school, {entry, focus_school}, string, contains);
        "slot" -> field(slot, {entry, slot}, string, eq);
        "standing" -> field(standing, {entry, standing}, number, eq);
        "title" -> field(title, {entry, title}, number, eq);
        "free_favors_earned" -> field(free_favors_earned, {entry, free_favors_earned}, number, eq);
        "free_favors_used" -> field(free_favors_used, {entry, free_favors_used}, number, eq);
        "progress" -> field(progress, {entry, progress}, number, eq);
        "level" -> field(level, {entry, level}, number, eq);
        "school" -> field(school, {entry, school}, string, contains);
        "completes" -> field(completes, {entry, completes}, number, eq);
        "tier" -> field(tier, {entry, tier}, number, eq);
        "player_level" -> field(player_level, {entry, player_level}, number, eq);
        "regular_credits" -> field(regular_credits, {entry, regular_credits}, number, eq);
        "premium_credits" -> field(premium_credits, {entry, premium_credits}, number, eq);
        "premium_credits_free" -> field(premium_credits_free, {entry, premium_credits_free}, number, eq);
        "fusion_points" -> field(fusion_points, {entry, fusion_points}, number, eq);
        "trades_remaining" -> field(trades_remaining, {entry, trades_remaining}, number, eq);
        "daily_focus" -> field(daily_focus, {entry, daily_focus}, number, eq);
        "focus_capacity" -> field(focus_capacity, {entry, focus_capacity}, number, eq);
        "last_region_played" -> field(last_region_played, {entry, last_region_played}, string, contains);
        _ ->
            case {lists:prefix("data.", Key), lists:prefix("typed.", Key)} of
                {true, _} -> field({data, string:slice(KeyText, 5)},
                                   {data_path, string:slice(KeyText, 5)}, dynamic, contains);
                {_, true} -> field({typed, string:slice(KeyText, 6)},
                                   {entry_path, projection, string:slice(KeyText, 6)},
                                   dynamic, contains);
                _ -> error
            end
    end.

-doc "Player sort fields use the same field contract as filters.".
-spec query_sort_field(term(), string() | atom()) -> {ok, map()} | error.
query_sort_field(Kind, Key) -> query_field(Kind, Key).

-doc "Keep typed player entries stable and group them by type and name.".
-spec default_sort(term()) -> [map()].
default_sort(_Kind) ->
    [#{key => type, dir => asc}, #{key => name, dir => asc}].

field(Key, Source, Kind, DefaultOp) ->
    {ok, #{key => Key, source => Source, kind => Kind, default_op => DefaultOp}}.

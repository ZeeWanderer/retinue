%%%-------------------------------------------------------------------
%% Build-planner view of concrete player equipment.
%%%-------------------------------------------------------------------
-module(wf_build_equipment).

-export([snapshot/0, snapshot/1, from_snapshot/2, from_snapshot/3,
         from_snapshot_with_issues/2]).

-define(SCHEMA_VERSION, 3).

-doc "Return normalized equipment definitions and concrete player instances.".
-spec snapshot() -> {ok, map()}.
snapshot() ->
    snapshot(all).

-spec snapshot(all | [binary()]) -> {ok, map()}.
snapshot(InstanceIds) ->
    Player = wf_player_service:snapshot(),
    case wf_item_catalog:load() of
        {ok, Catalog, Meta} ->
            View = from_snapshot(Player, Catalog, InstanceIds),
            CatalogMeta = catalog_meta(Meta),
            {ok, View#{<<"catalog">> => CatalogMeta}};
        {error, Reason} ->
            View = from_snapshot(Player, [], InstanceIds),
            {ok, View#{
                   <<"catalog">> => #{<<"available">> => false,
                                        <<"error">> => error_text(Reason)}}}
    end.

-doc "Build an equipment snapshot from supplied player and catalog data.".
-spec from_snapshot(map(), [map()]) -> map().
from_snapshot(Player, Catalog) ->
    from_snapshot(Player, Catalog, all).

-spec from_snapshot(map(), [map()], all | [binary()]) -> map().
from_snapshot(Player, Catalog, InstanceIds) ->
    {View, _Issues} = from_snapshot_with_issues(Player, Catalog, InstanceIds),
    View.

-doc "Build equipment plus attributed metadata-resolution issues.".
-spec from_snapshot_with_issues(map(), [map()]) -> {map(), [map()]}.
from_snapshot_with_issues(Player, Catalog) ->
    from_snapshot_with_issues(Player, Catalog, all).

from_snapshot_with_issues(Player, Catalog, InstanceIds) ->
    Projection = wf_player_projection:build(Player),
    CatalogIndex = wf_item_catalog:index(Catalog),
    Loadouts = loadout_index(Projection),
    {Definitions, Instances, Issues} = normalize_equipment(
                                         select_instances(Projection, InstanceIds),
                                         CatalogIndex, Loadouts),
    View = #{<<"schema">> => ?SCHEMA_VERSION,
             <<"player_revision">> => maps:get(revision, Player, 0),
             <<"player_updated_at">> => nullable(maps:get(updated_at, Player, undefined)),
             <<"definitions">> => Definitions,
             <<"instances">> => Instances},
    {View, unique_issues(wf_resolution_audit:scan(View) ++ Issues)}.

select_instances(Projection, all) -> maps:get(<<"equipment">>, Projection, []);
select_instances(Projection, InstanceIds) ->
    Ids = sets:from_list(InstanceIds),
    [Item || Item <- maps:get(<<"equipment">>, Projection, []),
             sets:is_element(maps:get(<<"instance_id">>, Item, null), Ids)].

normalize_equipment(Equipment, Catalog, Loadouts) ->
    {DefinitionMap, Instances0, Issues0} = lists:foldl(
      fun(Item, {Definitions, Instances, Issues}) when is_map(Item) ->
          ItemType = maps:get(<<"item_type">>, Item, <<>>),
          case byte_size(ItemType) of
              0 -> {Definitions, Instances, Issues};
              _ ->
                  Builtin = wf_builtin_metadata:equipment(ItemType),
                  Exact = maps:get(ItemType, Catalog, #{}),
                  AliasId = maps:get(catalog_alias, default_map(Builtin), ItemType),
                  Alias = case AliasId =:= ItemType of
                      true -> #{};
                      false -> maps:get(AliasId, Catalog, #{})
                  end,
                  CatalogItem = merge_present(Alias, Exact),
                  Class = equipment_class(maps:get(<<"collection">>, Item, <<>>),
                                          CatalogItem, ItemType),
                  {Definition, DefinitionIssues} =
                      definition(ItemType, Class,
                                 maps:get(<<"collection">>, Item, <<>>),
                                 Exact, AliasId, Alias, CatalogItem, Builtin),
                  Instance = instance(Item, Class, CatalogItem, Catalog,
                                      Loadouts),
                  {Definitions#{ItemType => Definition}, [Instance | Instances],
                   DefinitionIssues ++ Issues}
          end;
         (_Item, Acc) -> Acc
      end,
      {#{}, [], []}, Equipment),
    Definitions = lists:sort(fun definition_before/2, maps:values(DefinitionMap)),
    Instances = lists:sort(fun instance_before/2, Instances0),
    {Definitions, Instances, unique_issues(Issues0)}.

definition(ItemType, Class, Collection, Exact, AliasId, Alias, Catalog, Builtin) ->
    {Name, NameSource} = definition_name(ItemType, Exact, Builtin, Alias),
    {Asset, AssetSource} = definition_asset(ItemType, Exact, Builtin,
                                             AliasId, Alias),
    Base = #{<<"id">> => ItemType,
             <<"item_type">> => ItemType,
             <<"name">> => Name,
             <<"class">> => Class,
             <<"category">> => nullable(maps:get(<<"category">>, Catalog, undefined)),
             <<"type">> => nullable(maps:get(<<"type">>, Catalog, undefined)),
             <<"capacity">> => max(30, number(maps:get(<<"maxLevelCap">>,
                                                        Catalog, 30))),
             <<"metadata_sources">> => #{<<"name">> => NameSource,
                                          <<"asset">> => AssetSource}},
    Base1 = case maps:size(Exact) =:= 0 andalso
                     AliasId =/= ItemType andalso maps:size(Alias) > 0 of
        true -> Base#{<<"catalog_id">> => AliasId};
        false -> Base
    end,
    Definition = (maps:merge(Base1,
                             wf_build_topology:definition(Class, Catalog)))#{
                   <<"asset">> => Asset},
    {Definition, definition_issues(ItemType, Name, NameSource, Asset,
                                   Class, Collection)}.

instance(Item, Class, CatalogItem, Catalog, Loadouts) ->
    Id = maps:get(<<"instance_id">>, Item, maps:get(<<"id">>, Item)),
    ItemType = maps:get(<<"item_type">>, Item),
    Xp = number(maps:get(<<"xp">>, Item, 0)),
    ItemLoadouts = lists:reverse(maps:get(Id, Loadouts, [])),
    Base = #{<<"id">> => Id,
             <<"instance_id">> => Id,
             <<"definition_id">> => ItemType,
             <<"collection">> => maps:get(<<"collection">>, Item, <<>>),
             <<"class">> => Class,
             <<"capacity">> => max(30, number(maps:get(<<"maxLevelCap">>,
                                                        CatalogItem, 30))),
             <<"custom_name">> => nullable(maps:get(<<"item_name">>, Item, undefined)),
             <<"xp">> => Xp,
             <<"forma_count">> => number(maps:get(<<"forma_count">>, Item, 0)),
             <<"feature_flags">> => number(maps:get(<<"feature_flags">>, Item, 0)),
             <<"features">> => maps:get(<<"features">>, Item, #{}),
             <<"unknown_feature_flags">> =>
                 number(maps:get(<<"unknown_feature_flags">>, Item, 0)),
             <<"loadouts">> => ItemLoadouts,
             <<"active_config_indices">> => active_configs(ItemLoadouts)},
    Projected = maps:merge(
                  Base,
                  wf_build_topology:project(Class, CatalogItem, Catalog, Item)),
    copy_present([<<"mod_slot_purchases">>, <<"customization_slot_purchases">>,
                  <<"modular_parts">>, <<"focus_lens">>, <<"skill_tree">>], Item, Projected).

loadout_index(Projection) ->
    Loadouts = maps:from_list(
      [{maps:get(<<"id">>, Item), Item}
       || Item <- maps:get(<<"loadouts">>, Projection, []),
          is_map(Item), maps:is_key(<<"id">>, Item)]),
    lists:foldl(
      fun(Slot, Acc) when is_map(Slot) ->
          case maps:get(<<"instance_id">>, Slot, null) of
              Id when is_binary(Id) ->
                  Parent = maps:get(maps:get(<<"loadout_id">>, Slot, undefined),
                                    Loadouts, #{}),
                  Link = copy_present(
                           [<<"loadout_id">>, <<"group">>, <<"slot">>,
                            <<"config_index">>], Slot,
                           #{<<"name">> => maps:get(<<"name">>, Parent, <<>>),
                             <<"active">> => maps:get(<<"active">>, Parent, false)}),
                  maps:update_with(Id, fun(Values) -> [Link | Values] end,
                                   [Link], Acc);
              _ -> Acc
          end;
         (_Slot, Acc) -> Acc
      end,
      #{}, maps:get(<<"loadout_slots">>, Projection, [])).

active_configs(Loadouts) ->
    lists:usort([Index
                 || #{<<"active">> := true, <<"config_index">> := Index} <- Loadouts,
                    is_integer(Index)]).

catalog_meta(Meta) ->
    #{<<"available">> => true,
      <<"source">> => maps:get(source, Meta, <<"WFCD">>),
      <<"version">> => maps:get(version, Meta, <<"unknown">>),
      <<"fetched_at">> => maps:get(fetched_at, Meta, 0)}.

equipment_class(<<"Suits">>, _Catalog, _Path) -> <<"warframe">>;
equipment_class(<<"LongGuns">>, _Catalog, _Path) -> <<"primary">>;
equipment_class(<<"Pistols">>, _Catalog, _Path) -> <<"secondary">>;
equipment_class(<<"Melee">>, _Catalog, _Path) -> <<"melee">>;
equipment_class(<<"SpaceSuits">>, _Catalog, _Path) -> <<"archwing">>;
equipment_class(<<"SpaceGuns">>, _Catalog, _Path) -> <<"archgun">>;
equipment_class(<<"SpaceMelee">>, _Catalog, _Path) -> <<"archmelee">>;
equipment_class(<<"MechSuits">>, _Catalog, _Path) -> <<"necramech">>;
equipment_class(<<"SentinelWeapons">>, _Catalog, _Path) -> <<"companion_weapon">>;
equipment_class(Collection, _Catalog, _Path)
  when Collection =:= <<"Sentinels">>; Collection =:= <<"Robotics">>;
       Collection =:= <<"KubrowPets">>; Collection =:= <<"MoaPets">> ->
    <<"companion">>;
equipment_class(<<"Hoverboards">>, _Catalog, _Path) -> <<"kdrive">>;
equipment_class(<<"OperatorAmps">>, _Catalog, _Path) -> <<"amp">>;
equipment_class(<<"DataKnives">>, _Catalog, _Path) -> <<"parazon">>;
equipment_class(<<"SpecialItems">>, _Catalog, _Path) -> <<"exalted">>;
equipment_class(Collection, Catalog, _Path) ->
    first_present([maps:get(<<"category">>, Catalog, undefined), Collection], <<"other">>).

definition_before(A, B) ->
    sort_key(maps:get(<<"name">>, A)) =< sort_key(maps:get(<<"name">>, B)).

instance_before(A, B) ->
    {maps:get(<<"definition_id">>, A), maps:get(<<"id">>, A)} =<
        {maps:get(<<"definition_id">>, B), maps:get(<<"id">>, B)}.

sort_key(Value) when is_binary(Value) -> string:casefold(Value);
sort_key(Value) -> Value.

path_name(Path) when is_binary(Path) ->
    case binary:split(Path, <<"/">>, [global, trim_all]) of
        [] -> Path;
        Parts -> lists:last(Parts)
    end;
path_name(Value) -> Value.

copy_present([], _Source, Acc) -> Acc;
copy_present([Key | Rest], Source, Acc) ->
    Next = case maps:get(Key, Source, undefined) of
        undefined -> Acc;
        null -> Acc;
        Value -> Acc#{Key => Value}
    end,
    copy_present(Rest, Source, Next).

first_present([], Default) -> Default;
first_present([Value | Rest], Default) when Value =:= undefined; Value =:= null;
                                                Value =:= <<>> ->
    first_present(Rest, Default);
first_present([Value | _Rest], _Default) -> Value.

number(Value) when is_integer(Value) -> Value;
number(Value) when is_float(Value) -> trunc(Value);
number(_Value) -> 0.

nullable(undefined) -> null;
nullable(Value) -> Value.

error_text(Reason) -> iolist_to_binary(io_lib:format("~p", [Reason])).

definition_name(ItemType, Exact, Builtin, Alias) ->
    case catalog_name(Exact) of
        {ok, Name} -> {Name, <<"wfcd">>};
        error ->
            case maps:get(name, default_map(Builtin), undefined) of
                Name when is_binary(Name), byte_size(Name) > 0 ->
                    {Name, <<"builtin">>};
                _ ->
                    case catalog_name(Alias) of
                        {ok, Name} -> {Name, <<"wfcd">>};
                        error -> {path_name(ItemType), <<"path">>}
                    end
            end
    end.

definition_asset(ItemType, Exact, Builtin, AliasId, Alias) ->
    case catalog_image(Exact) of
        {ok, Image} -> {wfcd_asset(ItemType, ItemType, Image), <<"wfcd">>};
        error ->
            case maps:get(asset, default_map(Builtin), undefined) of
                Asset when is_map(Asset) -> {Asset, <<"builtin">>};
                _ ->
                    case catalog_image(Alias) of
                        {ok, Image} ->
                            {wfcd_asset(ItemType, AliasId, Image), <<"wfcd">>};
                        error -> {null, null}
                    end
            end
    end.

catalog_name(Catalog) -> present_binary(maps:get(<<"name">>, Catalog, undefined)).
catalog_image(Catalog) ->
    present_binary(maps:get(<<"imageName">>, Catalog, undefined)).

present_binary(Value) when is_binary(Value), byte_size(Value) > 0 -> {ok, Value};
present_binary(_Value) -> error.

wfcd_asset(ItemType, CatalogId, Image) ->
    #{<<"id">> => ItemType, <<"source">> => <<"wfcd">>,
      <<"catalog_id">> => CatalogId, <<"image_name">> => Image}.

merge_present(Base, Preferred) ->
    maps:fold(
      fun(_Key, Value, Acc) when Value =:= undefined; Value =:= null;
                                 Value =:= <<>> ->
              Acc;
         (Key, Value, Acc) -> Acc#{Key => Value}
      end,
      Base, Preferred).

definition_issues(ItemType, Name, NameSource, Asset, Class, Collection) ->
    Common = #{<<"identity">> => ItemType, <<"fallback">> => Name,
               <<"class">> => Class, <<"collection">> => Collection},
    NameIssues = case NameSource of
        <<"path">> ->
            [Common#{<<"kind">> => <<"friendly_name">>,
                     <<"reason">> => <<"no metadata source resolved a friendly name">>,
                     <<"attempts">> => [<<"wfcd_exact">>, <<"builtin">>,
                                         <<"wfcd_alias">>, <<"path">>]}];
        _ -> []
    end,
    AssetIssues = case Asset of
        null ->
            [Common#{<<"kind">> => <<"asset">>,
                     <<"reason">> => <<"no metadata source resolved an asset">>,
                     <<"attempts">> => [<<"wfcd_exact">>, <<"builtin">>,
                                         <<"wfcd_alias">>]}];
        _ -> []
    end,
    NameIssues ++ AssetIssues.

unique_issues(Issues) ->
    maps:values(maps:from_list(
                  [{{maps:get(<<"kind">>, Issue), maps:get(<<"identity">>, Issue)}, Issue}
                   || Issue <- Issues])).

default_map(Map) when is_map(Map) -> Map;
default_map(_Value) -> #{}.

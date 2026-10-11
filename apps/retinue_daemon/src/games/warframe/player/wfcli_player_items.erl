%%%-------------------------------------------------------------------
%% Typed item, upgrade, config, and loadout projections from player raw data.
%%%-------------------------------------------------------------------
-module(wfcli_player_items).

-export([build/1, configurable_equipment/1, equipment_collections/0]).

-type projection() :: #{
    binary() := [map()],
    entities := [map()]
}.

-doc "Project item-bearing player collections without discarding their raw records.".
-spec build(map()) -> projection().
build(Raw) when is_map(Raw) ->
    Upgrades0 = upgrade_records(Raw),
    UpgradeIndex = maps:from_list(
      [{Id, Upgrade}
       || Upgrade <- Upgrades0,
          Id <- [maps:get(<<"instance_id">>, Upgrade, undefined)],
          is_binary(Id)]),
    {Equipment, Items, Stacks, Configs} = item_records(Raw, UpgradeIndex),
    Usage = upgrade_usage(Configs),
    Upgrades = [attach_usage(Upgrade, Usage) || Upgrade <- Upgrades0],
    {Loadouts, LoadoutSlots} = loadout_records(Raw, Equipment ++ Items),
    Entities = Equipment ++ Items ++ Stacks ++ Upgrades ++ Configs ++
               Loadouts ++ LoadoutSlots,
    #{<<"equipment">> => Equipment,
      <<"items">> => Items,
      <<"stacks">> => Stacks,
      <<"upgrades">> => Upgrades,
      <<"configs">> => Configs,
      <<"loadouts">> => Loadouts,
      <<"loadout_slots">> => LoadoutSlots,
      entities => Entities};
build(_Raw) ->
    build(#{}).

upgrade_records(Raw) ->
    Instances = indexed_records(
      maps:get(<<"Upgrades">>, Raw, []),
      fun(Index, Item) -> upgrade_instance(Index, Item) end),
    Stacks = indexed_records(
      maps:get(<<"RawUpgrades">>, Raw, []),
      fun(Index, Item) -> upgrade_stack(Index, Item) end),
    Instances ++ Stacks.

upgrade_instance(Index, Raw) ->
    ItemType = maps:get(<<"ItemType">>, Raw, <<>>),
    InstanceId = id_value(maps:get(<<"ItemId">>, Raw, undefined)),
    {Fingerprint, FingerprintKind, Rank} = fingerprint(
      maps:get(<<"UpgradeFingerprint">>, Raw, undefined)),
    Id = fallback_id(InstanceId, <<"upgrade">>, Index),
    Fields0 = #{<<"id">> => Id,
                <<"collection">> => <<"Upgrades">>,
                <<"item_type">> => ItemType,
                <<"instance_id">> => nullable(InstanceId),
                <<"count">> => 1,
                <<"kind">> => FingerprintKind,
                <<"fingerprint">> => Fingerprint},
    Fields = optional(<<"rank">>, Rank, Fields0),
    record(player_upgrade, [<<"Upgrades">>, Index], Raw, Fields).

upgrade_stack(Index, Raw) ->
    ItemType = maps:get(<<"ItemType">>, Raw, <<>>),
    Count = integer(maps:get(<<"ItemCount">>, Raw, 0), 0),
    Fields0 = #{<<"id">> => stack_id(<<"RawUpgrades">>, ItemType, Index),
                <<"collection">> => <<"RawUpgrades">>,
                <<"item_type">> => ItemType,
                <<"instance_id">> => null,
                <<"count">> => Count,
                <<"rank">> => 0,
                <<"kind">> => <<"stack">>},
    Fields = optional(<<"last_added">>, maps:get(<<"LastAdded">>, Raw, undefined),
                      Fields0),
    record(player_upgrade, [<<"RawUpgrades">>, Index], Raw, Fields).

fingerprint(undefined) -> {null, <<"instance">>, undefined};
fingerprint(Value) when is_map(Value) -> fingerprint_map(Value);
fingerprint(Value) when is_binary(Value) ->
    try jsone:decode(Value, [{object_format, map}]) of
        Decoded when is_map(Decoded) -> fingerprint_map(Decoded);
        _ -> {Value, <<"unknown">>, undefined}
    catch _:_ -> {Value, <<"unknown">>, undefined}
    end;
fingerprint(Value) -> {Value, <<"unknown">>, undefined}.

fingerprint_map(Map) ->
    case maps:get(<<"lvl">>, Map, undefined) of
        Rank when is_integer(Rank), Rank >= 0 -> {Map, <<"ranked">>, Rank};
        _ ->
            case maps:is_key(<<"challenge">>, Map) of
                true -> {Map, <<"challenge">>, undefined};
                false -> {Map, <<"instance">>, undefined}
            end
    end.

item_records(Raw, UpgradeIndex) ->
    lists:foldl(
      fun({Collection, Values}, Acc) when is_binary(Collection), is_list(Values) ->
              item_collection(Collection, Values, UpgradeIndex, Acc);
         (_Pair, Acc) -> Acc
      end,
      {[], [], [], []},
      lists:sort(maps:to_list(Raw))).

item_collection(Collection, Values, UpgradeIndex, Acc0) ->
    case special_collection(Collection) of
        true -> Acc0;
        false ->
            lists:foldl(
              fun({Index, Raw}, Acc) when is_map(Raw) ->
                      item_record(Collection, Index, Raw, UpgradeIndex, Acc);
                 (_Value, Acc) -> Acc
              end,
              Acc0,
              lists:enumerate(0, Values))
    end.

item_record(Collection, Index, Raw, UpgradeIndex,
            {Equipment, Items, Stacks, Configs}) ->
    case maps:get(<<"ItemType">>, Raw, undefined) of
        ItemType when is_binary(ItemType) ->
            case item_kind(Collection, Raw) of
                equipment ->
                    {Record, ItemConfigs} = equipment_record(
                                              Collection, Index, Raw, UpgradeIndex),
                    {[Record | Equipment], Items, Stacks, ItemConfigs ++ Configs};
                item ->
                    {Record, ItemConfigs} = instance_record(
                                              Collection, Index, Raw, UpgradeIndex),
                    {Equipment, [Record | Items], Stacks, ItemConfigs ++ Configs};
                stack ->
                    Record = stack_record(Collection, Index, Raw),
                    {Equipment, Items, [Record | Stacks], Configs};
                skip -> {Equipment, Items, Stacks, Configs}
            end;
        _ -> {Equipment, Items, Stacks, Configs}
    end.

item_kind(Collection, Raw) ->
    case lists:member(Collection, equipment_collections()) orelse
         configurable_equipment(Raw) of
        true -> equipment;
        false ->
            case {maps:is_key(<<"ItemId">>, Raw), maps:is_key(<<"ItemCount">>, Raw)} of
                {true, _} -> item;
                {false, true} -> stack;
                _ -> skip
            end
    end.

-doc "Return whether an otherwise unknown record has configurable equipment shape.".
-spec configurable_equipment(term()) -> boolean().
configurable_equipment(Raw) when is_map(Raw) ->
    maps:is_key(<<"ItemId">>, Raw) andalso
    is_list(maps:get(<<"Configs">>, Raw, undefined)) andalso
    lists:any(fun(Key) -> maps:is_key(Key, Raw) end,
              [<<"XP">>, <<"UpgradeVer">>, <<"Features">>, <<"Polarized">>,
               <<"Polarity">>, <<"ModSlotPurchases">>]);
configurable_equipment(_Raw) -> false.

equipment_record(Collection, Index, Raw, UpgradeIndex) ->
    instance_record(player_equipment, Collection, Index, Raw, UpgradeIndex, 1).

instance_record(Collection, Index, Raw, UpgradeIndex) ->
    instance_record(player_item, Collection, Index, Raw, UpgradeIndex, 1).

instance_record(Type, Collection, Index, Raw, UpgradeIndex, DefaultCount) ->
    ItemType = maps:get(<<"ItemType">>, Raw, <<>>),
    InstanceId = id_value(maps:get(<<"ItemId">>, Raw, undefined)),
    Id = fallback_id(InstanceId, Collection, Index),
    Configs = config_records(Collection, Index, Id, ItemType, Raw, UpgradeIndex),
    Fields0 = #{<<"id">> => Id,
                <<"collection">> => Collection,
                <<"item_type">> => ItemType,
                <<"instance_id">> => nullable(InstanceId),
                <<"count">> => integer(maps:get(<<"ItemCount">>, Raw, DefaultCount),
                                      DefaultCount),
                <<"configs">> => [public_record(Config) || Config <- Configs]},
    Fields1 = copy_fields(
      [{<<"xp">>, <<"XP">>},
       {<<"item_name">>, <<"ItemName">>},
       {<<"upgrade_version">>, <<"UpgradeVer">>},
       {<<"forma_count">>, <<"Polarized">>},
       {<<"feature_flags">>, <<"Features">>},
       {<<"mod_slot_purchases">>, <<"ModSlotPurchases">>},
       {<<"customization_slot_purchases">>, <<"CustomizationSlotPurchases">>},
       {<<"focus_lens">>, <<"FocusLens">>},
       {<<"polarity_overrides">>, <<"Polarity">>},
       {<<"modular_parts">>, <<"ModularParts">>},
       {<<"archon_crystal_upgrades">>, <<"ArchonCrystalUpgrades">>},
       {<<"skill_tree">>, <<"SkillTree">>},
       {<<"upgrade_type">>, <<"UpgradeType">>},
       {<<"upgrade_fingerprint">>, <<"UpgradeFingerprint">>}],
      Raw, Fields0),
    Fields2 = attach_features(maps:get(<<"Features">>, Raw, undefined), Fields1),
    {record(Type, [Collection, Index], Raw, Fields2), Configs}.

attach_features(Flags, Fields) when is_integer(Flags), Flags >= 0 ->
    Known = 1 bor 2 bor 4 bor 8 bor 32 bor 64 bor 512 bor 1024,
    Fields#{<<"features">> =>
                #{<<"double_capacity">> => enabled(Flags, 1),
                  <<"utility_slot">> => enabled(Flags, 2),
                  <<"gravimag">> => enabled(Flags, 4),
                  <<"gilded">> => enabled(Flags, 8),
                  <<"arcane_slot">> => enabled(Flags, 32),
                  <<"second_arcane_slot">> => enabled(Flags, 64),
                  <<"incarnon_genesis">> => enabled(Flags, 512),
                  <<"valence_swap">> => enabled(Flags, 1024)},
            <<"unknown_feature_flags">> => Flags band bnot Known};
attach_features(_Flags, Fields) -> Fields.

enabled(Flags, Bit) -> Flags band Bit =/= 0.

stack_record(Collection, Index, Raw) ->
    ItemType = maps:get(<<"ItemType">>, Raw, <<>>),
    Count = integer(maps:get(<<"ItemCount">>, Raw, 0), 0),
    Fields0 = #{<<"id">> => stack_id(Collection, ItemType, Index),
                <<"collection">> => Collection,
                <<"item_type">> => ItemType,
                <<"instance_id">> => null,
                <<"count">> => Count},
    Fields = copy_fields([{<<"item_name">>, <<"ItemName">>},
                          {<<"last_added">>, <<"LastAdded">>}],
                         Raw, Fields0),
    record(player_stack, [Collection, Index], Raw, Fields).

config_records(Collection, ItemIndex, EquipmentId, ItemType, Raw, UpgradeIndex) ->
    Configs = maps:get(<<"Configs">>, Raw, []),
    indexed_records(
      Configs,
      fun(ConfigIndex, Config) ->
          config_record(Collection, ItemIndex, EquipmentId, ItemType,
                        ConfigIndex, Config, UpgradeIndex)
      end).

config_record(Collection, ItemIndex, EquipmentId, ItemType,
              ConfigIndex, Raw, UpgradeIndex) ->
    Slots = upgrade_slots(maps:get(<<"Upgrades">>, Raw, []), UpgradeIndex),
    Id = <<EquipmentId/binary, ":config:", (integer_to_binary(ConfigIndex))/binary>>,
    Fields0 = #{<<"id">> => Id,
                <<"collection">> => Collection,
                <<"equipment_id">> => EquipmentId,
                <<"item_type">> => ItemType,
                <<"config_index">> => ConfigIndex,
                <<"upgrade_count">> => length(Slots),
                <<"upgrade_slots">> => Slots},
    Fields = copy_fields([{<<"name">>, <<"Name">>},
                          {<<"ability_override">>, <<"AbilityOverride">>},
                          {<<"skins">>, <<"Skins">>},
                          {<<"songs">>, <<"Songs">>},
                          {<<"primary_color">>, <<"pricol">>}],
                         Raw, Fields0),
    record(player_config,
           [Collection, ItemIndex, <<"Configs">>, ConfigIndex], Raw, Fields).

upgrade_slots(Values, UpgradeIndex) when is_list(Values) ->
    [Slot
     || {Index, Value} <- lists:enumerate(0, Values),
        Id <- [id_value(Value)],
        is_binary(Id),
        Slot <- [upgrade_slot(Index, Id, maps:get(Id, UpgradeIndex, #{}))]];
upgrade_slots(_Values, _UpgradeIndex) -> [].

upgrade_slot(Index, Id, Upgrade) ->
    case {map_size(Upgrade), Id} of
        {0, <<"/", _/binary>>} ->
            #{<<"slot">> => Index, <<"instance_id">> => null,
              <<"item_type">> => Id, <<"rank">> => 0,
              <<"kind">> => <<"definition">>};
        _ ->
            Base = #{<<"slot">> => Index, <<"instance_id">> => Id},
            copy_present([<<"item_type">>, <<"rank">>, <<"kind">>], Upgrade, Base)
    end.

upgrade_usage(Configs) ->
    lists:foldl(
      fun(Config, Acc0) ->
          lists:foldl(
            fun(Slot, Acc) ->
                case usage_key(Slot) of
                    undefined -> Acc;
                    Key ->
                        Usage =
                            #{<<"equipment_id">> =>
                                  maps:get(<<"equipment_id">>, Config),
                              <<"config_id">> => maps:get(<<"id">>, Config),
                              <<"config_index">> =>
                                  maps:get(<<"config_index">>, Config),
                              <<"slot">> => maps:get(<<"slot">>, Slot)},
                        maps:update_with(Key, fun(Values) -> [Usage | Values] end,
                                         [Usage], Acc)
                end
            end,
            Acc0,
            maps:get(<<"upgrade_slots">>, Config, []))
      end,
      #{},
      Configs).

attach_usage(Upgrade, Usage) ->
    EquippedIn = lists:reverse(maps:get(usage_key(Upgrade), Usage, [])),
    Upgrade#{<<"equipped">> => EquippedIn =/= [],
             <<"equipped_in">> => EquippedIn}.

usage_key(Value) ->
    case maps:get(<<"instance_id">>, Value, undefined) of
        Id when is_binary(Id), byte_size(Id) > 0 -> {instance, Id};
        _ ->
            case maps:get(<<"item_type">>, Value, undefined) of
                ItemType when is_binary(ItemType), byte_size(ItemType) > 0 ->
                    {definition, ItemType};
                _ -> undefined
            end
    end.

loadout_records(Raw, Items) ->
    ItemIndex = maps:from_list(
      [{Id, Item}
       || Item <- Items,
          Id <- [maps:get(<<"instance_id">>, Item, undefined)],
          is_binary(Id)]),
    Active = maps:from_keys(ids_in_value(maps:get(<<"CurrentLoadOutIds">>, Raw, [])), true),
    Presets = maps:get(<<"LoadOutPresets">>, Raw, #{}),
    case is_map(Presets) of
        false -> {[], []};
        true ->
            lists:foldl(
              fun({Group, Values}, Acc) when is_binary(Group), is_list(Values) ->
                      loadout_group(Group, Values, Active, ItemIndex, Acc);
                 (_Pair, Acc) -> Acc
              end,
              {[], []},
              lists:sort(maps:to_list(Presets)))
    end.

loadout_group(Group, Values, Active, ItemIndex, Acc0) ->
    lists:foldl(
      fun({Index, Raw}, {Loadouts, Slots}) when is_map(Raw) ->
              {Loadout, LoadoutSlots} = loadout_record(
                                          Group, Index, Raw, Active, ItemIndex),
              {[Loadout | Loadouts], LoadoutSlots ++ Slots};
         (_Value, Acc) -> Acc
      end,
      Acc0,
      lists:enumerate(0, Values)).

loadout_record(Group, Index, Raw, Active, ItemIndex) ->
    PresetId0 = id_value(maps:get(<<"ItemId">>, Raw, undefined)),
    PresetId = fallback_id(PresetId0, <<"loadout:", Group/binary>>, Index),
    Slots = [loadout_slot(Group, Index, PresetId, Key, Value, ItemIndex)
             || {Key, Value} <- lists:sort(maps:to_list(Raw)),
                is_binary(Key),
                is_map(Value),
                maps:is_key(<<"ItemId">>, Value)],
    Name = case maps:get(<<"n">>, Raw, undefined) of
        Value when is_binary(Value), byte_size(Value) > 0 -> Value;
        _ -> Group
    end,
    Fields0 = #{<<"id">> => PresetId,
                <<"group">> => Group,
                <<"name">> => Name,
                <<"active">> => maps:is_key(PresetId, Active),
                <<"favorite">> => maps:get(<<"Favorite">>, Raw, false) =:= true,
                <<"slots">> => [public_record(Slot) || Slot <- Slots]},
    Fields = copy_fields([{<<"focus_school">>, <<"FocusSchool">>},
                          {<<"preset_icon">>, <<"PresetIcon">>}],
                         Raw, Fields0),
    {record(player_loadout,
            [<<"LoadOutPresets">>, Group, Index], Raw, Fields), Slots}.

loadout_slot(Group, PresetIndex, PresetId, SlotKey, Raw, ItemIndex) ->
    ItemId = id_value(maps:get(<<"ItemId">>, Raw, undefined)),
    Item = maps:get(ItemId, ItemIndex, #{}),
    Id = <<PresetId/binary, ":slot:", SlotKey/binary>>,
    Fields0 = #{<<"id">> => Id,
                <<"loadout_id">> => PresetId,
                <<"group">> => Group,
                <<"slot">> => SlotKey,
                <<"instance_id">> => nullable(ItemId)},
    Fields1 = copy_present([<<"collection">>, <<"item_type">>], Item, Fields0),
    Fields = copy_fields([{<<"config_index">>, <<"mod">>},
                          {<<"customization_index">>, <<"cus">>},
                          {<<"hidden">>, <<"hide">>}],
                         Raw, Fields1),
    record(player_loadout_slot,
           [<<"LoadOutPresets">>, Group, PresetIndex, SlotKey], Raw, Fields).

ids_in_value(Value) when is_list(Value) ->
    lists:usort(lists:flatten([ids_in_value(Item) || Item <- Value]));
ids_in_value(Value) ->
    case id_value(Value) of
        Id when is_binary(Id) -> [Id];
        _ -> []
    end.

record(Type, Path, Raw, Fields) ->
    Origin = origin(Path),
    Fields#{entity_type => Type, origin => Origin,
            origins => [Origin], raw => Raw}.

origin(Path) -> {<<"inventory">>, [<<"raw">> | Path]}.

public_record(Record) ->
    maps:without([entity_type, origin, origins, raw], Record).

copy_fields([], _Raw, Acc) -> Acc;
copy_fields([{Target, Source} | Rest], Raw, Acc) ->
    copy_fields(Rest, Raw, optional(Target, maps:get(Source, Raw, undefined), Acc)).

copy_present([], _Source, Acc) -> Acc;
copy_present([Key | Rest], Source, Acc) ->
    copy_present(Rest, Source, optional(Key, maps:get(Key, Source, undefined), Acc)).

optional(_Key, undefined, Acc) -> Acc;
optional(_Key, null, Acc) -> Acc;
optional(Key, Value, Acc) -> Acc#{Key => Value}.

nullable(undefined) -> null;
nullable(Value) -> Value.

integer(Value, _Default) when is_integer(Value) -> Value;
integer(_Value, Default) -> Default.

fallback_id(Value, _Prefix, _Index) when is_binary(Value), byte_size(Value) > 0 -> Value;
fallback_id(_Value, Prefix, Index) ->
    <<Prefix/binary, "#", (integer_to_binary(Index))/binary>>.

stack_id(Collection, ItemType, _Index) when is_binary(ItemType), byte_size(ItemType) > 0 ->
    <<Collection/binary, ":", ItemType/binary>>;
stack_id(Collection, _ItemType, Index) ->
    fallback_id(undefined, Collection, Index).

id_value(Value) when is_binary(Value), byte_size(Value) > 0 -> Value;
id_value(#{<<"$oid">> := Value}) -> id_value(Value);
id_value(#{<<"$numberLong">> := Value}) -> id_value(Value);
id_value(Value) when is_integer(Value) -> integer_to_binary(Value);
id_value(_Value) -> undefined.

indexed_records(Values, Fun) when is_list(Values) ->
    [Fun(Index, Item)
     || {Index, Item} <- lists:enumerate(0, Values), is_map(Item)];
indexed_records(_Values, _Fun) -> [].

special_collection(Collection) ->
    lists:member(Collection,
      [<<"Upgrades">>, <<"RawUpgrades">>, <<"XPInfo">>,
       <<"PendingRecipes">>, <<"Missions">>, <<"Affiliations">>,
       <<"FocusUpgrades">>, <<"Boosters">>, <<"ChallengeProgress">>]).

-doc "Collections whose records represent configurable equipment instances.".
-spec equipment_collections() -> [binary()].
equipment_collections() ->
    [<<"Suits">>, <<"LongGuns">>, <<"Pistols">>, <<"Melee">>,
     <<"Ships">>, <<"Scoops">>, <<"Sentinels">>, <<"SentinelWeapons">>,
     <<"KubrowPets">>, <<"SpaceSuits">>, <<"SpaceMelee">>, <<"SpaceGuns">>,
     <<"OperatorAmps">>, <<"OperatorSuits">>, <<"Hoverboards">>, <<"MoaPets">>,
     <<"DataKnives">>, <<"CrewShips">>, <<"CrewShipHarnesses">>,
     <<"CrewShipSalvagedWeapons">>, <<"CrewShipWeapons">>, <<"MechSuits">>,
     <<"Robotics">>, <<"DrifterMelee">>, <<"Horses">>, <<"Motorcycles">>].

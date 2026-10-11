%%%-------------------------------------------------------------------
%% Equipment slot topology and current-config enrichment.
%%%-------------------------------------------------------------------
-module(wfcli_build_topology).

-export([definition/2, project/4]).

-define(SCHEMA, 2).

-doc "Return class-specific base topology for one equipment definition.".
-spec definition(binary(), map()) -> map().
definition(Class, CatalogItem) ->
    LayoutClass = layout_class(Class, CatalogItem),
    #{<<"layout_class">> => LayoutClass,
      <<"topology">> => topology(LayoutClass, #{}, [], [])}.

-doc "Project topology, polarities, shards, and enriched configs for one instance.".
-spec project(binary(), map(), map(), map()) -> map().
project(Class, CatalogItem, CatalogIndex, Item) ->
    Configs0 = maps:get(<<"configs">>, Item, []),
    LayoutClass = instance_layout(Class, CatalogItem, CatalogIndex, Configs0),
    Features = maps:get(<<"features">>, Item, #{}),
    Observed = observed_slots(Configs0),
    Shards = shard_slots(LayoutClass,
                         maps:get(<<"archon_crystal_upgrades">>, Item, [])),
    Topology = topology(LayoutClass, Features, Observed, Shards),
    SlotIndex = slot_index(Topology),
    Polarities = effective_polarities(Topology, CatalogItem,
                                      maps:get(<<"polarity_overrides">>, Item, [])),
    PolarityIndex = maps:from_list(
                      [{maps:get(<<"player_index">>, Entry),
                        maps:get(<<"polarity">>, Entry)}
                       || Entry <- Polarities]),
    Configs = [enrich_config(Config, CatalogIndex, SlotIndex, PolarityIndex)
               || Config <- Configs0, is_map(Config)],
    #{<<"topology">> => Topology,
      <<"effective_polarities">> => Polarities,
      <<"shard_slots">> => Shards,
      <<"configs">> => Configs}.

topology(LayoutClass, Features, Observed, _Shards) ->
    Regions0 = regions(LayoutClass, Features),
    Known = maps:from_keys(
              [maps:get(<<"player_index">>, Slot)
               || Region <- Regions0,
                  Slot <- maps:get(<<"slots">>, Region, []),
                  is_integer(maps:get(<<"player_index">>, Slot, undefined))], true),
    Unknown = [Index || Index <- Observed, not maps:is_key(Index, Known)],
    Regions1 = case Unknown of
        [] -> Regions0;
        _ -> Regions0 ++ [region(<<"observed">>, <<"Other slots">>,
                                  min(4, length(Unknown)),
                                  [slot(observed_id(Index), Index, <<"unknown">>,
                                        slot_label(Index), false)
                                   || Index <- Unknown])]
    end,
    Regions = case LayoutClass of
        <<"warframe">> ->
            Regions1 ++ [region(<<"shards">>, <<"Archon shards">>, 5,
                               [#{<<"id">> => <<"shard-", (integer_to_binary(N))/binary>>,
                                  <<"role">> => <<"shard">>,
                                  <<"label">> => <<"Shard ", (integer_to_binary(N))/binary>>,
                                  <<"planner">> => false}
                                || N <- lists:seq(1, 5)])];
        _ -> Regions1
    end,
    #{<<"schema">> => ?SCHEMA,
      <<"layout_class">> => LayoutClass,
      <<"regions">> => Regions}.

regions(<<"warframe">>, Features) ->
    [region(<<"special">>, <<"Special slots">>, 4,
            [unlock(slot(<<"aura">>, 8, <<"aura">>, <<"Aura">>, true), true),
             unlock(slot(<<"exilus">>, 9, <<"exilus">>, <<"Exilus">>, true),
                    feature(Features, <<"utility_slot">>))]),
     regular_region(8),
     region(<<"arcanes">>, <<"Arcanes">>, 4,
            [unlock(slot(<<"arcane-1">>, 10, <<"arcane">>, <<"Arcane 1">>, false),
                    feature(Features, <<"arcane_slot">>)),
             unlock(slot(<<"arcane-2">>, 11, <<"arcane">>, <<"Arcane 2">>, false),
                    feature(Features, <<"second_arcane_slot">>))])];
regions(Class, Features) when Class =:= <<"primary">>; Class =:= <<"secondary">> ->
    ArcaneSlots = case feature(Features, <<"second_arcane_slot">>) of
        true -> [arcane_slot(9, 1, Features), arcane_slot(10, 2, Features)];
        false -> [arcane_slot(9, 1, Features)]
    end,
    [region(<<"special">>, <<"Special slots">>, 4,
            [unlock(slot(<<"exilus">>, 8, <<"exilus">>, <<"Exilus">>, true),
                    feature(Features, <<"utility_slot">>))]),
     regular_region(8),
     region(<<"arcanes">>, <<"Arcanes">>, 4, ArcaneSlots)];
regions(<<"melee">>, Features) ->
    [region(<<"special">>, <<"Special slots">>, 4,
            [slot(<<"stance">>, 8, <<"stance">>, <<"Stance">>, true),
             unlock(slot(<<"exilus">>, 9, <<"exilus">>, <<"Exilus">>, true),
                    feature(Features, <<"utility_slot">>))]),
     regular_region(8),
     region(<<"arcanes">>, <<"Arcanes">>, 4,
            [arcane_slot(10, 1, Features)])];
regions(<<"archgun">>, Features) ->
    [regular_region(8),
     region(<<"arcanes">>, <<"Arcanes">>, 4,
            [arcane_slot(8, 1, Features), arcane_slot(9, 2, Features)])];
regions(<<"amp">>, Features) ->
    [region(<<"arcanes">>, <<"Arcanes">>, 2,
            [arcane_slot(0, 1, Features), arcane_slot(1, 2, Features)])];
regions(<<"parazon">>, _Features) ->
    [region(<<"requiem">>, <<"Requiem">>, 3,
            indexed_slots(<<"requiem">>, <<"Requiem">>, 0, 3, false)),
     region(<<"utility">>, <<"Parazon mods">>, 3,
            indexed_slots(<<"parazon">>, <<"Parazon">>, 3, 3, false))];
regions(<<"necramech">>, _Features) -> [regular_region(12)];
regions(<<"companion">>, _Features) -> [regular_region(10)];
regions(<<"kdrive">>, _Features) -> [regular_region(8)];
regions(_Class, _Features) -> [regular_region(8)].

regular_region(Count) ->
    region(<<"mods">>, <<"Mods">>, min(4, Count),
           regular_slots(Count)).

regular_slots(Count) ->
    [slot(<<"mod-", (integer_to_binary(N + 1))/binary>>,
          Count - N - 1, <<"mod">>,
          <<"Mod ", (integer_to_binary(N + 1))/binary>>, true)
     || N <- lists:seq(0, Count - 1)].

indexed_slots(IdPrefix, LabelPrefix, Start, Count, Planner) ->
    [slot(<<IdPrefix/binary, "-", (integer_to_binary(N + 1))/binary>>,
          Start + N, <<"mod">>,
          <<LabelPrefix/binary, " ", (integer_to_binary(N + 1))/binary>>,
          Planner)
     || N <- lists:seq(0, Count - 1)].

arcane_slot(Index, Number, Features) ->
    Key = case Number of 1 -> <<"arcane_slot">>; _ -> <<"second_arcane_slot">> end,
    unlock(slot(<<"arcane-", (integer_to_binary(Number))/binary>>, Index,
                <<"arcane">>, <<"Arcane ", (integer_to_binary(Number))/binary>>,
                false), feature(Features, Key)).

region(Id, Label, Columns, Slots) ->
    #{<<"id">> => Id, <<"label">> => Label,
      <<"columns">> => max(1, Columns), <<"slots">> => Slots}.

slot(Id, PlayerIndex, Role, Label, Planner) ->
    #{<<"id">> => Id, <<"player_index">> => PlayerIndex,
      <<"build_slot">> => PlayerIndex + 1,
      <<"role">> => Role, <<"label">> => Label, <<"planner">> => Planner}.

unlock(Slot, Unlocked) -> Slot#{<<"unlocked">> => Unlocked}.

feature(Features, Key) when is_map(Features) -> maps:get(Key, Features, false) =:= true;
feature(_Features, _Key) -> false.

observed_slots(Configs) ->
    lists:usort(
      [Index || Config <- Configs, is_map(Config),
                Slot <- maps:get(<<"upgrade_slots">>, Config, []), is_map(Slot),
                Index <- [maps:get(<<"slot">>, Slot, undefined)], is_integer(Index)]).

slot_index(#{<<"regions">> := Regions}) ->
    maps:from_list(
      [{maps:get(<<"player_index">>, Slot), Slot}
       || Region <- Regions, Slot <- maps:get(<<"slots">>, Region, []),
          is_integer(maps:get(<<"player_index">>, Slot, undefined))]).

enrich_config(Config, Catalog, Slots, Polarities) ->
    Upgrades = [enrich_upgrade(Slot, Catalog, Slots, Polarities)
                || Slot <- maps:get(<<"upgrade_slots">>, Config, []), is_map(Slot)],
    Snapshot = Config#{<<"upgrade_slots">> => Upgrades},
    Snapshot#{<<"fingerprint">> => fingerprint(Snapshot)}.

enrich_upgrade(Upgrade, Catalog, Slots, Polarities) ->
    Index = maps:get(<<"slot">>, Upgrade, -1),
    TopologySlot = maps:get(Index, Slots,
                            slot(observed_id(Index), Index, <<"unknown">>,
                                 slot_label(Index), false)),
    ItemType = maps:get(<<"item_type">>, Upgrade, <<>>),
    Item = maps:get(ItemType, Catalog, #{}),
    Details = mod_details(ItemType),
    Base0 = Upgrade#{<<"topology_slot">> => maps:get(<<"id">>, TopologySlot),
                     <<"player_index">> => Index,
                     <<"build_slot">> =>
                         maps:get(<<"build_slot">>, TopologySlot, Index + 1),
                     <<"role">> => maps:get(<<"role">>, TopologySlot)},
    Base1 = copy_present(
              [{<<"name">>, <<"name">>}, {<<"kind_name">>, <<"type">>},
               {<<"category">>, <<"category">>}, {<<"base_drain">>, <<"baseDrain">>},
               {<<"max_rank">>, <<"fusionLimit">>},
               {<<"rarity">>, <<"rarity">>},
               {<<"compat_name">>, <<"compatName">>}], Item, Base0),
    Base2 = copy_mod_details(Details, Base1),
    Base3 = maybe_effects(Base2, Item, Details),
    Base4 = Base3#{<<"mod_variant">> => mod_variant(Item)},
    Base5 = case first([maps:get(<<"polarity">>, Item, undefined),
                        maps:get(polarity, Details, undefined)]) of
        undefined -> Base4;
        Value -> Base4#{<<"polarity">> => polarity(Value)}
    end,
    Base6 = maybe_drain(Base5),
    Base7 = maybe_effective_drain(Base6, TopologySlot, Polarities),
    {Asset, AssetSource} = case maps:get(<<"imageName">>, Item, undefined) of
        Image when is_binary(Image), byte_size(Image) > 0 ->
            {#{<<"id">> => ItemType, <<"source">> => <<"wfcd">>,
               <<"image_name">> => Image}, <<"wfcd">>};
        _ -> {null, null}
    end,
    {Name, NameSource} = case maps:get(<<"name">>, Base7, undefined) of
        Name0 when is_binary(Name0), byte_size(Name0) > 0 -> {Name0, <<"wfcd">>};
        _ -> {path_name(ItemType), <<"path">>}
    end,
    Base7#{<<"name">> => Name, <<"asset">> => Asset,
           <<"metadata_sources">> => #{<<"name">> => NameSource,
                                        <<"asset">> => AssetSource}}.

mod_variant(Item) ->
    Name = maps:get(<<"name">>, Item, <<>>),
    case {maps:get(<<"isRiven">>, Item, false),
          maps:get(<<"isAmalgam">>, Item, false),
          maps:get(<<"isGalvanized">>, Item, false),
          starts_with(Name, <<"Amalgam ">>),
          starts_with(Name, <<"Galvanized ">>)} of
        {true, _, _, _, _} -> <<"riven">>;
        {_, true, _, _, _} -> <<"amalgam">>;
        {_, _, true, _, _} -> <<"galvanized">>;
        {_, _, _, true, _} -> <<"amalgam">>;
        {_, _, _, _, true} -> <<"galvanized">>;
        _ -> <<"standard">>
    end.

starts_with(Value, Prefix) when is_binary(Value), is_binary(Prefix) ->
    binary:match(Value, Prefix) =:= {0, byte_size(Prefix)};
starts_with(_Value, _Prefix) -> false.

mod_details(ItemType) ->
    case wfcli_mod_catalog:details(ItemType) of
        Details when is_map(Details) -> Details;
        _ -> #{}
    end.

copy_mod_details(Details, Base) ->
    lists:foldl(
      fun({Target, Source}, Acc) ->
          case maps:get(Source, Details, undefined) of
              undefined -> Acc;
              Value -> Acc#{Target => Value}
          end
      end, Base,
      [{<<"base_drain">>, base_drain}, {<<"max_rank">>, fusion_limit},
       {<<"rarity">>, rarity}, {<<"compat_name">>, compat}]).

maybe_effects(#{<<"rank">> := Rank} = Upgrade, Item, Details)
  when is_integer(Rank), Rank >= 0 ->
    Levels0 = maps:get(<<"levelStats">>, Item, undefined),
    Levels = case Levels0 of
        undefined -> maps:get(level_stats, Details, undefined);
        _ -> Levels0
    end,
    case Levels of
        Levels when is_list(Levels), length(Levels) > Rank ->
            Level = lists:nth(Rank + 1, Levels),
            case maps:get(<<"stats">>, Level, undefined) of
                Stats when is_list(Stats) -> Upgrade#{<<"effects">> => Stats};
                _ -> Upgrade
            end;
        _ -> Upgrade
    end;
maybe_effects(Upgrade, _Item, _Details) -> Upgrade.

maybe_drain(#{<<"base_drain">> := Base, <<"rank">> := Rank} = Upgrade)
  when is_integer(Base), is_integer(Rank), Rank >= 0 ->
    Upgrade#{<<"drain">> => abs(Base) + Rank};
maybe_drain(Upgrade) -> Upgrade.

maybe_effective_drain(#{<<"drain">> := Drain, <<"polarity">> := ModPolarity} = Upgrade,
                      TopologySlot, Polarities) ->
    Index = maps:get(<<"player_index">>, TopologySlot),
    SlotPolarity = maps:get(Index, Polarities, <<"none">>),
    Compatibility = wfcli_polarity:compatibility(ModPolarity, SlotPolarity),
    Role = maps:get(<<"role">>, TopologySlot, <<"mod">>),
    Effective = case Role of
        <<"aura">> -> wfcli_polarity:aura_value(ModPolarity, SlotPolarity, Drain);
        <<"stance">> -> wfcli_polarity:aura_value(ModPolarity, SlotPolarity, Drain);
        _ -> wfcli_polarity:mod_cost(ModPolarity, SlotPolarity, Drain)
    end,
    Upgrade#{<<"slot_polarity">> => SlotPolarity,
             <<"polarity_state">> => atom_to_binary(Compatibility),
             <<"effective_drain">> => Effective};
maybe_effective_drain(Upgrade, _TopologySlot, _Polarities) -> Upgrade.

effective_polarities(Topology, Catalog, Overrides) ->
    Base = base_polarities(Topology, Catalog),
    OverrideMap = override_polarities(Overrides),
    [begin
         Index = maps:get(<<"player_index">>, Slot),
         {Value, Source} = case maps:find(Index, OverrideMap) of
             {ok, Override} -> {Override, <<"override">>};
             error -> case maps:find(Index, Base) of
                 {ok, Existing} -> {Existing, <<"base">>};
                 error -> {<<"none">>, <<"none">>}
             end
         end,
         #{<<"slot_id">> => maps:get(<<"id">>, Slot),
           <<"player_index">> => Index,
           <<"polarity">> => Value,
           <<"source">> => Source}
     end
     || Region <- maps:get(<<"regions">>, Topology),
        Slot <- maps:get(<<"slots">>, Region, []),
        is_integer(maps:get(<<"player_index">>, Slot, undefined))].

base_polarities(Topology, Catalog) ->
    Regular = maps:get(<<"polarities">>, Catalog, []),
    RegularSlots = [Slot
                    || Region <- maps:get(<<"regions">>, Topology, []),
                       Slot <- maps:get(<<"slots">>, Region, []),
                       maps:get(<<"role">>, Slot, <<>>) =:= <<"mod">>],
    Base0 = maps:from_list(zip_polarities(RegularSlots, Regular)),
    Layout = maps:get(<<"layout_class">>, Topology, <<"other">>),
    Special = case Layout of
        <<"warframe">> -> [{8, maps:get(<<"aura">>, Catalog, undefined)},
                            {9, maps:get(<<"exilusPolarity">>, Catalog, undefined)}];
        <<"melee">> -> [{8, maps:get(<<"stancePolarity">>, Catalog, undefined)},
                         {9, maps:get(<<"exilusPolarity">>, Catalog, undefined)}];
        Class when Class =:= <<"primary">>; Class =:= <<"secondary">> ->
            [{8, maps:get(<<"exilusPolarity">>, Catalog, undefined)}];
        _ -> []
    end,
    lists:foldl(fun
        ({_Index, undefined}, Acc) -> Acc;
        ({Index, Value}, Acc) -> Acc#{Index => polarity(Value)}
    end, Base0, Special).

zip_polarities([Slot | Slots], [Value | Values]) ->
    [{maps:get(<<"player_index">>, Slot), polarity(Value)}
     | zip_polarities(Slots, Values)];
zip_polarities(_Slots, _Values) -> [].

override_polarities(Values) when is_list(Values) ->
    lists:foldl(
      fun(Value, Acc) when is_map(Value) ->
              Index = first([maps:get(<<"Slot">>, Value, undefined),
                             maps:get(<<"slot">>, Value, undefined)]),
              Polarity = first([maps:get(<<"Value">>, Value, undefined),
                                maps:get(<<"value">>, Value, undefined)]),
              case is_integer(Index) andalso Polarity =/= undefined of
                  true -> Acc#{Index => polarity(Polarity)};
                  false -> Acc
              end;
         (_Value, Acc) -> Acc
      end, #{}, Values);
override_polarities(_Values) -> #{}.

shard_slots(<<"warframe">>, Values) when is_list(Values) ->
    [#{<<"slot_id">> => <<"shard-", (integer_to_binary(Index + 1))/binary>>,
       <<"index">> => Index, <<"upgrade">> => Value}
     || {Index, Value} <- lists:enumerate(0, Values)];
shard_slots(_Class, _Values) -> [].

layout_class(<<"exalted">>, Catalog) -> catalog_layout(Catalog, <<"exalted">>);
layout_class(<<"other">>, Catalog) -> catalog_layout(Catalog, <<"other">>);
layout_class(Class, _Catalog) -> Class.

instance_layout(<<"exalted">>, CatalogItem, Catalog, Configs) ->
    Default = layout_class(<<"exalted">>, CatalogItem),
    case equipped_layout(Configs, Catalog) of
        undefined -> Default;
        Layout -> Layout
    end;
instance_layout(Class, CatalogItem, _Catalog, _Configs) ->
    layout_class(Class, CatalogItem).

equipped_layout(Configs, Catalog) ->
    first_layout(
      [catalog_layout(maps:get(maps:get(<<"item_type">>, Upgrade, <<>>),
                               Catalog, #{}), undefined)
       || Config <- Configs, is_map(Config),
          Upgrade <- maps:get(<<"upgrade_slots">>, Config, []), is_map(Upgrade)]).

first_layout([undefined | Rest]) -> first_layout(Rest);
first_layout([<<"warframe">> | Rest]) -> first_layout(Rest);
first_layout([Layout | _Rest]) -> Layout;
first_layout([]) -> undefined.

catalog_layout(Catalog, Default) ->
    Text = string:casefold(iolist_to_binary(
             [maps:get(<<"exaltedSlot">>, Catalog, <<>>), <<" ">>,
              maps:get(<<"category">>, Catalog, <<>>), <<" ">>,
              maps:get(<<"type">>, Catalog, <<>>), <<" ">>,
              maps:get(<<"productCategory">>, Catalog, <<>>)])),
    classify_text(Text,
                  [{<<"warframe">>, [<<"warframe">>]},
                   {<<"secondary">>, [<<"secondary">>, <<"pistol">>]},
                   {<<"primary">>, [<<"primary">>, <<"rifle">>, <<"shotgun">>]},
                   {<<"melee">>, [<<"melee">>]},
                   {<<"archgun">>, [<<"archgun">>, <<"arch-gun">>]}],
                  Default).

classify_text(_Text, [], Default) -> Default;
classify_text(Text, [{Class, Needles} | Rest], Default) ->
    case lists:any(fun(Needle) -> binary:match(Text, Needle) =/= nomatch end,
                   Needles) of
        true -> Class;
        false -> classify_text(Text, Rest, Default)
    end.

polarity(Value) -> atom_to_binary(wfcli_polarity:normalize(Value)).

fingerprint(Value) ->
    binary:encode_hex(crypto:hash(sha256, term_to_binary(Value, [deterministic])),
                      lowercase).

observed_id(Index) -> <<"observed-", (integer_to_binary(Index + 1))/binary>>.
slot_label(Index) -> <<"Slot ", (integer_to_binary(Index + 1))/binary>>.

path_name(Path) when is_binary(Path) ->
    case binary:split(Path, <<"/">>, [global, trim_all]) of
        [] -> Path;
        Parts -> lists:last(Parts)
    end;
path_name(Value) -> Value.

copy_present([], _Source, Acc) -> Acc;
copy_present([{Target, SourceKey} | Rest], Source, Acc) ->
    Next = case maps:get(SourceKey, Source, undefined) of
        undefined -> Acc;
        null -> Acc;
        Value -> Acc#{Target => Value}
    end,
    copy_present(Rest, Source, Next).

first([undefined | Rest]) -> first(Rest);
first([Value | _Rest]) -> Value;
first([]) -> undefined.

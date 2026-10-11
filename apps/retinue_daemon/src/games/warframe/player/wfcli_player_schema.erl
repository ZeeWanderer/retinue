%%%-------------------------------------------------------------------
%% Non-fatal schema audit for player fields understood by projections.
%%%-------------------------------------------------------------------
-module(wfcli_player_schema).

-export([audit/1]).

-doc "Return unknown fields and shape mismatches in understood raw collections.".
-spec audit(map()) -> [map()].
audit(Raw) when is_map(Raw) ->
    Issues = lists:append(
      [audit_equipment(Raw),
       audit_records(<<"Upgrades">>, [<<"ItemId">>, <<"ItemType">>],
                     [<<"ItemId">>, <<"ItemType">>, <<"UpgradeFingerprint">>], Raw),
       audit_records(<<"RawUpgrades">>, [<<"ItemType">>, <<"ItemCount">>],
                     [<<"ItemType">>, <<"ItemCount">>, <<"LastAdded">>], Raw),
       audit_records(<<"XPInfo">>, [<<"ItemType">>, <<"XP">>],
                     [<<"ItemType">>, <<"XP">>], Raw),
       audit_records(<<"PendingRecipes">>, [<<"ItemType">>],
                     [<<"ItemType">>, <<"ItemId">>, <<"CompletionDate">>], Raw),
       audit_records(<<"Missions">>, [<<"Tag">>],
                     [<<"Tag">>, <<"Completes">>, <<"Tier">>], Raw),
       audit_records(<<"Affiliations">>, [<<"Tag">>],
                     [<<"Tag">>, <<"Standing">>, <<"Title">>, <<"Initiated">>,
                      <<"FreeFavorsEarned">>, <<"FreeFavorsUsed">>,
                      <<"WeeklyMissions">>], Raw),
       audit_records(<<"FocusUpgrades">>, [<<"ItemType">>],
                     [<<"ItemType">>, <<"Level">>, <<"IsUniversal">>], Raw),
       audit_records(<<"Boosters">>, [<<"ItemType">>, <<"ExpiryDate">>],
                     [<<"ItemType">>, <<"ExpiryDate">>], Raw),
       audit_records(<<"ChallengeProgress">>, [<<"Name">>, <<"Progress">>],
                     [<<"Name">>, <<"Progress">>, <<"Completed">>], Raw),
       audit_integer_map(<<"PlayerSkills">>, Raw),
       audit_integer_map(<<"FocusXP">>, Raw),
       audit_loadouts(Raw)]),
    lists:sort(Issues);
audit(_Raw) ->
    [issue(wrong_type, [<<"raw">>], #{expected => map})].

audit_equipment(Raw) ->
    Collections = lists:usort(
                    wfcli_player_items:equipment_collections() ++
                    [Collection
                     || {Collection, Values} <- maps:to_list(Raw),
                        is_binary(Collection), is_list(Values),
                        lists:any(fun wfcli_player_items:configurable_equipment/1,
                                  Values)]),
    lists:append(
      [audit_equipment_collection(Collection, Raw)
       || Collection <- Collections]).

audit_equipment_collection(Collection, Raw) ->
    Values = maps:get(Collection, Raw, undefined),
    case {lists:member(Collection, wfcli_player_items:equipment_collections()), Values} of
        {true, _} ->
            Issues = audit_records(Collection, [<<"ItemId">>, <<"ItemType">>],
                                   equipment_fields(), Raw),
            Issues ++ audit_equipment_configs(Collection, Values, fun erlang:is_map/1);
        {false, Records} when is_list(Records) ->
            lists:append(
              [audit_object([Collection, Index], Item,
                            [<<"ItemId">>, <<"ItemType">>], equipment_fields()) ++
               audit_configs(Collection, Index, Item)
               || {Index, Item} <- lists:enumerate(0, Records),
                  wfcli_player_items:configurable_equipment(Item)]);
        _ -> []
    end.

audit_equipment_configs(Collection, Values, Predicate) when is_list(Values) ->
    lists:append(
      [audit_configs(Collection, Index, Item)
       || {Index, Item} <- lists:enumerate(0, Values), Predicate(Item)]);
audit_equipment_configs(_Collection, _Values, _Predicate) -> [].

audit_configs(Collection, ItemIndex, Item) ->
    case maps:get(<<"Configs">>, Item, undefined) of
        undefined -> [];
        Configs when is_list(Configs) ->
            lists:append(
              [audit_config([Collection, ItemIndex, <<"Configs">>, ConfigIndex], Config)
               || {ConfigIndex, Config} <- lists:enumerate(0, Configs)]);
        _ -> [issue(wrong_type, [Collection, ItemIndex, <<"Configs">>],
                    #{expected => list})]
    end.

audit_records(Collection, Required, Allowed, Raw) ->
    case maps:get(Collection, Raw, undefined) of
        undefined -> [];
        Values when is_list(Values) ->
            lists:append(
              [audit_object([Collection, Index], Item, Required, Allowed)
               || {Index, Item} <- lists:enumerate(0, Values)]);
        _ -> [issue(wrong_type, [Collection], #{expected => list})]
    end.

audit_object(Path, Item, Required, Allowed) when is_map(Item) ->
    Missing = [Key || Key <- Required, not maps:is_key(Key, Item)],
    Unknown = lists:sort(maps:keys(maps:without(Allowed, Item))),
    MissingIssues = case Missing of
        [] -> [];
        _ -> [issue(missing_fields, Path, #{fields => Missing})]
    end,
    UnknownIssues = case Unknown of
        [] -> [];
        _ -> [issue(unknown_fields, Path, #{fields => Unknown})]
    end,
    MissingIssues ++ UnknownIssues ++ audit_field_types(Path, Item);
audit_object(Path, _Item, _Required, _Allowed) ->
    [issue(wrong_type, Path, #{expected => map})].

audit_config(_Path, []) -> [];
audit_config(Path, Config) -> audit_object(Path, Config, [], config_fields()).

audit_field_types(Path, Item) ->
    [issue(wrong_field_type, Path ++ [Key],
           #{expected => Kind, actual => value_kind(Value)})
     || {Key, Value} <- maps:to_list(Item),
        Kind <- [expected_kind(Key)],
        Kind =/= any,
        not valid_kind(Kind, Value)].

audit_integer_map(Key, Raw) ->
    case maps:get(Key, Raw, undefined) of
        undefined -> [];
        Values when is_map(Values) ->
            [issue(wrong_field_type, [Key, Child],
                   #{expected => integer, actual => value_kind(Value)})
             || {Child, Value} <- maps:to_list(Values), not is_integer(Value)];
        _ -> [issue(wrong_type, [Key], #{expected => map})]
    end.

audit_loadouts(Raw) ->
    case maps:get(<<"LoadOutPresets">>, Raw, undefined) of
        undefined -> [];
        Presets when is_map(Presets) ->
            lists:append(
              [audit_loadout_group(Group, Values)
               || {Group, Values} <- maps:to_list(Presets)]);
        _ -> [issue(wrong_type, [<<"LoadOutPresets">>], #{expected => map})]
    end.

audit_loadout_group(Group, Values) when is_list(Values) ->
    lists:append(
      [audit_loadout([<<"LoadOutPresets">>, Group, Index], Preset)
       || {Index, Preset} <- lists:enumerate(0, Values)]);
audit_loadout_group(Group, _Values) ->
    [issue(wrong_type, [<<"LoadOutPresets">>, Group], #{expected => list})].

audit_loadout(Path, Preset) when is_map(Preset) ->
    Required = [<<"ItemId">>],
    Unknown = [Key
               || {Key, Value} <- maps:to_list(Preset),
                  not lists:member(Key, loadout_fields()),
                  not loadout_slot(Key, Value)],
    Missing = [Key || Key <- Required, not maps:is_key(Key, Preset)],
    Base = case Missing of
        [] -> [];
        _ -> [issue(missing_fields, Path, #{fields => Missing})]
    end,
    UnknownIssues = case Unknown of
        [] -> [];
        _ -> [issue(unknown_fields, Path, #{fields => lists:sort(Unknown)})]
    end,
    SlotIssues = lists:append(
      [audit_loadout_slot(Path ++ [Key], Value)
       || {Key, Value} <- maps:to_list(Preset), loadout_slot(Key, Value)]),
    Base ++ UnknownIssues ++ SlotIssues;
audit_loadout(Path, _Preset) ->
    [issue(wrong_type, Path, #{expected => map})].

loadout_slot(Key, Value) ->
    is_binary(Key) andalso byte_size(Key) =:= 1 andalso
    is_map(Value) andalso maps:is_key(<<"ItemId">>, Value).

audit_loadout_slot(Path, Slot) ->
    Allowed = [<<"ItemId">>, <<"mod">>, <<"cus">>, <<"hide">>],
    audit_object(Path, Slot, [<<"ItemId">>], Allowed).

issue(Kind, Path, Extra) ->
    Extra#{kind => Kind, path => Path}.

expected_kind(<<"ItemId">>) -> id;
expected_kind(<<"ItemType">>) -> binary;
expected_kind(<<"ItemName">>) -> binary;
expected_kind(<<"XP">>) -> integer;
expected_kind(<<"ItemCount">>) -> integer;
expected_kind(<<"UpgradeVer">>) -> integer;
expected_kind(<<"Polarized">>) -> integer;
expected_kind(<<"Features">>) -> integer;
expected_kind(<<"ModSlotPurchases">>) -> integer;
expected_kind(<<"CustomizationSlotPurchases">>) -> integer;
expected_kind(<<"Configs">>) -> list;
expected_kind(<<"Polarity">>) -> list;
expected_kind(<<"ModularParts">>) -> list;
expected_kind(<<"ArchonCrystalUpgrades">>) -> list;
expected_kind(<<"Upgrades">>) -> list;
expected_kind(<<"Tag">>) -> binary;
expected_kind(<<"Name">>) -> binary;
expected_kind(<<"Progress">>) -> integer;
expected_kind(<<"Completes">>) -> integer;
expected_kind(<<"Tier">>) -> integer;
expected_kind(<<"Standing">>) -> integer;
expected_kind(<<"Title">>) -> integer;
expected_kind(<<"Level">>) -> integer;
expected_kind(<<"IsUniversal">>) -> boolean;
expected_kind(<<"Completed">>) -> boolean_or_list;
expected_kind(<<"Favorite">>) -> boolean;
expected_kind(<<"UpgradeFingerprint">>) -> fingerprint;
expected_kind(_) -> any.

valid_kind(_Kind, null) -> true;
valid_kind(binary, Value) -> is_binary(Value);
valid_kind(integer, Value) -> is_integer(Value);
valid_kind(boolean, Value) -> is_boolean(Value);
valid_kind(boolean_or_list, Value) -> is_boolean(Value) orelse is_list(Value);
valid_kind(list, Value) -> is_list(Value);
valid_kind(map, Value) -> is_map(Value);
valid_kind(id, Value) -> is_binary(Value) orelse
                         (is_map(Value) andalso
                          (maps:is_key(<<"$oid">>, Value) orelse
                           maps:is_key(<<"$numberLong">>, Value)));
valid_kind(fingerprint, Value) -> is_binary(Value) orelse is_map(Value);
valid_kind(any, _Value) -> true.

value_kind(Value) when is_map(Value) -> map;
value_kind(Value) when is_list(Value) -> list;
value_kind(Value) when is_binary(Value) -> binary;
value_kind(Value) when is_integer(Value) -> integer;
value_kind(Value) when is_float(Value) -> float;
value_kind(Value) when is_boolean(Value) -> boolean;
value_kind(null) -> null;
value_kind(_) -> other.

equipment_fields() ->
    [<<"AirSupportPower">>, <<"ArchonCrystalUpgrades">>, <<"Configs">>,
     <<"CrewMembers">>, <<"Details">>, <<"Features">>, <<"FocusLens">>,
     <<"InfestationDate">>, <<"IsNew">>, <<"ItemId">>, <<"ItemName">>,
     <<"ItemType">>, <<"ModSlotPurchases">>, <<"ModularParts">>,
     <<"CustomizationSlotPurchases">>,
     <<"Polarity">>, <<"Polarized">>, <<"RailjackImage">>, <<"ShipExterior">>,
     <<"SkillTree">>, <<"UpgradeFingerprint">>, <<"UpgradeType">>,
     <<"UpgradeVer">>, <<"Weapon">>, <<"XP">>].

config_fields() ->
    [<<"AbilityOverride">>, <<"Name">>, <<"Skins">>, <<"Songs">>,
     <<"Upgrades">>, <<"eyecol">>, <<"facial">>, <<"pricol">>].

loadout_fields() ->
    [<<"Favorite">>, <<"FocusSchool">>, <<"ItemId">>, <<"PresetIcon">>, <<"n">>].

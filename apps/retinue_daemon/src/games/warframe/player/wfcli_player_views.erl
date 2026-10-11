%%%-------------------------------------------------------------------
%% Catalog joins for desktop player views.
%%%-------------------------------------------------------------------
-module(wfcli_player_views).

-export([foundry/0, inventory/0, mastery/0,
         foundry/2, inventory/2, mastery/2, mastery/3]).

-define(VIEW_CACHE, wfcli_player_view_cache).

-doc "Build Foundry view from current daemon snapshot and managed item catalog.".
-spec foundry() -> {ok, map()} | {error, term()}.
foundry() -> with_catalog(foundry, fun foundry/2).

-doc "Build inventory view from current daemon snapshot and managed item catalog.".
-spec inventory() -> {ok, map()} | {error, term()}.
inventory() -> with_catalog(inventory, fun inventory/2).

-doc "Build mastery view from current daemon snapshot and managed item catalog.".
-spec mastery() -> {ok, map()} | {error, term()}.
mastery() ->
    case load_star_chart() of
        {ok, Chart, Meta} ->
            Version = {maps:get(version, Meta, undefined),
                       maps:get(fetched_at, Meta, undefined)},
            with_catalog(mastery,
                         fun(Snapshot, Catalog) -> mastery(Snapshot, Catalog, Chart) end,
                         Version);
        {error, _Reason} ->
            with_catalog(mastery, fun mastery/2)
    end.

-doc "Build Foundry view from supplied data; exposed for deterministic tests.".
-spec foundry(map(), [map()]) -> {ok, map()}.
foundry(Snapshot, Catalog) ->
    Player = wfcli_player_projection:build(Snapshot),
    Owned = aggregate(owned_records(Player)),
    Mastery = mastery_index(maps:get(<<"mastery">>, Player, [])),
    Pending = pending_index(maps:get(<<"pending_recipes">>, Player, []), Catalog),
    Subsumed = subsumed_index(maps:get(<<"raw">>, Player, #{})),
    Items = [foundry_item(Item, Owned, Mastery, Pending, Subsumed)
             || Item <- Catalog, mastery_item_supported(Item)],
    Sorted = lists:sort(fun name_before/2, Items),
    {ok, (base_response(Snapshot, Player))#{
        <<"items">> => Sorted,
        <<"summary">> => foundry_summary(Sorted)
    }}.

-doc "Build inventory view from supplied data; exposed for deterministic tests.".
-spec inventory(map(), [map()]) -> {ok, map()}.
inventory(Snapshot, Catalog) ->
    Player = wfcli_player_projection:build(Snapshot),
    CatalogIndex = wfcli_item_catalog:index(Catalog),
    Mastery = mastery_index(maps:get(<<"mastery">>, Player, [])),
    Stacks = aggregate(maps:get(<<"stacks">>, Player, [])),
    Upgrades = aggregate_upgrades(maps:get(<<"upgrades">>, Player, [])),
    StackItems = [inventory_item(Unique, Entry,
                                 maps:get(Unique, CatalogIndex, #{}), Mastery)
                  || {Unique, Entry} <- maps:to_list(Stacks),
                     maps:get(count, Entry, 0) > 0],
    UpgradeItems = [inventory_item(Id, Entry,
                                   maps:get(maps:get(item_type, Entry),
                                            CatalogIndex, #{}), Mastery)
                    || {Id, Entry} <- maps:to_list(Upgrades),
                       maps:get(count, Entry, 0) > 0],
    SetItems = [Set || Item <- Catalog,
                       Set <- [inventory_set(Item, Stacks, Mastery)],
                       Set =/= undefined],
    Items = StackItems ++ UpgradeItems ++ SetItems,
    Sorted = lists:sort(fun name_before/2, Items),
    {ok, (base_response(Snapshot, Player))#{
        <<"items">> => Sorted,
        <<"summary">> => inventory_summary(Sorted)
    }}.

-doc "Build mastery planner view from supplied data; exposed for deterministic tests.".
-spec mastery(map(), [map()]) -> {ok, map()}.
mastery(Snapshot, Catalog) ->
    mastery(Snapshot, Catalog, undefined).

-doc "Build mastery view with supplied Star Chart metadata.".
-spec mastery(map(), [map()], map() | undefined) -> {ok, map()}.
mastery(Snapshot, Catalog, StarChart) ->
    Player = wfcli_player_projection:build(Snapshot),
    Owned = aggregate(owned_records(Player)),
    Mastery = mastery_index(maps:get(<<"mastery">>, Player, [])),
    Pending = pending_index(maps:get(<<"pending_recipes">>, Player, []), Catalog),
    Items = [mastery_item(Item, Owned, Mastery, Pending)
             || Item <- Catalog, mastery_item_supported(Item)],
    Sorted = lists:sort(fun mastery_before/2, Items),
    Profile = profile(maps:get(<<"profile">>, Player, #{})),
    {ok, (base_response(Snapshot, Player))#{
        <<"items">> => Sorted,
        <<"summary">> => mastery_summary(Sorted, Profile, Player, StarChart)
    }}.

with_catalog(View, Build) ->
    with_catalog(View, Build, undefined).

with_catalog(View, Build, ExtraVersion) ->
    case wfcli_item_catalog:load() of
        {ok, Catalog, Meta} -> cached_view(View, Build, Catalog, Meta, ExtraVersion);
        {error, _Reason} ->
            case wfcli_source_manager:ensure_catalog("player_views", #{}) of
                ok ->
                    case wfcli_item_catalog:load() of
                        {ok, Catalog, Meta} ->
                            cached_view(View, Build, Catalog, Meta, ExtraVersion);
                        {error, _LoadReason} = Error -> Error
                    end;
                {error, _EnsureReason} = Error -> Error
            end
    end.

cached_view(View, Build, Catalog, Meta, ExtraVersion) ->
    Snapshot = wfcli_player_service:snapshot(),
    Key = {View, maps:get(revision, Snapshot, 0),
           maps:get(version, Meta, undefined), maps:get(fetched_at, Meta, undefined),
           ExtraVersion, ?MODULE:module_info(md5),
           wfcli_item_catalog:module_info(md5),
           maps:get(recipe_version, Meta, undefined)},
    case cache_lookup(Key) of
        {ok, Result} -> Result;
        error ->
            Result = Build(Snapshot, Catalog),
            cache_store(Key, Result),
            Result
    end.

load_star_chart() ->
    case wfcli_star_chart:load() of
        {ok, _Chart, _Meta} = Result -> Result;
        {error, _Reason} ->
            case wfcli_source_manager:ensure_catalog("mastery_star_chart", #{}) of
                ok -> wfcli_star_chart:load();
                {error, _EnsureReason} = Error -> Error
            end
    end.

cache_lookup(Key) ->
    try ets:lookup(?VIEW_CACHE, Key) of
        [{Key, Result}] -> {ok, Result};
        [] -> error
    catch error:badarg -> error
    end.

cache_store(Key, Result) ->
    try ets:insert(?VIEW_CACHE, {Key, Result})
    catch error:badarg -> false
    end,
    ok.

base_response(Snapshot, Player) ->
    #{<<"revision">> => maps:get(revision, Snapshot, 0),
      <<"updated_at">> => nullable(maps:get(updated_at, Snapshot, undefined)),
      <<"profile">> => profile(maps:get(<<"profile">>, Player, #{}))}.

owned_records(Player) ->
    maps:get(<<"equipment">>, Player, []) ++
    maps:get(<<"items">>, Player, []) ++
    maps:get(<<"stacks">>, Player, []).

profile(Profile) when is_map(Profile) ->
    case maps:get(<<"player_level">>, Profile, undefined) of
        Level when is_integer(Level), Level >= 0 ->
            Profile#{<<"rank_asset">> => rank_asset(Level)};
        Level when is_float(Level), Level >= 0 ->
            Profile#{<<"rank_asset">> => rank_asset(trunc(Level))};
        _ -> Profile
    end;
profile(_Profile) -> #{}.

rank_asset(Level) ->
    Rank = integer_to_binary(Level),
    #{<<"id">> => <<"mastery-rank:", Rank/binary>>,
      <<"source">> => <<"mastery">>,
      <<"image_name">> => <<Rank/binary, ".webp">>}.

aggregate(Entries) when is_list(Entries) ->
    lists:foldl(fun aggregate_entry/2, #{}, Entries);
aggregate(_Entries) -> #{}.

aggregate_entry(Entry, Acc) when is_map(Entry) ->
    case maps:get(<<"item_type">>, Entry, undefined) of
        Unique when is_binary(Unique) ->
            Current = maps:get(Unique, Acc, #{count => 0, xp => 0, name => undefined,
                                             collections => []}),
            Count = max(0, number(maps:get(<<"count">>, Entry, 0))),
            Xp = max(number(maps:get(<<"xp">>, Entry, 0)), maps:get(xp, Current)),
            Name = first_present([maps:get(<<"item_name">>, Entry, undefined),
                                  maps:get(name, Current)]),
            Collection = maps:get(<<"collection">>, Entry, <<>>),
            Collections = lists:usort([Collection | maps:get(collections, Current)]),
            Acc#{Unique => Current#{count => maps:get(count, Current) + Count,
                                    xp => Xp, name => Name,
                                    collections => Collections}};
        _ -> Acc
    end;
aggregate_entry(_Entry, Acc) -> Acc.

aggregate_upgrades(Entries) when is_list(Entries) ->
    lists:foldl(fun aggregate_upgrade/2, #{}, Entries);
aggregate_upgrades(_Entries) -> #{}.

aggregate_upgrade(Entry, Acc) when is_map(Entry) ->
    case maps:get(<<"item_type">>, Entry, undefined) of
        ItemType when is_binary(ItemType), byte_size(ItemType) > 0 ->
            Id = upgrade_variant_id(ItemType, Entry),
            Current = maps:get(
                        Id, Acc,
                        #{id => Id, item_type => ItemType, count => 0, xp => 0,
                          name => undefined, collections => [], equipped => false,
                          equipped_in => [], kind => maps:get(<<"kind">>, Entry,
                                                              <<"upgrade">>)}),
            Count = max(0, number(maps:get(<<"count">>, Entry, 0))),
            Collection = maps:get(<<"collection">>, Entry, <<>>),
            Collections = lists:usort([Collection | maps:get(collections, Current)]),
            EquippedIn = lists:usort(maps:get(equipped_in, Current) ++
                                      list_value(maps:get(<<"equipped_in">>, Entry, []))),
            Updated0 = Current#{count => maps:get(count, Current) + Count,
                                collections => Collections,
                                equipped => maps:get(equipped, Current) orelse
                                            maps:get(<<"equipped">>, Entry, false),
                                equipped_in => EquippedIn},
            Updated = case maps:get(<<"rank">>, Entry, undefined) of
                Rank when is_integer(Rank), Rank >= 0 -> Updated0#{rank => Rank};
                _ -> Updated0
            end,
            Acc#{Id => Updated};
        _ -> Acc
    end;
aggregate_upgrade(_Entry, Acc) -> Acc.

upgrade_variant_id(ItemType, Entry) ->
    case maps:get(<<"rank">>, Entry, undefined) of
        Rank when is_integer(Rank), Rank >= 0 ->
            <<ItemType/binary, "#rank:", (integer_to_binary(Rank))/binary>>;
        _ ->
            case maps:get(<<"instance_id">>, Entry, undefined) of
                Id when is_binary(Id), byte_size(Id) > 0 ->
                    <<ItemType/binary, "#instance:", Id/binary>>;
                _ -> maps:get(<<"id">>, Entry, ItemType)
            end
    end.

mastery_index(Entries) when is_list(Entries) ->
    maps:from_list([{maps:get(<<"item_type">>, Entry),
                     number(maps:get(<<"xp">>, Entry, 0))}
                    || Entry <- Entries, is_map(Entry),
                       is_binary(maps:get(<<"item_type">>, Entry, undefined))]);
mastery_index(_Entries) -> #{}.

pending_index(Pending, Catalog) ->
    RecipeParents = lists:foldl(
      fun(Item, Acc) ->
          Parent = maps:get(<<"uniqueName">>, Item, undefined),
          lists:foldl(
            fun(Component, A) ->
                case {maps:get(<<"name">>, Component, <<>>),
                      maps:get(<<"uniqueName">>, Component, undefined)} of
                    {<<"Blueprint">>, Recipe} when is_binary(Recipe), is_binary(Parent) ->
                        A#{Recipe => Parent};
                    _ -> A
                end
            end, Acc, maps:get(<<"components">>, Item, []))
      end, #{}, Catalog),
    maps:from_list(
      [{maps:get(maps:get(<<"item_type">>, Entry), RecipeParents,
                 maps:get(<<"item_type">>, Entry)), true}
       || Entry <- Pending, is_map(Entry),
          is_binary(maps:get(<<"item_type">>, Entry, undefined))]).

subsumed_index(Raw) when is_map(Raw) ->
    Foundry = maps:get(<<"InfestedFoundry">>, Raw, #{}),
    Suits = maps:get(<<"ConsumedSuits">>, Foundry, []),
    maps:from_list([{Unique, true}
                    || Suit <- Suits, is_map(Suit),
                       Unique <- [maps:get(<<"s">>, Suit, undefined)],
                       is_binary(Unique)]);
subsumed_index(_Raw) -> #{}.

inventory_item(Unique, Entry, Catalog, Mastery) ->
    ItemType = maps:get(item_type, Entry, Unique),
    {Name, NameSource} = item_name(ItemType, Entry, Catalog),
    Category = maps:get(<<"category">>, Catalog,
                        maps:get(<<"parentCategory">>, Catalog, <<>>)),
    Collections = maps:get(collections, Entry, []),
    Group = inventory_group(ItemType, Catalog, Category, Collections),
    Tradable = maps:get(<<"tradable">>, Catalog, false) =:= true,
    MasteryKey = case maps:get(<<"component">>, Catalog, false) of
                     true -> maps:get(<<"parentUniqueName">>, Catalog, ItemType);
                     false -> ItemType
                 end,
    Base = #{<<"id">> => maps:get(id, Entry, Unique),
      <<"item_type">> => ItemType,
      <<"name">> => Name,
      <<"name_source">> => NameSource,
      <<"market_name">> => inventory_market_name(Name, Group, Catalog, Tradable),
      <<"group">> => Group,
      <<"category">> => Category,
      <<"type">> => maps:get(<<"type">>, Catalog,
                              maps:get(<<"parentType">>, Catalog, <<>>)),
      <<"quantity">> => maps:get(count, Entry),
      <<"xp">> => maps:get(xp, Entry),
      <<"mastered">> => maps:get(MasteryKey, Mastery, 0) > 0,
      <<"is_prime">> => maps:get(<<"isPrime">>, Catalog, false) =:= true orelse
                        maps:get(<<"parentIsPrime">>, Catalog, false) =:= true,
      <<"vaulted">> => optional_or(maps:get(<<"vaulted">>, Catalog, undefined),
                                    maps:get(<<"parentVaulted">>, Catalog,
                                             undefined)),
      <<"tradable">> => Tradable,
      <<"ducats">> => number(maps:get(<<"primeSellingPrice">>, Catalog, 0)),
      <<"asset">> => asset(ItemType,
                            maps:get(<<"imageName">>, Catalog, undefined))},
    inventory_variant_fields(Base, Entry, Catalog).

inventory_variant_fields(Item, Entry, Catalog) ->
    case maps:is_key(equipped, Entry) of
        false -> Item;
        true ->
            Variant = Item#{<<"equipped">> => maps:get(equipped, Entry, false),
                            <<"equipped_in">> => maps:get(equipped_in, Entry, [])},
            case maps:get(rank, Entry, undefined) of
                Rank when is_integer(Rank), Rank >= 0 ->
                    Variant#{<<"rank">> => Rank,
                             <<"max_rank">> => number(maps:get(<<"fusionLimit">>,
                                                                Catalog, 0))};
                _ -> Variant
            end
    end.

inventory_group(Unique, Catalog, Category, Collections) ->
    Component = maps:get(<<"component">>, Catalog, false) =:= true,
    EquipmentPart = Component andalso equipment_category(Category),
    Projection = binary:match(Unique, <<"/Projections/">>) =/= nomatch,
    Arcane = binary:match(Unique, <<"/Arcane">>) =/= nomatch,
    Upgrade = lists:member(<<"RawUpgrades">>, Collections) orelse
              lists:member(<<"Upgrades">>, Collections),
    case {Category, EquipmentPart, Projection, Arcane, Upgrade} of
        {<<"Relics">>, _, _, _, _} -> <<"relics">>;
        {<<"Mods">>, _, _, _, _} -> <<"mods">>;
        {<<"Arcanes">>, _, _, _, _} -> <<"arcanes">>;
        {_, true, _, _, _} -> <<"parts">>;
        {_, _, true, _, _} -> <<"relics">>;
        {_, _, _, true, _} -> <<"arcanes">>;
        {_, _, _, _, true} -> <<"mods">>;
        _ -> <<"misc">>
    end.

inventory_summary(Items) ->
    Counts = lists:foldl(
      fun(Item, Acc) ->
          Group = maps:get(<<"group">>, Item),
          Acc#{Group => maps:get(Group, Acc, 0) + 1}
      end, #{}, Items),
    Counts#{<<"total">> => length(Items)}.

inventory_set(Item, Stacks, Mastery) ->
    Category = maps:get(<<"category">>, Item, <<>>),
    Components = maps:get(<<"components">>, Item, []),
    Tradable = [Component || Component <- Components,
                             maps:get(<<"tradable">>, Component, false) =:= true],
    OwnedComponents = [inventory_set_component(Component, Stacks)
                       || Component <- Tradable],
    case equipment_category(Category) andalso length(Tradable) >= 2 andalso
         lists:any(fun(Component) -> maps:get(<<"owned">>, Component) > 0 end,
                   OwnedComponents) of
        false -> undefined;
        true ->
            Unique = maps:get(<<"uniqueName">>, Item),
            {ItemName, NameSource} = catalog_item_name(Unique, Item),
            Quantity = lists:min(
                         [maps:get(<<"owned">>, Component) div
                          maps:get(<<"required">>, Component)
                          || Component <- OwnedComponents]),
            Ducats = lists:sum(
                       [maps:get(<<"required">>, Component) *
                        maps:get(<<"ducats">>, Component)
                        || Component <- OwnedComponents]),
            #{<<"id">> => <<"set:", Unique/binary>>,
              <<"item_type">> => Unique,
              <<"name">> => <<ItemName/binary, " Set">>,
              <<"name_source">> => NameSource,
              <<"market_name">> => <<ItemName/binary, " Set">>,
              <<"group">> => <<"sets">>,
              <<"category">> => Category,
              <<"type">> => <<"Set">>,
              <<"quantity">> => Quantity,
              <<"xp">> => maps:get(Unique, Mastery, 0),
              <<"mastered">> => maps:get(Unique, Mastery, 0) > 0,
              <<"is_prime">> => maps:get(<<"isPrime">>, Item, false) =:= true,
              <<"vaulted">> => optional_bool(maps:get(<<"vaulted">>, Item,
                                                       undefined)),
              <<"tradable">> => true,
              <<"ducats">> => Ducats,
              <<"components">> => OwnedComponents,
              <<"asset">> => asset(Unique,
                                     maps:get(<<"imageName">>, Item, undefined))}
    end.

inventory_set_component(Component, Stacks) ->
    Unique = maps:get(<<"uniqueName">>, Component),
    {Name, NameSource} = catalog_item_name(Unique, Component),
    #{<<"id">> => Unique,
      <<"name">> => Name,
      <<"name_source">> => NameSource,
      <<"required">> => max(1, number(maps:get(<<"itemCount">>, Component, 1))),
      <<"owned">> => maps:get(count, maps:get(Unique, Stacks, #{}), 0),
      <<"ducats">> => number(maps:get(<<"primeSellingPrice">>, Component, 0)),
      <<"asset">> => asset(Unique, maps:get(<<"imageName">>, Component, undefined))}.

inventory_market_name(_Name, _Group, _Catalog, false) -> null;
inventory_market_name(Name, <<"relics">>, _Catalog, true) ->
    case binary:split(Name, <<" ">>, [global, trim_all]) of
        [Era, Code | _] -> <<Era/binary, " ", Code/binary, " Relic">>;
        _ -> Name
    end;
inventory_market_name(_Name, _Group, #{<<"component">> := true} = Catalog, true) ->
    Parent = #{<<"name">> => maps:get(<<"parentName">>, Catalog, <<>>),
               <<"category">> => maps:get(<<"parentCategory">>, Catalog, <<>>)},
    component_market_name(Catalog, component_external_name(Parent, Catalog));
inventory_market_name(Name, _Group, _Catalog, true) -> Name.

equipment_category(Category) ->
    lists:member(Category,
                 [<<"Warframes">>, <<"Archwing">>, <<"Primary">>,
                  <<"Secondary">>, <<"Melee">>, <<"Arch-Gun">>,
                  <<"Arch-Melee">>, <<"Sentinels">>]).

mastery_item_supported(Item) when is_map(Item) ->
    maps:get(<<"masterable">>, Item, false) =:= true andalso
    lists:member(maps:get(<<"category">>, Item, <<>>),
                 [<<"Warframes">>, <<"Archwing">>, <<"Primary">>, <<"Secondary">>,
                  <<"Melee">>, <<"Arch-Gun">>, <<"Arch-Melee">>, <<"Pets">>,
                  <<"Sentinels">>, <<"Misc">>]);
mastery_item_supported(_Item) -> false.

mastery_item(Item, Owned, Mastery, Pending) ->
    Unique = maps:get(<<"uniqueName">>, Item),
    Category = maps:get(<<"category">>, Item, <<>>),
    Type = maps:get(<<"type">>, Item, <<>>),
    {Name, NameSource} = catalog_item_name(Unique, Item),
    Xp = max(maps:get(Unique, Mastery, 0),
             maps:get(xp, maps:get(Unique, Owned, #{}), 0)),
    Progress = wfcli_player_mastery:progress(Item, Xp),
    MaxRank = maps:get(max_rank, Progress),
    Rank = maps:get(rank, Progress),
    MasteryPerRank = maps:get(mastery_per_rank, Progress),
    Components = mastery_components(Item, Owned),
    Missing = [Component || Component <- Components,
                            maps:get(<<"owned">>, Component) <
                            maps:get(<<"required">>, Component)],
    RelicProbability = item_relic_probability(Missing),
    OwnedCount = maps:get(count, maps:get(Unique, Owned, #{}), 0),
    #{<<"id">> => Unique,
      <<"name">> => Name,
      <<"name_source">> => NameSource,
      <<"group">> => mastery_group(Category, Type),
      <<"category">> => Category,
      <<"type">> => Type,
      <<"is_prime">> => maps:get(<<"isPrime">>, Item, false) =:= true,
      <<"vaulted">> => maps:get(<<"vaulted">>, Item, false) =:= true,
      <<"mastery_requirement">> => number(maps:get(<<"masteryReq">>, Item, 0)),
      <<"owned">> => OwnedCount > 0,
      <<"pending">> => maps:get(Unique, Pending, false),
      <<"rank">> => Rank,
      <<"max_rank">> => MaxRank,
      <<"mastered">> => Rank >= MaxRank,
      <<"earned_xp">> => Rank * MasteryPerRank,
      <<"potential_xp">> => (MaxRank - Rank) * MasteryPerRank,
      <<"has_recipe">> => Components =/= [],
      <<"missing_parts">> => length(Missing),
      <<"relic_probability">> => RelicProbability,
      <<"from_relics">> => RelicProbability > 0,
      <<"buyable">> => buyable_candidate(Missing),
      <<"components">> => Components,
      <<"asset">> => asset(Unique, maps:get(<<"imageName">>, Item, undefined))}.

foundry_item(Item, Owned, Mastery, Pending, Subsumed) ->
    Base = mastery_item(Item, Owned, Mastery, Pending),
    Unique = maps:get(<<"id">>, Base),
    Components = maps:get(<<"components">>, Base),
    Required = lists:sum([maps:get(<<"required">>, Component)
                          || Component <- Components]),
    Available = lists:sum([min(maps:get(<<"owned">>, Component),
                               maps:get(<<"required">>, Component))
                           || Component <- Components]),
    Ready = Components =/= [] andalso Available >= Required,
    Base#{<<"group">> => foundry_group(maps:get(<<"category">>, Base),
                                        maps:get(<<"type">>, Base)),
          <<"subsumed">> => maps:get(Unique, Subsumed, false),
          <<"ready_to_build">> => Ready,
          <<"components_owned">> => Available,
          <<"components_required">> => Required}.

mastery_components(Item, Owned) ->
    Components = [Component || Component <- maps:get(<<"components">>, Item, []),
                               is_map(Component),
                               is_binary(maps:get(<<"uniqueName">>, Component,
                                                  undefined))],
    {Result, _Remaining} =
        lists:mapfoldl(
          fun(Component, Remaining) ->
              Unique = maps:get(<<"uniqueName">>, Component),
              Required = max(1, number(maps:get(<<"itemCount">>, Component, 1))),
              TotalOwned = maps:get(count, maps:get(Unique, Owned, #{}), 0),
              Available = maps:get(Unique, Remaining, TotalOwned),
              DisplayOwned = case Available >= Required of
                                 true -> TotalOwned;
                                 false -> Available
                             end,
              {mastery_component(Item, Component, Required, DisplayOwned, Owned),
               Remaining#{Unique => max(0, Available - Required)}}
          end, #{}, Components),
    Result.

mastery_component(Item, Component, Required, OwnedCount, Owned) ->
    Unique = maps:get(<<"uniqueName">>, Component),
    RecipeIds = component_recipe_ids(Component),
    {Name, NameSource} = catalog_item_name(Unique, Component),
    ExternalName = component_external_name(Item, Component),
    MarketName = component_market_name(Component, ExternalName),
    #{<<"id">> => Unique,
      <<"name">> => Name,
      <<"name_source">> => NameSource,
      <<"market_name">> => MarketName,
      <<"market_required">> =>
          contains(ExternalName, <<"Blueprint">>) andalso
          not zero_price_component(ExternalName),
      <<"required">> => Required,
      <<"owned">> => OwnedCount,
      <<"blueprint_owned">> => component_blueprint_count(RecipeIds, Owned),
      <<"recipe_ids">> => RecipeIds,
      <<"tradable">> => maps:get(<<"tradable">>, Component, false) =:= true,
      <<"relic_drop">> => maps:get(<<"drops">>, Component, []) =/= [],
      <<"owned_relic">> => has_owned_relic(maps:get(<<"drops">>, Component, []), Owned),
      <<"relic_probability">> =>
          component_relic_probability(maps:get(<<"drops">>, Component, []), Owned),
      <<"asset">> => asset(Unique, maps:get(<<"imageName">>, Component, undefined))}.

component_recipe_ids(Component) ->
    [Identity || Identity <- maps:get(<<"recipeAliases">>, Component, []),
                 is_binary(Identity), byte_size(Identity) > 0].

component_blueprint_count(RecipeIds, Owned) ->
    lists:sum([maps:get(count, maps:get(Identity, Owned, #{}), 0)
               || Identity <- RecipeIds]).

buyable_candidate([]) -> false;
buyable_candidate(Components) ->
    lists:any(fun(Component) -> present(maps:get(<<"market_name">>, Component, null)) end,
              Components) andalso
    lists:all(
      fun(Component) ->
          not maps:get(<<"market_required">>, Component, false) orelse
          present(maps:get(<<"market_name">>, Component, null))
      end, Components).

component_market_name(Component, ExternalName) ->
    case maps:get(<<"tradable">>, Component, false) =:= true of
        false -> null;
        true ->
            Unique = maps:get(<<"uniqueName">>, Component, <<>>),
            case standalone_component(Unique) andalso
                 not ends_with(ExternalName, <<" Set">>) of
                true -> <<ExternalName/binary, " Set">>;
                false -> ExternalName
            end
    end.

component_external_name(Item, Component) ->
    Name = maps:get(<<"name">>, Component,
                    fallback_name(maps:get(<<"uniqueName">>, Component, <<>>))),
    Unique = maps:get(<<"uniqueName">>, Component, <<>>),
    Parent = maps:get(<<"name">>, Item, <<>>),
    Category = maps:get(<<"category">>, Item, <<>>),
    case wfcli_builtin_metadata:component_name(Unique, Name) of
        {Override, <<"builtin">>} -> Override;
        {_Default, _Source} ->
            case contains(Name, <<"Kavasa Prime">>) orelse
                 resource_component(Unique) orelse standalone_component(Unique) of
                true -> Name;
                false -> maybe_blueprint(join_component_name(Parent, Name), Category)
            end
    end.

zero_price_component(Name) ->
    Lower = string:lowercase(Name),
    contains(Lower, <<"forma">>) orelse contains(Lower, <<"orokin">>).

join_component_name(<<>>, Name) -> Name;
join_component_name(Parent, Name) ->
    Prefix = <<Parent/binary, " ">>,
    case Name of
        <<Prefix:(byte_size(Prefix))/binary, _/binary>> -> Name;
        _ -> <<Parent/binary, " ", Name/binary>>
    end.

maybe_blueprint(Name, Category)
  when Category =:= <<"Warframes">>; Category =:= <<"Archwing">> ->
    case ends_with(Name, <<"Blueprint">>) of
        true -> Name;
        false -> <<Name/binary, " Blueprint">>
    end;
maybe_blueprint(Name, _Category) -> Name.

resource_component(Unique) ->
    contains(Unique, <<"/Resources/">>) orelse
    contains(Unique, <<"/Resource/">>) orelse
    contains(Unique, <<"/Types/Items/">>).

standalone_component(Unique) ->
    starts_with(Unique, <<"/Lotus/Weapons/">>) orelse
    starts_with(Unique, <<"/Lotus/Powersuits/">>).

contains(Value, Part) when is_binary(Value), is_binary(Part) ->
    binary:match(Value, Part) =/= nomatch;
contains(_Value, _Part) -> false.

starts_with(Value, Prefix) when is_binary(Value), is_binary(Prefix),
                                byte_size(Value) >= byte_size(Prefix) ->
    binary:part(Value, 0, byte_size(Prefix)) =:= Prefix;
starts_with(_Value, _Prefix) -> false.

ends_with(Value, Suffix) when is_binary(Value), is_binary(Suffix),
                              byte_size(Value) >= byte_size(Suffix) ->
    binary:part(Value, byte_size(Value) - byte_size(Suffix), byte_size(Suffix))
        =:= Suffix;
ends_with(_Value, _Suffix) -> false.

has_owned_relic(Drops, Owned) ->
    lists:any(
      fun(Drop) ->
          case drop_relic_unique(Drop) of
              Unique when is_binary(Unique) ->
                  maps:get(count, maps:get(Unique, Owned, #{}), 0) > 0;
              _ -> false
          end
      end, Drops).

component_relic_probability(Drops, Owned) ->
    Relics = lists:foldl(fun relic_drop_chance/2, #{}, Drops),
    1.0 - maps:fold(
      fun(Unique, Chance, MissChance) ->
          Count = maps:get(count, maps:get(Unique, Owned, #{}), 0),
          MissChance * math:pow(1.0 - Chance, Count)
      end, 1.0, Relics).

relic_drop_chance(Drop, Acc) ->
    case drop_relic_unique(Drop) of
        Unique when is_binary(Unique) ->
            Chance = min(100, max(0, number(maps:get(<<"chance">>, Drop, 0)))) / 100,
            maps:update_with(Unique, fun(Previous) -> max(Previous, Chance) end,
                             Chance, Acc);
        _ -> Acc
    end.

drop_relic_unique(Drop) ->
    case maps:get(<<"uniqueName">>, Drop, undefined) of
        Unique when is_binary(Unique) ->
            replace_relic_refinement(
              Unique, relic_refinement_suffix(maps:get(<<"location">>, Drop, <<>>)));
        _ -> undefined
    end.

relic_refinement_suffix(Location) ->
    case {contains(Location, <<"(Exceptional)">>),
          contains(Location, <<"(Flawless)">>),
          contains(Location, <<"(Radiant)">>)} of
        {true, _, _} -> <<"Silver">>;
        {_, true, _} -> <<"Gold">>;
        {_, _, true} -> <<"Platinum">>;
        _ -> <<"Bronze">>
    end.

replace_relic_refinement(Unique, Refinement) ->
    Existing = [Suffix || Suffix <- [<<"Bronze">>, <<"Silver">>, <<"Gold">>,
                                          <<"Platinum">>],
                         ends_with(Unique, Suffix)],
    case Existing of
        [Suffix | _] ->
            PrefixSize = byte_size(Unique) - byte_size(Suffix),
            <<Prefix:PrefixSize/binary, _/binary>> = Unique,
            <<Prefix/binary, Refinement/binary>>;
        [] -> Unique
    end.

item_relic_probability([]) -> 0.0;
item_relic_probability(Components) ->
    lists:foldl(
      fun(Component, Chance) ->
          Chance * maps:get(<<"relic_probability">>, Component, 0.0)
      end, 1.0, Components).

foundry_summary(Items) ->
    Groups = lists:foldl(
      fun(Item, Acc) ->
          Group = maps:get(<<"group">>, Item),
          Acc#{Group => maps:get(Group, Acc, 0) + 1}
      end, #{}, Items),
    #{<<"total">> => length(Items),
      <<"owned">> => count_true(<<"owned">>, Items),
      <<"mastered">> => count_true(<<"mastered">>, Items),
      <<"pending">> => count_true(<<"pending">>, Items),
      <<"ready">> => count_true(<<"ready_to_build">>, Items),
      <<"groups">> => Groups}.

mastery_summary(Items, Profile, Index, StarChartMetadata) ->
    Groups = [<<"warframes">>, <<"weapons">>, <<"companions">>],
    GroupSummary = maps:from_list(
      [{Group, category_summary(Group, Items)} || Group <- Groups]),
    Total = length(Items),
    Mastered = length([ok || Item <- Items, maps:get(<<"mastered">>, Item)]),
    Intrinsics = intrinsic_summary(maps:get(<<"player_skills">>, Index, #{})),
    StarChart = star_chart_summary(maps:get(<<"missions">>, Index, []),
                                   StarChartMetadata),
    Level = player_level(Profile),
    RankProgress = mastery_progress(Level, Items, Index, StarChart, Intrinsics),
    GroupSummary#{
        <<"total">> => Total,
        <<"mastered">> => Mastered,
        <<"content_percent">> => percent(Mastered, Total),
        <<"player_name">> => nullable(maps:get(<<"player_name">>, Profile, undefined)),
        <<"player_level">> => nullable(Level),
        <<"rank_progress">> => RankProgress,
        <<"intrinsics">> => Intrinsics,
        <<"star_chart">> => StarChart
    }.

player_level(Profile) ->
    case maps:get(<<"player_level">>, Profile, undefined) of
        Level when is_integer(Level), Level >= 0 -> Level;
        Level when is_float(Level), Level >= 0 -> trunc(Level);
        _ -> undefined
    end.

mastery_progress(Level, Items, Index, StarChart, Intrinsics)
  when is_integer(Level) ->
    case {maps:get(<<"xp">>, StarChart, null),
          maps:get(<<"xp">>, Intrinsics, null)} of
        {StarXp, IntrinsicXp} when is_integer(StarXp), is_integer(IntrinsicXp) ->
            Known = lists:sum([maps:get(<<"earned_xp">>, Item, 0) || Item <- Items]),
            Extra = extra_mastery_xp(maps:get(<<"mastery">>, Index, []), Items),
            TotalXp = Known + Extra + StarXp + IntrinsicXp,
            Start = mastery_threshold(Level),
            Required = mastery_threshold(Level + 1) - Start,
            Current = max(0, TotalXp - Start),
            Available = TotalXp >= Start,
            #{<<"available">> => Available,
              <<"current">> => case Available of true -> Current; false -> null end,
              <<"total">> => case Available of true -> Required; false -> null end,
              <<"percent">> => case Available of
                  true -> min(100, percent(Current, Required));
                  false -> null
              end,
              <<"total_xp">> => TotalXp};
        _ -> unavailable_mastery_progress()
    end;
mastery_progress(_Level, _Items, _Index, _StarChart, _Intrinsics) ->
    unavailable_mastery_progress().

unavailable_mastery_progress() ->
    #{<<"available">> => false, <<"current">> => null, <<"total">> => null,
      <<"percent">> => null, <<"total_xp">> => null}.

extra_mastery_xp(Records, Items) when is_list(Records) ->
    Known = maps:from_keys([maps:get(<<"id">>, Item) || Item <- Items], true),
    lists:sum(
      [extra_mastery_record_xp(Record)
       || Record <- Records,
          is_map(Record),
          not maps:is_key(maps:get(<<"item_type">>, Record, undefined), Known)]);
extra_mastery_xp(_Records, _Items) -> 0.

extra_mastery_record_xp(Record) ->
    Unique = maps:get(<<"item_type">>, Record, <<>>),
    PerRank = case mastery_heavy_item(Unique) of true -> 200; false -> 100 end,
    wfcli_player_mastery:rank(
       number(maps:get(<<"xp">>, Record, 0)), PerRank, 30) * PerRank.

mastery_heavy_item(Unique) when is_binary(Unique) ->
    binary:match(Unique, <<"/Powersuits/">>) =/= nomatch orelse
    binary:match(Unique, <<"/Sentinel/">>) =/= nomatch orelse
    Unique =:= <<"/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness">>;
mastery_heavy_item(_Unique) -> false.

mastery_threshold(Level) when Level =< 0 -> 0;
mastery_threshold(Level) when Level =< 30 -> 2500 * Level * Level;
mastery_threshold(Level) -> 2250000 + 147500 * (Level - 30).

intrinsic_summary(Skills) when is_map(Skills) ->
    Railjack = sum_keys(Skills, [<<"LPS_TACTICAL">>, <<"LPS_PILOTING">>,
                                  <<"LPS_GUNNERY">>, <<"LPS_ENGINEERING">>,
                                  <<"LPS_COMMAND">>]),
    Duviri = sum_keys(Skills, [<<"LPS_DRIFT_COMBAT">>,
                                <<"LPS_DRIFT_OPPORTUNITY">>,
                                <<"LPS_DRIFT_RIDING">>,
                                <<"LPS_DRIFT_ENDURANCE">>]),
    #{<<"railjack">> => progress(Railjack, 50),
      <<"duviri">> => progress(Duviri, 40),
      <<"percent">> => percent(Railjack + Duviri, 90),
      <<"xp">> => (Railjack + Duviri) * 1500};
intrinsic_summary(_Skills) ->
    #{<<"railjack">> => progress(0, 50),
      <<"duviri">> => progress(0, 40),
      <<"percent">> => 0,
      <<"xp">> => 0}.

star_chart_summary(Missions, #{nodes := Nodes, junctions := JunctionIndex})
  when is_list(Missions), is_map(Nodes), is_map(JunctionIndex) ->
    Normal = length([ok || Mission <- Missions,
                           completed_mission(Mission, normal, Nodes)]),
    Steel = length([ok || Mission <- Missions,
                          completed_mission(Mission, steel, Nodes)]),
    JunctionCount = length([ok || Mission <- Missions,
                                  completed_junction(Mission, normal,
                                                     JunctionIndex)]),
    SteelJunctions = length([ok || Mission <- Missions,
                                   completed_junction(Mission, steel,
                                                      JunctionIndex)]),
    NodeTotal = map_size(Nodes),
    JunctionTotal = map_size(JunctionIndex),
    Total = NodeTotal * 2 + JunctionTotal * 2,
    Current = Normal + Steel + JunctionCount + SteelJunctions,
    Xp = completed_node_xp(Missions, normal, Nodes) +
         completed_node_xp(Missions, steel, Nodes) +
         (JunctionCount + SteelJunctions) * 1000,
    #{<<"normal">> => progress(Normal, NodeTotal),
      <<"steel">> => progress(Steel, NodeTotal),
      <<"junctions">> => progress(JunctionCount, JunctionTotal),
      <<"steel_junctions">> => progress(SteelJunctions, JunctionTotal),
      <<"percent">> => percent(Current, Total),
      <<"xp">> => Xp};
star_chart_summary(Missions, _Metadata) when is_list(Missions) ->
    Normal = length([ok || Mission <- Missions,
                           completed_mission(Mission, normal)]),
    Steel = length([ok || Mission <- Missions,
                          completed_mission(Mission, steel)]),
    Junctions = length([ok || Mission <- Missions,
                              completed_junction(Mission, normal)]),
    SteelJunctions = length([ok || Mission <- Missions,
                                   completed_junction(Mission, steel)]),
    #{<<"normal">> => observed_progress(Normal),
      <<"steel">> => observed_progress(Steel),
      <<"junctions">> => observed_progress(Junctions),
      <<"steel_junctions">> => observed_progress(SteelJunctions),
      <<"percent">> => null,
      <<"xp">> => null};
star_chart_summary(_Missions, Metadata) -> star_chart_summary([], Metadata).

completed_node_xp(Missions, Mode, Nodes) ->
    lists:sum([maps:get(mission_tag(Mission), Nodes)
               || Mission <- Missions,
                  completed_mission(Mission, Mode, Nodes)]).

completed_mission(Mission, Mode, Nodes) ->
    Tag = mission_tag(Mission),
    maps:is_key(Tag, Nodes) andalso completed_mission(Mission, Mode).

completed_mission(Mission, Mode) when is_map(Mission) ->
    Tag = mission_tag(Mission),
    not is_junction(Tag) andalso
        mission_number(Mission, <<"completes">>, <<"Completes">>) > 0
        andalso (Mode =:= normal orelse mission_number(Mission, <<"tier">>, <<"Tier">>) > 0);
completed_mission(_Mission, _Mode) -> false.

completed_junction(Mission, Mode, Junctions) ->
    Tag = mission_tag(Mission),
    maps:is_key(Tag, Junctions) andalso completed_junction(Mission, Mode).

completed_junction(Mission, Mode) when is_map(Mission) ->
    Tag = mission_tag(Mission),
    Completes = mission_number(Mission, <<"completes">>, <<"Completes">>),
    Tier = mission_number(Mission, <<"tier">>, <<"Tier">>),
    is_junction(Tag) andalso binary:match(Tag, <<"To">>) =/= nomatch andalso
        case Mode of
            normal -> Completes > 0;
            steel -> Completes =:= 2 orelse (Completes >= 1 andalso Tier >= 1)
        end;
completed_junction(_Mission, _Mode) -> false.

mission_tag(Mission) ->
    maps:get(<<"tag">>, Mission, maps:get(<<"Tag">>, Mission, <<>>)).

mission_number(Mission, Key, LegacyKey) ->
    number(maps:get(Key, Mission, maps:get(LegacyKey, Mission, 0))).

is_junction(Tag) when is_binary(Tag) ->
    Suffix = <<"Junction">>,
    byte_size(Tag) >= byte_size(Suffix) andalso
        binary:part(Tag, byte_size(Tag) - byte_size(Suffix), byte_size(Suffix)) =:= Suffix;
is_junction(_Tag) -> false.

observed_progress(Current) -> #{<<"current">> => Current, <<"total">> => null}.

progress(Current, Total) ->
    #{<<"current">> => Current, <<"total">> => Total,
      <<"percent">> => percent(Current, Total)}.

sum_keys(Map, Keys) ->
    lists:sum([number(maps:get(Key, Map, 0)) || Key <- Keys]).

count_true(Key, Items) ->
    length([ok || Item <- Items, maps:get(Key, Item, false) =:= true]).

category_summary(Group, Items) ->
    Matching = [Item || Item <- Items, maps:get(<<"group">>, Item) =:= Group],
    Total = length(Matching),
    Mastered = length([ok || Item <- Matching, maps:get(<<"mastered">>, Item)]),
    #{<<"total">> => Total,
      <<"mastered">> => Mastered,
      <<"percent">> => percent(Mastered, Total)}.

mastery_group(Category, _Type) when Category =:= <<"Warframes">>;
                                    Category =:= <<"Archwing">> -> <<"warframes">>;
mastery_group(Category, _Type) when Category =:= <<"Pets">>;
                                    Category =:= <<"Sentinels">> -> <<"companions">>;
mastery_group(_Category, _Type) -> <<"weapons">>.

foundry_group(<<"Warframes">>, _Type) -> <<"warframe">>;
foundry_group(<<"Primary">>, _Type) -> <<"primary">>;
foundry_group(<<"Secondary">>, _Type) -> <<"secondary">>;
foundry_group(<<"Melee">>, _Type) -> <<"melee">>;
foundry_group(Category, _Type) when Category =:= <<"Archwing">>;
                                          Category =:= <<"Arch-Gun">>;
                                          Category =:= <<"Arch-Melee">> -> <<"arch">>;
foundry_group(Category, _Type) when Category =:= <<"Pets">>;
                                          Category =:= <<"Sentinels">> -> <<"companion">>;
foundry_group(<<"Misc">>, _Type) -> <<"modular">>;
foundry_group(_Category, _Type) -> <<"other">>.

mastery_before(A, B) ->
    {maps:get(<<"mastered">>, A), acquisition_unknown(A),
     not maps:get(<<"owned">>, A),
     maps:get(<<"missing_parts">>, A), -maps:get(<<"potential_xp">>, A),
     string:lowercase(maps:get(<<"name">>, A))} <
    {maps:get(<<"mastered">>, B), acquisition_unknown(B),
     not maps:get(<<"owned">>, B),
     maps:get(<<"missing_parts">>, B), -maps:get(<<"potential_xp">>, B),
     string:lowercase(maps:get(<<"name">>, B))}.

acquisition_unknown(Item) ->
    not maps:get(<<"owned">>, Item) andalso
    not maps:get(<<"pending">>, Item) andalso
    maps:get(<<"components">>, Item) =:= [].

name_before(A, B) ->
    string:lowercase(maps:get(<<"name">>, A)) <
    string:lowercase(maps:get(<<"name">>, B)).

item_name(Unique, Entry, Catalog) ->
    case {maps:get(<<"component">>, Catalog, false),
          maps:get(<<"parentName">>, Catalog, undefined),
          maps:get(<<"name">>, Catalog, undefined)} of
        {true, Parent, Component} when is_binary(Parent), byte_size(Parent) > 0,
                                       is_binary(Component), byte_size(Component) > 0 ->
            {<<Parent/binary, " ", Component/binary>>, <<"wfcd">>};
        _ ->
            first_present_with_source(
              [{maps:get(<<"name">>, Catalog, undefined), <<"wfcd">>},
               {maps:get(name, Entry, undefined), <<"player">>},
               {fallback_name(Unique), <<"path">>}])
    end.

catalog_item_name(Unique, Catalog) ->
    first_present_with_source(
      [{maps:get(<<"name">>, Catalog, undefined), <<"wfcd">>},
       {fallback_name(Unique), <<"path">>}]).

fallback_name(Unique) when is_binary(Unique) ->
    case binary:split(Unique, <<"/">>, [global, trim_all]) of
        [] -> Unique;
        Parts -> lists:last(Parts)
    end.

asset(_Unique, undefined) -> null;
asset(_Unique, null) -> null;
asset(Unique, ImageName) when is_binary(ImageName), byte_size(ImageName) > 0 ->
    #{<<"id">> => <<"wfcd-item:", Unique/binary>>,
      <<"source">> => <<"wfcd">>, <<"image_name">> => ImageName};
asset(_Unique, _ImageName) -> null.

first_present([]) -> undefined;
first_present([Value | Rest]) ->
    case present(Value) of true -> Value; false -> first_present(Rest) end.

first_present_with_source([{Value, Source} | Rest]) ->
    case present(Value) of
        true -> {Value, Source};
        false -> first_present_with_source(Rest)
    end.

present(undefined) -> false;
present(null) -> false;
present(<<>>) -> false;
present([]) -> false;
present(_) -> true.

number(Value) when is_integer(Value) -> Value;
number(Value) when is_float(Value) -> trunc(Value);
number(_) -> 0.

list_value(Value) when is_list(Value) -> Value;
list_value(_) -> [].

percent(_Current, 0) -> 0;
percent(Current, Total) -> round(Current * 100 / Total).

nullable(undefined) -> null;
nullable(Value) -> Value.

optional_or(true, _) -> true;
optional_or(_, true) -> true;
optional_or(false, _) -> false;
optional_or(_, false) -> false;
optional_or(_, _) -> null.

optional_bool(true) -> true;
optional_bool(false) -> false;
optional_bool(_) -> null.

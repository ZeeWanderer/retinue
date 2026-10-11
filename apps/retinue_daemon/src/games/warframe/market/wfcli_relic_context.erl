%%%-------------------------------------------------------------------
%% Relic reward metadata joined from Market and the local player snapshot.
%%%-------------------------------------------------------------------
-module(wfcli_relic_context).

-export([build/7]).

-spec build([binary()], [map()], map(), map(), map(), map(), [map()]) -> map().
build(Slugs, Items, Details, Quotes, PlayerSnapshot, Relics, Catalog) ->
    ById = maps:from_list([{maps:get(<<"id">>, Item), Item}
                           || Item <- Items, maps:is_key(<<"id">>, Item)]),
    BySlug = maps:from_list([{maps:get(<<"slug">>, Item), Item}
                             || Item <- Items, maps:is_key(<<"slug">>, Item)]),
    CatalogByRef = maps:from_list([{maps:get(<<"uniqueName">>, Item), Item}
                                   || Item <- Catalog,
                                      maps:is_key(<<"uniqueName">>, Item)]),
    Player = player_index(PlayerSnapshot),
    Available = available_reward_slugs(Relics),
    Contexts = [context(Slug, BySlug, ById, Details, Quotes,
                        Player, CatalogByRef, Available)
                || Slug <- Slugs],
    Owned = maps:get(owned, Player),
    Account = maps:filter(fun(_Key, Value) -> Value =/= undefined end, #{
          <<"platinum">> => profile_value(PlayerSnapshot, <<"premium_credits">>),
          <<"ducats">> => maps:get(
              <<"/Lotus/Types/Items/MiscItems/PrimeBucks">>, Owned, 0)
      }),
    #{<<"items">> => Contexts, <<"account">> => Account}.

context(Slug, BySlug, ById, Details, Quotes, Player, CatalogByRef, Available) ->
    Detail = detail(Slug, Details),
    Item = maps:merge(maps:get(Slug, BySlug, #{}), Detail),
    PartIds = maps:get(<<"setParts">>, Detail, []),
    Parts0 = [with_detail(maps:get(Id, ById), Details)
              || Id <- PartIds, maps:is_key(Id, ById)],
    RootItem = first_root_item(Parts0),
    RootRef0 = maps:get(<<"gameRef">>, RootItem, undefined),
    RootCatalog = maps:get(RootRef0, CatalogByRef, #{}),
    Owned = maps:get(owned, Player),
    Parts = [part(Part, Owned, RootCatalog) || Part <- Parts0],
    Root = first_root(Parts),
    Current = part(Item, Owned, RootCatalog),
    GameRef = maps:get(<<"game_ref">>, Current, undefined),
    Required = maps:get(<<"required">>, Current, 1),
    Count = maps:get(GameRef, Owned, 0),
    RootRef = maps:get(<<"game_ref">>, Root, undefined),
    Quote = maps:get(Slug, Quotes, #{}),
    RootSlug = maps:get(<<"slug">>, Root, undefined),
    SetQuote = maps:get(RootSlug, Quotes, #{}),
    Base = Current#{
        <<"count_owned">> => Count,
        <<"total_to_own">> => Required,
        <<"crafted">> => crafted(RootRef, RootCatalog, Player),
        <<"set_complete">> => set_complete(Parts),
        <<"vaulted">> => vaulted(Item, Parts, RootCatalog, Available),
        <<"parts">> => Parts,
        <<"lowest_sell">> => maps:get(lowest_sell, Quote, undefined),
        <<"highest_buy">> => maps:get(highest_buy, Quote, undefined),
        <<"set_slug">> => RootSlug,
        <<"set_price">> => maps:get(lowest_sell, SetQuote, undefined)
    },
    maps:filter(fun(_Key, Value) -> Value =/= undefined end, Base).

part(Item, Owned, RootCatalog) when is_map(Item) ->
    Slug = maps:get(<<"slug">>, Item, undefined),
    GameRef = maps:get(<<"gameRef">>, Item, undefined),
    Required = positive(maps:get(<<"quantityInSet">>, Item,
                                 catalog_required(GameRef, RootCatalog)), 1),
    Base = #{
        <<"id">> => maps:get(<<"id">>, Item, undefined),
        <<"slug">> => Slug,
        <<"name">> => item_name(Item),
        <<"game_ref">> => GameRef,
        <<"ducats">> => maps:get(<<"ducats">>, Item, undefined),
        <<"set_root">> => is_set_root(Item),
        <<"required">> => Required,
        <<"owned">> => maps:get(GameRef, Owned, 0),
        <<"asset">> => asset(Slug, Item)
    },
    maps:filter(fun(_Key, Value) -> Value =/= undefined end, Base).

asset(undefined, _Item) -> undefined;
asset(Slug, Item) ->
    English = maps:get(<<"en">>, maps:get(<<"i18n">>, Item, #{}), #{}),
    case maps:get(<<"subIcon">>, English,
                  maps:get(<<"thumb">>, English, maps:get(<<"icon">>, English, undefined))) of
        Path when is_binary(Path) ->
            #{<<"id">> => <<"market:", Slug/binary>>,
              <<"source">> => <<"market">>, <<"image_name">> => Path};
        _ -> undefined
    end.

available_reward_slugs(Relics) when map_size(Relics) =:= 0 -> undefined;
available_reward_slugs(Relics) ->
    maps:fold(
      fun(_Id, #{vaulted := false, rewards := Rewards}, Available) ->
              lists:foldl(
                fun(#{slug := Slug}, Acc) when is_binary(Slug) -> Acc#{Slug => true};
                   (_Reward, Acc) -> Acc
                end,
                Available,
                Rewards);
         (_Id, _Relic, Available) -> Available
      end,
      #{},
      Relics).

vaulted(Item, Parts, RootCatalog, Available) ->
    case maps:find(<<"vaulted">>, RootCatalog) of
        {ok, Value} when is_boolean(Value) ->
            lists:member(<<"prime">>, maps:get(<<"tags">>, Item, [])) andalso
            Value;
        _ -> vaulted_from_relics(Item, Parts, Available)
    end.

vaulted_from_relics(_Item, _Parts, undefined) -> false;
vaulted_from_relics(Item, Parts, Available) ->
    Prime = lists:member(<<"prime">>, maps:get(<<"tags">>, Item, [])),
    CraftParts = [Part || Part <- Parts, not maps:get(<<"set_root">>, Part, false)],
    Prime andalso CraftParts =/= [] andalso
    lists:all(
      fun(Part) ->
          case maps:get(<<"slug">>, Part, undefined) of
              Slug when is_binary(Slug) -> not maps:is_key(Slug, Available);
              _ -> false
          end
      end,
      CraftParts).

first_root(Parts) ->
    case [Part || Part <- Parts, maps:get(<<"set_root">>, Part, false)] of
        [Root | _] -> Root;
        [] -> #{}
    end.

first_root_item(Parts) ->
    case [Part || Part <- Parts, is_set_root(Part)] of
        [Root | _] -> Root;
        [] -> #{}
    end.

is_set_root(Item) ->
    maps:get(<<"setRoot">>, Item, false) =:= true orelse
    lists:member(<<"set">>, maps:get(<<"tags">>, Item, [])).

detail(Slug, Details) ->
    maps:get(data, maps:get(Slug, Details, #{}), #{}).

with_detail(Item, Details) ->
    maps:merge(Item, detail(maps:get(<<"slug">>, Item, undefined), Details)).

catalog_required(undefined, _RootCatalog) -> 1;
catalog_required(GameRef, RootCatalog) ->
    Key = recipe_key(GameRef),
    Counts = [positive(maps:get(<<"itemCount">>, Component, 1), 1)
              || Component <- maps:get(<<"components">>, RootCatalog, []),
                 recipe_key(maps:get(<<"uniqueName">>, Component, <<>>)) =:= Key],
    case Counts of [] -> 1; _ -> lists:sum(Counts) end.

crafted(undefined, _RootCatalog, _Player) -> false;
crafted(RootRef, RootCatalog, Player) ->
    Owned = maps:get(owned, Player),
    Mastery = maps:get(mastery, Player),
    Pending = maps:get(pending, Player),
    RecipeRefs = [maps:get(<<"uniqueName">>, Component, undefined)
                  || Component <- maps:get(<<"components">>, RootCatalog, [])],
    maps:get(RootRef, Owned, 0) > 0 orelse
    (map_size(RootCatalog) > 0 andalso
     wfcli_player_mastery:mastered(
       RootCatalog, maps:get(RootRef, Mastery, 0))) orelse
    lists:any(fun(Ref) -> maps:is_key(recipe_key(Ref), Pending) end, RecipeRefs).

set_complete([]) -> false;
set_complete(Parts) ->
    CraftParts = [Part || Part <- Parts, not maps:get(<<"set_root">>, Part, false)],
    CraftParts =/= [] andalso
    lists:all(fun(Part) ->
        maps:get(<<"owned">>, Part, 0) >= maps:get(<<"required">>, Part, 1)
    end, CraftParts).

player_index(PlayerSnapshot) ->
    Player = wfcli_player_projection:build(PlayerSnapshot),
    Entries = maps:get(<<"equipment">>, Player, []) ++
              maps:get(<<"items">>, Player, []) ++
              maps:get(<<"stacks">>, Player, []),
    #{owned => lists:foldl(fun owned_entry/2, #{}, Entries),
      mastery => maps:from_list(
        [{maps:get(<<"item_type">>, Entry), maps:get(<<"xp">>, Entry, 0)}
         || Entry <- maps:get(<<"mastery">>, Player, []), is_map(Entry),
            is_binary(maps:get(<<"item_type">>, Entry, undefined)),
            is_integer(maps:get(<<"xp">>, Entry, undefined))]),
      pending => maps:from_list(
        [{recipe_key(maps:get(<<"item_type">>, Entry)), true}
         || Entry <- maps:get(<<"pending_recipes">>, Player, []), is_map(Entry),
            is_binary(maps:get(<<"item_type">>, Entry, undefined))])}.

owned_entry(#{<<"item_type">> := ItemType, <<"count">> := Count}, Acc)
  when is_binary(ItemType), is_integer(Count), Count > 0 ->
    Acc#{ItemType => maps:get(ItemType, Acc, 0) + Count};
owned_entry(_Entry, Acc) -> Acc.

profile_value(PlayerSnapshot, Key) ->
    Player = wfcli_player_projection:build(PlayerSnapshot),
    maps:get(Key, maps:get(<<"profile">>, Player, #{}), undefined).

item_name(Item) ->
    English = maps:get(<<"en">>, maps:get(<<"i18n">>, Item, #{}), #{}),
    maps:get(<<"name">>, English, maps:get(<<"slug">>, Item, undefined)).

positive(Value, _Default) when is_integer(Value), Value > 0 -> Value;
positive(_Value, Default) -> Default.

recipe_key(undefined) -> undefined;
recipe_key(Value) when is_binary(Value) ->
    strip_suffix(strip_suffix(Value, <<"Blueprint">>), <<"Component">>).

strip_suffix(Value, Suffix) ->
    ValueSize = byte_size(Value),
    SuffixSize = byte_size(Suffix),
    case ValueSize >= SuffixSize andalso
         binary:part(Value, ValueSize - SuffixSize, SuffixSize) =:= Suffix of
        true -> binary:part(Value, 0, ValueSize - SuffixSize);
        false -> Value
    end.

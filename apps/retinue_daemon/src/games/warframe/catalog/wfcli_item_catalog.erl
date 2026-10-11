%%%-------------------------------------------------------------------
%% Compact WFCD item graph used by player inventory and mastery views.
%%%-------------------------------------------------------------------
-module(wfcli_item_catalog).

-include_lib("kernel/include/file.hrl").

-export([load/0, source/0, recipe_source/0, store/2, invalidate/0,
         index/1, lookup/2]).
-ifdef(TEST).
-export([compact/1]).
-endif.

-define(ITEM_FILE, "WFCDItems.json").
-define(CACHE_KEY, {?MODULE, cache}).

-doc "Load compact item records, caching by file signature until next update.".
-spec load() -> {ok, [map()], map()} | {error, term()}.
load() ->
    Path = source(),
    RecipePath = recipe_source(),
    case {file:read_file_info(Path, [{time, posix}]),
          file:read_file_info(RecipePath, [{time, posix}])} of
        {{ok, #file_info{mtime = Modified, size = Size}},
         {ok, #file_info{mtime = RecipeModified, size = RecipeSize}}} ->
            Signature = {{Path, Modified, Size},
                         {RecipePath, RecipeModified, RecipeSize}},
            case persistent_term:get(?CACHE_KEY, undefined) of
                #{signature := Signature, entries := Entries, meta := Meta} ->
                    {ok, Entries, Meta};
                _ -> load_file(Path, RecipePath, Signature,
                               {RecipeModified, RecipeSize})
            end;
        {{error, Reason}, _} -> {error, {item_catalog_missing, Path, Reason}};
        {_, {error, Reason}} ->
            {error, {item_catalog_recipe_missing, RecipePath, Reason}}
    end.

-doc "Return preferred managed item-catalog path.".
-spec source() -> file:filename_all().
source() ->
    case application:get_env(retinue_daemon, item_catalog_file) of
        {ok, Path} when is_list(Path); is_binary(Path) -> Path;
        _ -> default_source()
    end.

-doc "Return managed official recipe-export path used for catalog aliases.".
-spec recipe_source() -> file:filename_all().
recipe_source() ->
    case application:get_env(retinue_daemon, item_recipe_file) of
        {ok, Path} when is_list(Path); is_binary(Path) -> Path;
        _ ->
            [{_, Path}] = wfcli_exports:item_sources(
                            undefined, ["ExportRecipes_en.json"]),
            Path
    end.

default_source() ->
    Paths = wfcli_worldstate:metadata_paths(?ITEM_FILE),
    case [Path || Path <- Paths, filelib:is_file(Path)] of
        [Path | _] -> Path;
        [] -> case Paths of
                  [Path | _] -> Path;
                  [] -> wfcli_paths:cache_file(?ITEM_FILE)
              end
    end.

-doc "Compact and store a validated WFCD snapshot for local player-data joins.".
-spec store([map()], map()) -> ok | {error, term()}.
store(Items, Meta) ->
    Entries = [Item || Value <- Items,
                      Item <- [compact(Value)], Item =/= undefined],
    Wrapper = Meta#{<<"entries">> => Entries},
    case wfcli_worldstate:write_metadata_file(?ITEM_FILE, jsone:encode(Wrapper)) of
        ok -> invalidate();
        Error -> Error
    end.

-doc "Discard cached catalog data after either upstream source changes.".
-spec invalidate() -> ok.
invalidate() ->
    persistent_term:erase(?CACHE_KEY),
    ok.

-doc "Index canonical items and nested components by game identity.".
-spec index([map()]) -> map().
index(Items) when is_list(Items) ->
    lists:foldl(
      fun(Item, Acc) when is_map(Item) ->
          Acc1 = put_index_entry(Item, Item, false, Acc),
          lists:foldl(
            fun(Component, Inner) -> put_index_entry(Component, Item, true, Inner) end,
            Acc1, maps:get(<<"components">>, Item, []));
         (_Item, Acc) -> Acc
      end, #{}, Items);
index(_Items) -> #{}.

-doc "Look up one canonical item or component in an indexed catalog.".
-spec lookup(binary(), map()) -> map() | undefined.
lookup(Identity, Index) when is_binary(Identity), is_map(Index) ->
    maps:get(Identity, Index, undefined);
lookup(_Identity, _Index) -> undefined.

load_file(Path, RecipePath, Signature, RecipeVersion) ->
    case file:read_file(Path) of
        {ok, Body} ->
            try jsone:decode(Body, [{object_format, map}]) of
                #{<<"entries">> := Entries0} = Wrapper when is_list(Entries0) ->
                    case recipe_aliases(RecipePath) of
                        {ok, Aliases} ->
                            Entries = attach_recipe_aliases(Entries0, Aliases),
                            Meta = #{source => maps:get(<<"source">>, Wrapper,
                                                       <<"WFCD">>),
                                     version => maps:get(<<"version">>, Wrapper,
                                                        <<"unknown">>),
                                     fetched_at => maps:get(<<"fetchedAt">>, Wrapper, 0),
                                     recipe_version => RecipeVersion},
                            persistent_term:put(
                              ?CACHE_KEY,
                              #{signature => Signature, entries => Entries,
                                meta => Meta}),
                            {ok, Entries, Meta};
                        {error, _Reason} = Error -> Error
                    end;
                _ -> {error, {bad_item_catalog, Path}}
            catch error:Reason -> {error, {bad_item_catalog_json, Path, Reason}}
            end;
        {error, Reason} -> {error, {item_catalog_read_failed, Path, Reason}}
    end.

recipe_aliases(Path) ->
    case wfcli_exports:load_items(filename:dirname(Path),
                                  [filename:basename(Path)]) of
        {ok, Recipes} ->
            {ok, lists:foldl(fun add_recipe_alias/2, #{}, Recipes)};
        {error, Reason} -> {error, {item_catalog_recipe_failed, Path, Reason}}
    end.

add_recipe_alias(Recipe, Acc) ->
    Alias = retinue_text:to_binary(maps:get(uniqueName, Recipe, <<>>)),
    Result = retinue_text:to_binary(maps:get(resultType, Recipe, <<>>)),
    case present(Alias) andalso present(Result) of
        true -> maps:update_with(Result, fun(Values) -> [Alias | Values] end,
                                 [Alias], Acc);
        false -> Acc
    end.

attach_recipe_aliases(Items, Aliases) ->
    [attach_item_aliases(Item, Aliases) || Item <- Items].

attach_item_aliases(Item, Aliases) ->
    with_recipe_aliases(
      Item#{<<"components">> => [with_recipe_aliases(Component, Aliases)
                                  || Component <- maps:get(<<"components">>, Item, [])]},
      Aliases).

with_recipe_aliases(Item, Aliases) ->
    case maps:get(maps:get(<<"uniqueName">>, Item, undefined), Aliases, []) of
        [] -> Item;
        Values -> Item#{<<"recipeAliases">> => lists:usort(Values)}
    end.

compact(Item) when is_map(Item) ->
    case maps:get(<<"uniqueName">>, Item, undefined) of
        Unique when is_binary(Unique), byte_size(Unique) > 0 ->
            (copy(Item,
                  [<<"uniqueName">>, <<"name">>, <<"imageName">>, <<"category">>,
                   <<"type">>, <<"masteryReq">>, <<"masterable">>, <<"tradable">>,
                   <<"isPrime">>, <<"vaulted">>, <<"primeSellingPrice">>,
                   <<"polarities">>, <<"aura">>, <<"exilusPolarity">>,
                   <<"stancePolarity">>, <<"polarity">>, <<"baseDrain">>,
                   <<"fusionLimit">>, <<"rarity">>, <<"compatName">>,
                   <<"productCategory">>, <<"exaltedSlot">>,
                   <<"isGalvanized">>, <<"isAmalgam">>, <<"isRiven">>]))#{
                <<"components">> => compact_components(maps:get(<<"components">>, Item, []))
            };
        _ -> undefined
    end;
compact(_Item) -> undefined.

compact_components(Values) when is_list(Values) ->
    [Component || Value <- Values,
                  Component <- [compact_component(Value)], Component =/= undefined];
compact_components(_Values) -> [].

compact_component(Component) when is_map(Component) ->
    case maps:get(<<"uniqueName">>, Component, undefined) of
        Unique when is_binary(Unique), byte_size(Unique) > 0 ->
            (copy(Component,
                  [<<"uniqueName">>, <<"name">>, <<"imageName">>, <<"type">>,
                   <<"itemCount">>, <<"tradable">>, <<"primeSellingPrice">>]))#{
                <<"drops">> => compact_drops(maps:get(<<"drops">>, Component, []))
            };
        _ -> undefined
    end;
compact_component(_Component) -> undefined.

compact_drops(Values) when is_list(Values) ->
    [copy(Drop, [<<"location">>, <<"chance">>, <<"uniqueName">>])
     || Drop <- Values, is_map(Drop), relic_drop(Drop)];
compact_drops(_Values) -> [].

put_index_entry(Item, Parent, IsComponent, Acc) when is_map(Item) ->
    case maps:get(<<"uniqueName">>, Item, undefined) of
        Identity when is_binary(Identity) ->
            Candidate = Item#{
                <<"parentCategory">> => maps:get(<<"category">>, Parent, <<>>),
                <<"parentName">> => maps:get(<<"name">>, Parent, <<>>),
                <<"parentUniqueName">> => maps:get(<<"uniqueName">>, Parent, undefined),
                <<"parentType">> => maps:get(<<"type">>, Parent, <<>>),
                <<"parentIsPrime">> => maps:get(<<"isPrime">>, Parent, false),
                <<"parentVaulted">> => maps:get(<<"vaulted">>, Parent, undefined),
                <<"component">> => IsComponent
            },
            Acc1 = maps:update_with(
              Identity, fun(Existing) -> prefer_index_entry(Existing, Candidate) end,
              Candidate, Acc),
            lists:foldl(
              fun(Alias, Inner) when is_binary(Alias), byte_size(Alias) > 0 ->
                      AliasCandidate = Candidate#{<<"uniqueName">> => Alias,
                                                  <<"recipeResultType">> => Identity,
                                                  <<"recipeAliases">> => []},
                      maps:update_with(
                        Alias,
                        fun(Existing) ->
                            prefer_index_entry(Existing, AliasCandidate)
                        end,
                        AliasCandidate, Inner);
                 (_Alias, Inner) -> Inner
              end,
              Acc1,
              maps:get(<<"recipeAliases">>, Item, []));
        _ -> Acc
    end;
put_index_entry(_Item, _Parent, _IsComponent, Acc) -> Acc.

prefer_index_entry(Existing, Candidate) ->
    case {maps:is_key(<<"recipeResultType">>, Existing),
          maps:is_key(<<"recipeResultType">>, Candidate)} of
        {true, false} -> maps:merge(Existing, Candidate);
        {false, true} -> maps:merge(Candidate, Existing);
        _ -> prefer_catalog_entry(Existing, Candidate)
    end.

prefer_catalog_entry(#{<<"component">> := true} = Component,
                     #{<<"component">> := false} = Item) ->
    maps:merge(Component, Item);
prefer_catalog_entry(#{<<"component">> := false} = Item,
                     #{<<"component">> := true} = Component) ->
    maps:merge(Component, Item);
prefer_catalog_entry(Existing, Candidate) ->
    case {present(maps:get(<<"imageName">>, Existing, undefined)),
          present(maps:get(<<"imageName">>, Candidate, undefined))} of
        {false, true} -> maps:merge(Existing, Candidate);
        _ -> maps:merge(Candidate, Existing)
    end.

relic_drop(Drop) ->
    case maps:get(<<"location">>, Drop, <<>>) of
        Location when is_binary(Location) ->
            binary:match(Location, <<" Relic">>) =/= nomatch;
        _ -> false
    end.

copy(Map, Keys) ->
    maps:with([Key || Key <- Keys, maps:is_key(Key, Map)], Map).

present(undefined) -> false;
present(null) -> false;
present(<<>>) -> false;
present([]) -> false;
present(_) -> true.

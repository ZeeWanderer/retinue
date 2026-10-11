%%%-------------------------------------------------------------------
%% Deterministic Deep/Temporal Archimedea equipment reconstruction.
%%%-------------------------------------------------------------------
-module(wf_archimedea_loadout).

-include_lib("kernel/include/file.hrl").

-export([context/0, resolve/2, select/3]).
-ifdef(TEST).
-export([pools/2, metadata/1]).
-endif.

-define(MASK32, 16#ffffffff).
-define(MASK64, 16#ffffffffffffffff).
-define(PCG_MULTIPLIER, 6364136223846793005).
-define(PCG_INCREMENT, 105).
-define(NAMES_CACHE, {?MODULE, names}).
-define(EXECUTABLE_HASH_CACHE, {?MODULE, executable_hash}).

-type context() :: map().

-doc "Build the player/catalog context used to index Archimedea rows.".
-spec context() -> context().
context() ->
    case application:get_env(retinue_daemon, archimedea_context_fun) of
        {ok, Fun} when is_function(Fun, 0) -> Fun();
        _ -> build_context()
    end.

-doc "Resolve one worldstate rotation against a prepared player/catalog context.".
-spec resolve(map(), context()) -> map().
resolve(Data, #{status := ready} = Context) ->
    case maps:get(<<"RandomSeed">>, Data, undefined) of
        Seed when is_integer(Seed) ->
            Forced = forced_loadouts(Data),
            Selected = select(Seed, Forced, Context),
            Selected#{status => ready};
        _ -> unavailable(missing_worldstate_seed)
    end;
resolve(_Data, #{status := unavailable, reason := Reason}) -> unavailable(Reason);
resolve(_Data, _Context) -> unavailable(missing_player_context).

-doc "Select all four equipment categories using Warframe's PCG32 sequence.".
-spec select(integer(), map(), context()) -> map().
select(WorldSeed, Forced, Context) ->
    Seed = luau_seed(WorldSeed + maps:get(account_seed, Context)),
    Pools = maps:get(pools, Context),
    Names = maps:get(names, Context, #{}),
    Categories = [suits, primaries, secondaries, melees],
    Selected = maps:from_list(
                 [{Category,
                   select_category(Seed, maps:get(Category, Pools),
                                   maps:get(Category, Forced, []), Names)}
                  || Category <- Categories]),
    Selected#{effective_seed => Seed,
              catalog_source => maps:get(catalog_source, Context, public_export),
              warning => maps:get(warning, Context, none)}.

luau_seed(Value) ->
    Narrowed = Value band ?MASK32,
    case Narrowed band 16#80000000 of
        0 -> Narrowed;
        _ -> Narrowed - 16#100000000
    end.

build_context() ->
    case {player_snapshot(), game_metadata_snapshot()} of
        {{ok, Player}, {ok, Metadata}} -> context_from_snapshots(Player, Metadata);
        {{error, Reason}, _} -> (unavailable(Reason))#{key => {unavailable, Reason}};
        {_, {error, Reason}} -> (unavailable(Reason))#{key => {unavailable, Reason}}
    end.

context_from_snapshots(Player, Metadata) ->
    Data = maps:get(data, Player, #{}),
    Account = maps:get(<<"account">>, Data, #{}),
    Inventory = maps:get(<<"inventory">>, Data, #{}),
    case {maps:get(<<"archimedea_seed">>, Account, undefined),
          maps:get(<<"raw">>, Inventory, undefined),
          metadata(Metadata)} of
        {Seed, Raw, {ok, Archimedea, MetadataKey, Warning}}
          when is_integer(Seed), is_map(Raw) ->
            case names() of
                {ok, Names, NamesSignature} ->
                    Pools = pools(Raw, Archimedea),
                    Key = erlang:md5(
                            term_to_binary(
                              {Seed, owned_key(Pools), MetadataKey, NamesSignature})),
                    #{status => ready, key => Key, account_seed => Seed,
                      pools => Pools, names => Names,
                      catalog_source => game_metadata, warning => Warning};
                {error, Reason} -> (unavailable(Reason))#{key => {names, Reason}}
            end;
        {Seed, _Raw, _Metadata} when not is_integer(Seed) ->
            (unavailable(missing_account_seed))#{key => {account, maps:get(revision, Player, 0)}};
        {_Seed, Raw, _Metadata} when not is_map(Raw) ->
            (unavailable(missing_player_inventory))#{key => {inventory, maps:get(revision, Player, 0)}};
        {_Seed, _Raw, {error, Reason}} ->
            (unavailable(Reason))#{key => {game_metadata,
                                           maps:get(revision, Metadata, 0), Reason}}
    end.

metadata(#{revision := 0}) -> {error, missing_game_metadata};
metadata(#{data := #{<<"schema">> := 2,
                     <<"executable">> := Executable = #{<<"sha256">> := Hash},
                     <<"archimedea">> := Archimedea} = Data,
           revision := Revision})
  when is_binary(Hash), is_map(Archimedea) ->
    case valid_archimedea(Archimedea) of
        true ->
            Warning = case not maps:is_key(<<"capture_error">>, Data) andalso
                           executable_current(Executable) of
                          true -> none;
                          false -> unverified_game_metadata
                      end,
            {ok, Archimedea, {Revision, Hash, Warning}, Warning};
        false -> {error, invalid_game_metadata}
    end;
metadata(#{data := #{<<"schema">> := 2,
                     <<"unavailable">> := #{<<"reason">> := Reason}}}) ->
    {error, unavailable_reason(Reason)};
metadata(_Metadata) -> {error, invalid_game_metadata}.

unavailable_reason(<<"unsupported_executable">>) -> unsupported_game_build;
unavailable_reason(_Reason) -> game_metadata_unavailable.

valid_archimedea(#{<<"catalog">> := Catalog,
                    <<"owned_suit_items">> := Suits,
                    <<"owned_weapon_items">> := Weapons,
                    <<"suit_aliases">> := SuitAliases,
                    <<"weapon_aliases">> := WeaponAliases}) ->
    is_map(Catalog) andalso
    lists:all(fun(Key) -> binary_list(maps:get(Key, Catalog, invalid)) end,
              [<<"suits">>, <<"primaries">>, <<"secondaries">>, <<"melees">>]) andalso
    binary_list(Suits) andalso binary_list(Weapons) andalso
    alias_list(SuitAliases) andalso alias_list(WeaponAliases);
valid_archimedea(_Archimedea) -> false.

binary_list(Values) when is_list(Values) -> lists:all(fun is_binary/1, Values);
binary_list(_Values) -> false.

alias_list(Values) when is_list(Values) ->
    lists:all(
      fun(#{<<"source">> := Source, <<"canonical">> := Canonical}) ->
              is_binary(Source) andalso is_binary(Canonical);
         (_Value) -> false
      end,
      Values);
alias_list(_Values) -> false.

executable_current(#{<<"path">> := Path0, <<"size">> := CapturedSize,
                     <<"modified_unix_ms">> := CapturedModified,
                     <<"sha256">> := Hash})
  when is_integer(CapturedSize), is_integer(CapturedModified), is_binary(Hash) ->
    Path = retinue_text:to_list(Path0),
    case file:read_file_info(Path, [{time, posix}]) of
        {ok, #file_info{size = CapturedSize, mtime = Modified}}
          when CapturedModified div 1000 =:= Modified -> true;
        {ok, #file_info{size = CapturedSize, mtime = Modified}} ->
            executable_hash(Path, CapturedSize, Modified) =:= decode_hash(Hash);
        {ok, #file_info{}} -> false;
        {error, _Reason} -> true
    end;
executable_current(_Executable) -> true.

decode_hash(Hash) ->
    try binary:decode_hex(Hash)
    catch error:_ -> invalid
    end.

executable_hash(Path, Size, Modified) ->
    Key = {Path, Size, Modified},
    case persistent_term:get(?EXECUTABLE_HASH_CACHE, undefined) of
        {Key, Hash} -> Hash;
        _ ->
            Hash = hash_file(Path),
            persistent_term:put(?EXECUTABLE_HASH_CACHE, {Key, Hash}),
            Hash
    end.

hash_file(Path) ->
    case file:open(Path, [read, binary, raw]) of
        {ok, File} ->
            try hash_file(File, crypto:hash_init(sha256))
            after file:close(File)
            end;
        {error, _Reason} -> unavailable
    end.

hash_file(File, Context) ->
    case file:read(File, 1024 * 1024) of
        {ok, Data} -> hash_file(File, crypto:hash_update(Context, Data));
        eof -> crypto:hash_final(Context);
        {error, _Reason} -> unavailable
    end.

player_snapshot() ->
    try {ok, wf_player_service:snapshot()}
    catch exit:_ -> {error, player_store_unavailable}
    end.

game_metadata_snapshot() ->
    try {ok, wf_game_metadata_service:snapshot()}
    catch exit:_ -> {error, game_metadata_store_unavailable}
    end.

names() ->
    case names_once() of
        {error, missing_equipment_exports} = Missing ->
            case ensure_catalog() of
                ok -> names_once();
                {error, _Reason} -> Missing
            end;
        Result -> Result
    end.

names_once() ->
    Files = ["ExportWarframes_en.json", "ExportWeapons_en.json"],
    Sources = wf_exports:item_sources(undefined, Files),
    Signature = [{Path, filelib:last_modified(Path), filelib:file_size(Path)}
                 || {_File, Path} <- Sources],
    case lists:all(fun({_Path, Modified, Size}) -> Modified =/= 0 andalso Size > 0 end,
                   Signature) of
        false -> {error, missing_equipment_exports};
        true -> cached_names(Files, Signature)
    end.

ensure_catalog() ->
    case whereis(wf_source_manager) of
        undefined -> {error, source_manager_unavailable};
        _Pid -> wf_source_manager:ensure_catalog("archimedea", #{})
    end.

cached_names(Files, Signature) ->
    case persistent_term:get(?NAMES_CACHE, undefined) of
        {Signature, Names} -> {ok, Names, Signature};
        _ ->
            case wf_exports:load_items(undefined, Files) of
                {ok, Items} ->
                    Names = build_names(Items),
                    persistent_term:put(?NAMES_CACHE, {Signature, Names}),
                    {ok, Names, Signature};
                {error, Reason} -> {error, Reason}
            end
    end.

build_names(Items) ->
    maps:from_list([{path(Item), retinue_text:to_list(maps:get(name, Item, <<>>))}
                    || Item <- Items, path(Item) =/= <<>>]).

pools(Raw, Archimedea) ->
    Catalog = maps:get(<<"catalog">>, Archimedea),
    SuitAliases = alias_map(maps:get(<<"suit_aliases">>, Archimedea)),
    WeaponAliases = alias_map(maps:get(<<"weapon_aliases">>, Archimedea)),
    SuitItems = maps:from_keys(maps:get(<<"owned_suit_items">>, Archimedea), true),
    WeaponItems = maps:from_keys(maps:get(<<"owned_weapon_items">>, Archimedea), true),
    #{suits => category_pool(
                  owned_paths(Raw, <<"Suits">>, SuitAliases, SuitItems, suit),
                  maps:get(<<"suits">>, Catalog)),
      primaries => category_pool(
                     owned_paths(Raw, <<"LongGuns">>, WeaponAliases, WeaponItems, weapon),
                     maps:get(<<"primaries">>, Catalog)),
      secondaries => category_pool(
                       owned_paths(Raw, <<"Pistols">>, WeaponAliases, WeaponItems, weapon),
                       maps:get(<<"secondaries">>, Catalog)),
      melees => category_pool(
                  owned_paths(Raw, <<"Melee">>, WeaponAliases, WeaponItems, weapon),
                  maps:get(<<"melees">>, Catalog))}.

category_pool(Owned, Catalog) -> #{owned => unique(Owned), catalog => Catalog}.

owned_paths(Raw, Key, Aliases, Allowed, Kind) ->
    unique(
      [Canonical || Item <- maps:get(Key, Raw, []), is_map(Item),
                    eligible_owned_item(Item, Kind),
                    Path <- [retinue_text:to_binary(maps:get(<<"ItemType">>, Item, <<>>))],
                    Path =/= <<>>,
                    maps:is_key(Path, Allowed),
                    Canonical <- [canonical_path(Path, Aliases, Kind)],
                    Canonical =/= <<>>]).

eligible_owned_item(_Item, suit) -> true;
eligible_owned_item(Item, weapon) ->
    AltMode = maps:get(<<"mAltWeaponModeId">>, Item,
                       maps:get(<<"AltWeaponModeId">>, Item, undefined)),
    AltMode =:= undefined orelse AltMode =:= null orelse AltMode =:= <<>>.

alias_map(Aliases) ->
    maps:from_list(
      [{retinue_text:to_binary(Source), retinue_text:to_binary(Canonical)}
       || #{<<"source">> := Source, <<"canonical">> := Canonical} <- Aliases]).

canonical_path(Path, Aliases, weapon) ->
    canonical_alias(hardcoded_weapon_alias(canonical_alias(Path, Aliases)), Aliases);
canonical_path(Path, Aliases, suit) -> canonical_alias(Path, Aliases).

canonical_alias(Path, Aliases) -> canonical_alias(Path, Aliases, #{}).

canonical_alias(Path, Aliases, Seen) ->
    case {maps:is_key(Path, Seen), maps:get(Path, Aliases, undefined)} of
        {true, _} -> Path;
        {false, Next} when is_binary(Next), Next =/= Path ->
            canonical_alias(Next, Aliases, Seen#{Path => true});
        _ -> Path
    end.

hardcoded_weapon_alias(<<"/Lotus/Weapons/Tenno/Shotgun/QuadShotgunBase">>) ->
    <<"/Lotus/Weapons/Tenno/Shotgun/QuadShotgun">>;
hardcoded_weapon_alias(<<"/Lotus/Weapons/Tenno/Melee/Dagger/DarkDaggerBase">>) ->
    <<"/Lotus/Weapons/Tenno/Melee/Dagger/DarkDagger">>;
hardcoded_weapon_alias(Path) -> Path.

path(Item) -> retinue_text:to_binary(maps:get(uniqueName, Item, <<>>)).

owned_key(Pools) ->
    [{Category, maps:get(owned, maps:get(Category, Pools))}
     || Category <- [suits, primaries, secondaries, melees]].

forced_loadouts(Data) ->
    Raw = maps:get(<<"ForcedLoadouts">>, Data,
                   maps:get(<<"forcedLoadout">>, Data, #{})),
    #{suits => forced_category(Raw, <<"suits">>, <<"Suits">>),
      primaries => forced_category(Raw, <<"primaries">>, <<"Primaries">>),
      secondaries => forced_category(Raw, <<"secondaries">>, <<"Secondaries">>),
      melees => forced_category(Raw, <<"melees">>, <<"Melees">>)}.

forced_category(Raw, Lower, Upper) when is_map(Raw) ->
    [Path || Item <- maps:get(Lower, Raw, maps:get(Upper, Raw, [])),
             Path <- [forced_path(Item)], Path =/= <<>>];
forced_category(_Raw, _Lower, _Upper) -> [].

forced_path(Item) when is_map(Item) ->
    retinue_text:to_binary(
      maps:get(<<"ItemType">>, Item,
               maps:get(<<"itemType">>, Item, maps:get(<<"type">>, Item, <<>>))));
forced_path(Item) -> retinue_text:to_binary(Item).

select_category(Seed, Pool, Forced, Names) ->
    ForcedCount = length(Forced),
    CatalogCount = max(0, 2 - ForcedCount),
    OwnedCount = max(0, 1 - max(0, ForcedCount - 2)),
    Rng0 = pcg_seed(Seed),
    {Owned, _OwnedPool, Rng1} = take_random(OwnedCount, maps:get(owned, Pool), Rng0, []),
    {Catalog, _CatalogPool, _Rng2} =
        take_catalog(CatalogCount, maps:get(catalog, Pool), Rng1, Owned, []),
    OwnedSet = maps:from_keys(maps:get(owned, Pool), true),
    [item(Path, Source, maps:is_key(Path, OwnedSet), Names)
     || {Path, Source} <- [{Path, owned} || Path <- Owned] ++
                          [{Path, catalog} || Path <- Catalog] ++
                          [{Path, forced} || Path <- Forced]].

take_random(0, Pool, Rng, Acc) -> {lists:reverse(Acc), Pool, Rng};
take_random(_Count, [], Rng, Acc) -> {lists:reverse(Acc), [], Rng};
take_random(Count, Pool, Rng0, Acc) ->
    {Index, Rng1} = bounded(length(Pool), Rng0),
    {Value, Rest} = take_at(Index, Pool),
    take_random(Count - 1, Rest, Rng1, [Value | Acc]).

take_catalog(0, Pool, Rng, _Selected, Acc) -> {lists:reverse(Acc), Pool, Rng};
take_catalog(_Count, [], Rng, _Selected, Acc) -> {lists:reverse(Acc), [], Rng};
take_catalog(Count, Pool, Rng0, Selected, Acc) ->
    {Index, Rng1} = bounded(length(Pool), Rng0),
    {Value, Rest} = take_at(Index, Pool),
    case lists:member(Value, Selected) orelse lists:member(Value, Acc) of
        true -> take_catalog(Count, Rest, Rng1, Selected, Acc);
        false -> take_catalog(Count - 1, Rest, Rng1, Selected, [Value | Acc])
    end.

take_at(Index, List) ->
    {Before, [Value | After]} = lists:split(Index, List),
    {Value, Before ++ After}.

item(Path, Source, Owned, Names) ->
    #{id => retinue_text:to_list(Path),
      name => maps:get(Path, Names, fallback_name(Path)),
      source => Source,
      owned => Owned}.

fallback_name(Path) ->
    case binary:split(Path, <<"/">>, [global]) of
        [] -> retinue_text:to_list(Path);
        Parts -> retinue_text:to_list(lists:last(Parts))
    end.

pcg_seed(Seed) ->
    {_Value0, State1} = pcg_next(0),
    State2 = (State1 + (Seed band ?MASK64)) band ?MASK64,
    {_Value1, State3} = pcg_next(State2),
    State3.

pcg_next(State) ->
    Next = (State * ?PCG_MULTIPLIER + ?PCG_INCREMENT) band ?MASK64,
    Shifted = (((State bsr 18) bxor State) bsr 27) band ?MASK32,
    Rotation = State bsr 59,
    Value = ((Shifted bsr Rotation) bor
             ((Shifted bsl ((-Rotation) band 31)) band ?MASK32)) band ?MASK32,
    {Value, Next}.

bounded(Bound, State0) ->
    {Value, State1} = pcg_next(State0),
    {(Value * Bound) bsr 32, State1}.

unique(Values) -> unique(Values, #{}, []).

unique([], _Seen, Acc) -> lists:reverse(Acc);
unique([Value | Rest], Seen, Acc) ->
    case maps:is_key(Value, Seen) of
        true -> unique(Rest, Seen, Acc);
        false -> unique(Rest, Seen#{Value => true}, [Value | Acc])
    end.

unavailable(Reason) -> #{status => unavailable, reason => Reason}.

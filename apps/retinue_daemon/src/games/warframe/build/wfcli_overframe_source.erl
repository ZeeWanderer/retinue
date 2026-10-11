%%%-------------------------------------------------------------------
%% Overframe adapter. Private web/API details stop at this module.
%%%-------------------------------------------------------------------
-module(wfcli_overframe_source).

-export([catalog_schema/0, refresh_catalog/0, search/4, list/2, detail/2,
         decode_database/1, normalize_detail/2, present_revision/2, mod_elements/2]).

-define(PROBE_URL, "https://overframe.gg/_next/static/chunks/webpack.js").
-define(API_ROOT, "https://overframe.gg/api/v1").
-define(STATIC_ROOT, "https://static.overframe.gg/_next/static/chunks/").
-define(CATALOG_TIMEOUT, 45000).
-define(CATALOG_SCHEMA, 5).

-doc "Return normalized Overframe catalog schema understood by this adapter.".
-spec catalog_schema() -> pos_integer().
catalog_schema() -> ?CATALOG_SCHEMA.

-doc "Fetch current Overframe item, mod, and Riven ID catalogs.".
-spec refresh_catalog() -> {ok, map()} | {error, term()}.
refresh_catalog() ->
    case wfcli_overframe_http:get(?PROBE_URL, []) of
        {ok, Status, Html} when Status =:= 200; Status =:= 404 ->
            case runtime_url(Html) of
                {ok, RuntimeUrl} -> fetch_runtime(RuntimeUrl);
                {error, _Reason} = Error -> Error
            end;
        {ok, Status, _Body} -> {error, {manifest_status, Status}};
        {error, _Reason} = Error -> Error
    end.

-doc "Search source-supported equipment without exposing source-specific IDs as identity.".
-spec search(map(), binary(), binary(), pos_integer()) -> {ok, map()}.
search(Catalog, Query0, Class, Limit) ->
    Query = string:casefold(Query0),
    Items = maps:values(maps:get(items_by_path, Catalog, #{})),
    Matches0 = [Item || Item <- Items,
                        maps:get(buildable, Item, false),
                        displayable(Item),
                        class_matches(Class, maps:get(<<"class">>, Item, <<"other">>)),
                        text_matches(Query, Item)],
    Matches = lists:sublist(lists:sort(fun item_before/2, Matches0), Limit),
    {ok, #{<<"source">> => <<"overframe">>, <<"items">> => Matches,
           <<"count">> => length(Matches)}}.

-doc "List normalized build summaries for one canonical equipment item.".
-spec list(map(), map()) -> {ok, map()} | {error, term()}.
list(Catalog, Request) ->
    case item_for_request(Catalog, Request) of
        {ok, Item} ->
            case list_params(Item, Request) of
                {ok, Params, Headers} ->
                    Url = api_url("/builds/", Params),
                    case wfcli_overframe_http:get_json(Url, Headers) of
                        {ok, 200, Body} when is_map(Body) ->
                            normalize_list(Body, Catalog, Request);
                        {ok, Status, _Body} -> {error, {build_list_status, Status}};
                        {error, _Reason} = Error -> Error
                    end;
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Fetch one build and normalize it as an immutable revision.".
-spec detail(map(), map()) -> {ok, map()} | {error, term()}.
detail(Catalog, Request) ->
    case positive_integer(maps:get(id, Request, undefined)) of
        {ok, Id} ->
            Url = api_url("/builds/" ++ integer_to_list(Id) ++ "/", []),
            case wfcli_overframe_http:get_json(Url, []) of
                {ok, 200, Body} when is_map(Body) ->
                    {ok, normalize_detail(Body, Catalog)};
                {ok, 404, _Body} -> {error, build_not_found};
                {ok, Status, _Body} -> {error, {build_detail_status, Status}};
                {error, _Reason} = Error -> Error
            end;
        error -> {error, invalid_build_id}
    end.

-doc "Decode a webpack database chunk containing a JSON.parse string.".
-spec decode_database(binary()) -> {ok, map()} | {error, term()}.
decode_database(Source) when is_binary(Source) ->
    Marker = <<"JSON.parse('">>,
    case binary:match(Source, Marker) of
        nomatch -> {error, missing_json_database};
        {Position, _Length} ->
            Start = Position + byte_size(Marker),
            Tail = binary:part(Source, Start, byte_size(Source) - Start),
            case take_js_string(Tail, []) of
                {ok, Escaped} ->
                    case unescape_js(Escaped, []) of
                        {ok, Json} -> decode_json_map(Json);
                        {error, _Reason} = Error -> Error
                    end;
                {error, _Reason} = Error -> Error
            end
    end.

-doc "Normalize one raw Overframe build detail payload.".
-spec normalize_detail(map(), map()) -> map().
normalize_detail(Raw, Catalog) ->
    ExternalId = maps:get(<<"id">>, Raw),
    ExternalItemId = maps:get(<<"item">>, Raw, undefined),
    Item = item_by_external_id(Catalog, ExternalItemId),
    Slots = [normalize_slot(Slot, Catalog)
             || Slot <- maps:get(<<"slots">>, Raw, []), is_map(Slot)],
    Content = #{<<"item">> => maps:get(<<"canonical_id">>, Item, null),
                <<"item_external_id">> => nullable(ExternalItemId),
                <<"item_rank">> => maps:get(<<"item_rank">>, Raw, null),
                <<"buildstring">> => maps:get(<<"buildstring">>, Raw, <<>>),
                <<"slots">> => Slots},
    Fingerprint = binary:encode_hex(
                    crypto:hash(sha256,
                                term_to_binary(Content, [deterministic])),
                    lowercase),
    present_revision(
      #{<<"schema">> => 1,
        <<"identity">> => #{<<"source">> => <<"overframe">>,
                             <<"external_id">> => ExternalId},
        <<"fingerprint">> => Fingerprint,
        <<"content">> => Content,
        <<"metadata">> => normalize_metadata(Raw, Item),
        <<"raw">> => Raw,
        <<"fetched_at">> => erlang:system_time(millisecond)},
      Catalog).

-doc "Add current display metadata without changing revision identity.".
-spec present_revision(map(), map()) -> map().
present_revision(Revision, Catalog) when is_map(Revision), is_map(Catalog) ->
    Content = maps:get(<<"content">>, Revision, #{}),
    Metadata0 = maps:get(<<"metadata">>, Revision, #{}),
    Item = revision_item(Content, Metadata0, Catalog),
    Definition = wfcli_build_topology:definition(
                   maps:get(<<"class">>, Item, <<"other">>), Item),
    Topology = maps:get(<<"topology">>, Definition),
    Bindings = source_slot_bindings(Topology),
    Upgrades = [present_slot(Slot, Catalog, Bindings)
                 || Slot <- maps:get(<<"slots">>, Content, []), is_map(Slot)],
    Presentation = #{<<"topology">> => Topology,
                      <<"upgrades">> => Upgrades,
                      <<"item_asset">> => item_asset(Item)},
    Revision#{<<"metadata">> => present_metadata(Metadata0, Revision),
              <<"presentation">> => Presentation}.

fetch_runtime(RuntimeUrl) ->
    case wfcli_overframe_http:get(RuntimeUrl, []) of
        {ok, 200, Runtime} ->
            Specs = [{items, <<"db/items">>},
                     {mods, <<"db/mods">>},
                     {rivens, <<"db/rivens">>}],
            case chunk_urls(Runtime, Specs, []) of
                {ok, Urls} -> fetch_chunks(Urls, RuntimeUrl);
                {error, _Reason} = Error -> Error
            end;
        {ok, Status, _Body} -> {error, {runtime_status, Status}};
        {error, _Reason} = Error -> Error
    end.

fetch_chunks(Urls, RuntimeUrl) ->
    Parent = self(),
    Workers = [begin
                   {Pid, Monitor} = spawn_monitor(fun() ->
                       Result = fetch_chunk(Url),
                       Parent ! {overframe_chunk, self(), Kind, Result}
                   end),
                   {Pid, Monitor, Kind}
               end || {Kind, Url} <- Urls],
    case gather_chunks(Workers, #{}) of
        {ok, Databases} ->
            Items = normalize_items(maps:get(items, Databases)),
            Mods = normalize_mods(maps:get(mods, Databases)),
            Rivens = normalize_rivens(maps:get(rivens, Databases)),
            Identity = crypto:hash(sha256,
                                   iolist_to_binary([RuntimeUrl |
                                                     [Url || {_Kind, Url} <- Urls]])),
            {ok, #{schema => ?CATALOG_SCHEMA,
                   version => binary:encode_hex(Identity, lowercase),
                   fetched_at => erlang:system_time(millisecond),
                   items_by_path => Items,
                   items_by_id => index_external_id(Items),
                   mods_by_id => Mods,
                   mods_by_path => maps:from_list(
                     [{maps:get(<<"canonical_id">>, Mod), Mod} || Mod <- maps:values(Mods)]),
                   rivens_by_id => Rivens}};
        {error, _Reason} = Error -> Error
    end.

fetch_chunk(Url) ->
    case wfcli_overframe_http:get(Url, []) of
        {ok, 200, Body} -> decode_database(Body);
        {ok, Status, _Body} -> {error, {chunk_status, Status}};
        {error, _Reason} = Error -> Error
    end.

gather_chunks([], Databases) -> {ok, Databases};
gather_chunks(Workers, Databases) ->
    receive
        {overframe_chunk, Pid, Kind, {ok, Database}} ->
            case lists:keytake(Pid, 1, Workers) of
                {value, {_Pid, Monitor, Kind}, Rest} ->
                    erlang:demonitor(Monitor, [flush]),
                    gather_chunks(Rest, Databases#{Kind => Database});
                false -> gather_chunks(Workers, Databases)
            end;
        {overframe_chunk, _Pid, Kind, {error, Reason}} ->
            stop_chunk_workers(Workers),
            {error, {catalog_chunk, Kind, Reason}};
        {'DOWN', Monitor, process, Pid, Reason} ->
            case lists:keyfind(Pid, 1, Workers) of
                {_Pid, Monitor, Kind} ->
                    stop_chunk_workers(Workers),
                    {error, {catalog_worker, Kind, Reason}};
                false -> gather_chunks(Workers, Databases)
            end
    after ?CATALOG_TIMEOUT ->
        stop_chunk_workers(Workers),
        {error, catalog_timeout}
    end.

stop_chunk_workers(Workers) ->
    lists:foreach(fun({Pid, Monitor, _Kind}) ->
                      exit(Pid, kill),
                      erlang:demonitor(Monitor, [flush])
                  end, Workers).

runtime_url(Html) ->
    Pattern = <<"https://static\\.overframe\\.gg/_next/static/chunks/"
                "webpack-[0-9a-f]+\\.js">>,
    case re:run(Html, Pattern, [{capture, first, binary}]) of
        {match, [Url]} -> {ok, binary_to_list(Url)};
        nomatch -> {error, runtime_url_not_found}
    end.

chunk_urls(_Runtime, [], Acc) -> {ok, lists:reverse(Acc)};
chunk_urls(Runtime, [{Kind, Name} | Rest], Acc) ->
    NamePattern = iolist_to_binary([<<"([0-9]+):\\\"">>, Name, <<"\\\"">>]),
    case re:run(Runtime, NamePattern, [{capture, [1], binary}]) of
        {match, [Id]} ->
            HashPattern = iolist_to_binary(
                            [<<"(?:\\{|,)">>, Id,
                             <<":\\\"([0-9a-f]+)\\\"">>]),
            case re:run(Runtime, HashPattern, [{capture, [1], binary}]) of
                {match, [Hash]} ->
                    Url = iolist_to_binary([?STATIC_ROOT, Name, $., Hash, <<".js">>]),
                    chunk_urls(Runtime, Rest, [{Kind, binary_to_list(Url)} | Acc]);
                nomatch -> {error, {chunk_hash_not_found, Name}}
            end;
        nomatch -> {error, {chunk_id_not_found, Name}}
    end.

normalize_items(Database) ->
    maps:fold(fun(Path, Record, Acc) when is_map(Record) ->
                  Data = maps:get(<<"data">>, Record, #{}),
                  Categories = maps:get(<<"categories">>, Record, []),
                  Slots = maps:get(<<"ArtifactSlots">>, Data, []),
                  RawName = maps:get(<<"name">>, Record, Path),
                  Item = #{<<"source">> => <<"overframe">>,
                           <<"external_id">> => maps:get(<<"id">>, Record),
                           <<"canonical_id">> => Path,
                           <<"name">> => display_name(RawName),
                           <<"class">> => item_class(Categories, Record, RawName),
                           <<"categories">> => Categories,
                           <<"texture">> => maps:get(<<"texture_new">>, Record,
                                                    maps:get(<<"texture">>, Record,
                                                             null)),
                           buildable => is_list(Slots) andalso Slots =/= []},
                  Acc#{Path => Item};
             (_Path, _Record, Acc) -> Acc
              end, #{}, Database).

normalize_mods(Database) ->
    maps:fold(fun(Path, Record, Acc) when is_map(Record) ->
                  case maps:get(<<"id">>, Record, undefined) of
                      Id when is_integer(Id) ->
                          Data = maps:get(<<"data">>, Record, #{}),
                          Entry = #{<<"external_id">> => Id,
                                    <<"canonical_id">> => Path,
                                    <<"name">> => maps:get(<<"name">>, Record, Path),
                                    <<"categories">> =>
                                        maps:get(<<"categories">>, Record, []),
                                    <<"polarity">> =>
                                        overframe_polarity(
                                          maps:get(<<"ArtifactPolarity">>, Data,
                                                   <<"AP_UNIVERSAL">>)),
                                    <<"base_drain">> =>
                                        overframe_base_drain(
                                          maps:get(<<"BaseDrain">>, Data, <<"QA_LOW">>)),
                                    <<"max_rank">> =>
                                        overframe_max_rank(
                                          maps:get(<<"FusionLimit">>, Data, <<"QA_HIGH">>)),
                                    <<"rarity">> =>
                                        overframe_rarity(
                                          maps:get(<<"Rarity">>, Data, null)),
                                    <<"mod_variant">> => mod_variant(Data),
                                    <<"elemental_types">> => elemental_types(Data),
                                    <<"texture">> =>
                                        maps:get(<<"texture_new">>, Record,
                                                 maps:get(<<"texture">>, Record, null))},
                          Acc#{Id => Entry};
                      _ -> Acc
                  end;
             (_Path, _Record, Acc) -> Acc
              end, #{}, Database).

normalize_rivens(Database) ->
    maps:fold(fun(_Path, Record, Acc) when is_map(Record) ->
                  case maps:get(<<"id">>, Record, undefined) of
                      Id when is_integer(Id) -> Acc#{Id => Record};
                      _ -> Acc
                  end;
             (_Path, _Record, Acc) -> Acc
              end, #{}, Database).

index_external_id(Items) ->
    maps:from_list([{maps:get(<<"external_id">>, Item), Item}
                    || Item <- maps:values(Items)]).

normalize_list(Body, Catalog, Request) ->
    Results = [normalize_summary(Build, Catalog)
               || Build <- maps:get(<<"results">>, Body, []), is_map(Build)],
    {ok, #{<<"source">> => <<"overframe">>,
           <<"scope">> => scope_binary(maps:get(scope, Request, public)),
           <<"count">> => maps:get(<<"count">>, Body, length(Results)),
           <<"next">> => nullable(maps:get(<<"next">>, Body, undefined)),
           <<"previous">> => nullable(maps:get(<<"previous">>, Body, undefined)),
           <<"builds">> => Results}}.

normalize_summary(Build, Catalog) ->
    ItemId = maps:get(<<"item">>, Build, undefined),
    Item = item_by_external_id(Catalog, ItemId),
    #{<<"identity">> => #{<<"source">> => <<"overframe">>,
                           <<"external_id">> => maps:get(<<"id">>, Build)},
      <<"item">> => Item,
      <<"title">> => maps:get(<<"title">>, Build, <<>>),
      <<"author">> => normalize_author(maps:get(<<"author">>, Build, null)),
      <<"score">> => maps:get(<<"score">>, Build, 0),
      <<"formas">> => maps:get(<<"formas">>, Build, 0),
      <<"updated_at">> => nullable(maps:get(<<"updated">>, Build, undefined)),
      <<"url">> => nullable(maps:get(<<"url">>, Build, undefined))}.

normalize_metadata(Raw, Item) ->
    #{<<"item">> => Item,
      <<"title">> => maps:get(<<"title">>, Raw, <<>>),
      <<"author">> => normalize_author(maps:get(<<"author">>, Raw, null)),
      <<"score">> => maps:get(<<"score">>, Raw, 0),
      <<"formas">> => maps:get(<<"formas">>, Raw, 0),
      <<"created_at">> => nullable(maps:get(<<"created">>, Raw, undefined)),
      <<"updated_at">> => nullable(maps:get(<<"updated">>, Raw, undefined)),
      <<"url">> => nullable(maps:get(<<"url">>, Raw, undefined))}.

present_metadata(Metadata, Revision) ->
    Raw = maps:get(<<"raw">>, Revision, #{}),
    case maps:get(<<"description">>, Raw,
                  maps:get(<<"description">>, Metadata, undefined)) of
        Description when is_binary(Description), byte_size(Description) > 0 ->
            Metadata#{<<"description">> => Description};
        _ -> Metadata
    end.

revision_item(Content, Metadata, Catalog) ->
    case maps:get(<<"item">>, Metadata, undefined) of
        Item when is_map(Item) -> Item;
        _ -> item_by_external_id(
               Catalog, maps:get(<<"item_external_id">>, Content, undefined))
    end.

present_slot(Slot, Catalog) ->
    Mod = maps:get(maps:get(<<"external_mod_id">>, Slot, undefined),
                   maps:get(mods_by_id, Catalog, #{}), #{}),
    CanonicalId = maps:get(<<"canonical_id">>, Slot,
                           maps:get(<<"canonical_id">>, Mod, null)),
    Texture = maps:get(<<"texture">>, Mod, null),
    Fields = maps:with([<<"name">>, <<"rarity">>, <<"mod_variant">>,
                        <<"base_drain">>, <<"max_rank">>, <<"elemental_types">>], Mod),
    Current = maps:merge(Slot, Fields),
    Cost = case ranked_cost(maps:get(<<"base_drain">>, Current, null),
                             maps:get(<<"rank">>, Slot, 0)) of
        null -> maps:get(<<"cost">>, Slot, null);
        Value -> Value
    end,
    Current#{<<"item_type">> => CanonicalId,
             <<"cost">> => Cost,
             <<"asset">> => asset(CanonicalId, Texture)}.

present_slot(Slot, Catalog, Bindings) ->
    Presented = present_slot(Slot, Catalog),
    case maps:find(maps:get(<<"source_slot">>, Slot, undefined), Bindings) of
        {ok, TopologySlot} -> Presented#{<<"topology_slot">> => TopologySlot};
        error -> Presented
    end.

source_slot_bindings(Topology) ->
    maps:from_list(
      [{maps:get(<<"build_slot">>, Slot), maps:get(<<"id">>, Slot)}
       || Region <- maps:get(<<"regions">>, Topology, []),
          Slot <- maps:get(<<"slots">>, Region, []),
          is_integer(maps:get(<<"build_slot">>, Slot, undefined))]).

item_asset(Item) ->
    asset(maps:get(<<"canonical_id">>, Item, null),
          maps:get(<<"texture">>, Item, null)).

asset(Id, Image) when is_binary(Id), byte_size(Id) > 0,
                      is_binary(Image), byte_size(Image) > 0 ->
    #{<<"id">> => Id, <<"source">> => <<"wfcd">>,
      <<"image_name">> => filename:basename(Image)};
asset(_Id, _Image) -> null.

normalize_author(Author) when is_map(Author) ->
    maps:with([<<"id">>, <<"username">>, <<"url">>], Author);
normalize_author(_Author) -> null.

normalize_slot(Slot, Catalog) ->
    ModId = maps:get(<<"mod">>, Slot, undefined),
    Mod = maps:get(ModId, maps:get(mods_by_id, Catalog, #{}), #{}),
    Rank = maps:get(<<"rank">>, Slot, 0),
    ModPolarity = maps:get(<<"polarity">>, Mod, null),
    SlotPolarity = overframe_polarity(maps:get(<<"polarity">>, Slot, null)),
    #{<<"source_slot">> => maps:get(<<"slot_id">>, Slot, null),
      <<"external_mod_id">> => nullable(ModId),
      <<"canonical_id">> => maps:get(<<"canonical_id">>, Mod, null),
      <<"name">> => maps:get(<<"name">>, Mod, null),
      <<"kind">> => mod_kind(Mod),
      <<"rank">> => Rank,
      <<"mod_polarity">> => ModPolarity,
      <<"slot_polarity">> => SlotPolarity,
      <<"polarity_state">> => polarity_state(ModPolarity, SlotPolarity),
      <<"rarity">> => maps:get(<<"rarity">>, Mod, null),
      <<"mod_variant">> => maps:get(<<"mod_variant">>, Mod, <<"standard">>),
      <<"base_drain">> => maps:get(<<"base_drain">>, Mod, null),
      <<"max_rank">> => maps:get(<<"max_rank">>, Mod, null),
      <<"cost">> => ranked_cost(maps:get(<<"base_drain">>, Mod, null), Rank),
      <<"polarity">> => maps:get(<<"polarity">>, Slot, null),
      <<"drain">> => maps:get(<<"drain">>, Slot, null)}.

polarity_state(null, _Slot) -> null;
polarity_state(_Mod, null) -> null;
polarity_state(Mod, Slot) ->
    atom_to_binary(wfcli_polarity:compatibility(Mod, Slot)).

mod_kind(Mod) ->
    Categories = maps:get(<<"categories">>, Mod, []),
    case lists:member(<<"arcane">>, Categories) of
        true -> <<"arcane">>;
        false -> <<"mod">>
    end.

overframe_polarity(null) -> null;
overframe_polarity(Value) ->
    atom_to_binary(wfcli_polarity:normalize(Value)).

overframe_base_drain(Value) ->
    quality_value(Value, #{0 => 0, 1 => 2, 2 => 4, 3 => 6, 4 => 10}).

overframe_max_rank(Value) ->
    quality_value(Value, #{0 => 0, 1 => 0, 2 => 3, 3 => 5, 4 => 10}).

overframe_rarity(Value) when is_binary(Value) -> string:lowercase(Value);
overframe_rarity(_Value) -> null.

mod_variant(#{<<"IsGalvanized">> := V}) when V =:= true; V =:= 1 -> <<"galvanized">>;
mod_variant(#{<<"IsAmalgam">> := V}) when V =:= true; V =:= 1 -> <<"amalgam">>;
mod_variant(#{<<"IsRiven">> := V}) when V =:= true; V =:= 1 -> <<"riven">>;
mod_variant(_Data) -> <<"standard">>.

mod_elements(Catalog, ItemType) ->
    Mod = maps:get(ItemType, maps:get(mods_by_path, Catalog, #{}), #{}),
    maps:get(<<"elemental_types">>, Mod, null).

elemental_types(Data) ->
    Types = #{<<"DT_FIRE">> => <<"heat">>, <<"DT_FREEZE">> => <<"cold">>,
              <<"DT_ELECTRICITY">> => <<"electricity">>, <<"DT_POISON">> => <<"toxin">>},
    Elements = [maps:get(Type, Types) || Upgrade <- maps:get(<<"Upgrades">>, Data, []),
                 Type <- [maps:get(<<"DamageType">>, Upgrade, undefined)],
                 maps:is_key(Type, Types)],
    Sub = maps:get(<<"SubUpgrades">>, Data, []) ++
          case maps:find(<<"SubUpgrade">>, Data) of {ok, S} -> [S]; error -> [] end,
    SubElements = [case S of M when is_map(M) -> elemental_types(M); _ -> null end || S <- Sub],
    Conditional = maps:get(<<"ConditionalUpgrades">>, Data, false),
    case mod_variant(Data) =:= <<"riven">> orelse
         (Elements =/= [] andalso Conditional =/= false andalso Conditional =/= 0)
         orelse lists:any(fun(E) -> E =/= [] end, SubElements) of
        true -> null;
        false -> Elements
    end.

quality_value(Value, Values) when is_integer(Value) ->
    maps:get(Value, Values, null);
quality_value(Value, Values) when is_binary(Value) ->
    quality_value(binary_to_list(Value), Values);
quality_value("QA_NONE", Values) -> maps:get(0, Values);
quality_value("QA_LOW", Values) -> maps:get(1, Values);
quality_value("QA_MEDIUM", Values) -> maps:get(2, Values);
quality_value("QA_HIGH", Values) -> maps:get(3, Values);
quality_value("QA_VERY_HIGH", Values) -> maps:get(4, Values);
quality_value(_Value, _Values) -> null.

ranked_cost(Base, Rank) when is_integer(Base), is_integer(Rank), Rank >= 0 ->
    Base + Rank;
ranked_cost(_Base, _Rank) -> null.

item_for_request(Catalog, Request) ->
    case maps:get(item, Request, undefined) of
        Path when is_binary(Path) ->
            case maps:get(Path, maps:get(items_by_path, Catalog, #{}), undefined) of
                undefined -> {error, unsupported_source_item};
                Item -> {ok, Item}
            end;
        _ -> {error, invalid_source_item}
    end.

item_by_external_id(Catalog, Id) ->
    maps:get(Id, maps:get(items_by_id, Catalog, #{}),
             #{<<"source">> => <<"overframe">>,
               <<"external_id">> => nullable(Id),
               <<"canonical_id">> => null,
               <<"name">> => null,
               <<"class">> => <<"other">>,
               <<"categories">> => []}).

list_params(Item, Request) ->
    Limit = maps:get(limit, Request, 30),
    Offset = maps:get(offset, Request, 0),
    Ordering = ordering(maps:get(ordering, Request, <<"score">>)),
    case is_integer(Limit) andalso Limit >= 1 andalso Limit =< 100 andalso
         is_integer(Offset) andalso Offset >= 0 andalso Ordering =/= error of
        false -> {error, invalid_build_list_options};
        true ->
            Base = [{"item_id", integer_to_list(maps:get(<<"external_id">>, Item))},
                    {"limit", integer_to_list(Limit)},
                    {"offset", integer_to_list(Offset)},
                    {"ordering", binary_to_list(Ordering)}],
            Query = maps:get(query, Request, <<>>),
            Params = case Query of
                Text when is_binary(Text), byte_size(Text) > 0,
                               byte_size(Text) =< 200 ->
                    [{"title", binary_to_list(Text)} | Base];
                <<>> -> Base;
                _ -> invalid
            end,
            case Params of
                invalid -> {error, invalid_build_query};
                _ -> scope_params(maps:get(scope, Request, public), Params)
            end
    end.

scope_params(public, Params) -> {ok, Params, []};
scope_params(<<"public">>, Params) -> {ok, Params, []};
scope_params(favorites, Params) -> authenticated_params([{"favorites", "true"} | Params]);
scope_params(<<"favorites">>, Params) -> authenticated_params([{"favorites", "true"} | Params]);
scope_params(mine, Params) ->
    case current_user_id() of
        {ok, UserId, Headers} ->
            {ok, [{"author_id", integer_to_list(UserId)} | Params], Headers};
        {error, _Reason} = Error -> Error
    end;
scope_params(<<"mine">>, Params) -> scope_params(mine, Params);
scope_params(_Scope, _Params) -> {error, invalid_build_scope}.

authenticated_params(Params) ->
    case wfcli_overframe_account:session_headers() of
        {ok, Headers} -> {ok, Params, Headers};
        {error, _Reason} -> {error, overframe_sign_in_required}
    end.

current_user_id() ->
    case {wfcli_overframe_account:snapshot(),
          wfcli_overframe_account:session_headers()} of
        {{ok, #{<<"authenticated">> := true, <<"profile">> := Profile}},
         {ok, Headers}} ->
            case positive_integer(maps:get(<<"id">>, Profile, undefined)) of
                {ok, Id} -> {ok, Id, Headers};
                error -> {error, invalid_overframe_profile}
            end;
        {{ok, _Account}, _Headers} -> {error, overframe_sign_in_required};
        {{error, Reason}, _Headers} -> {error, Reason}
    end.

ordering(<<"score">>) -> <<"-score">>;
ordering(<<"updated">>) -> <<"-updated">>;
ordering(<<"formas">>) -> <<"formas">>;
ordering(<<"formas_desc">>) -> <<"-formas">>;
ordering(Value) when Value =:= <<"-score">>; Value =:= <<"-updated">>;
                         Value =:= <<"formas">>; Value =:= <<"-formas">> -> Value;
ordering(_Value) -> error.

api_url(Path, []) -> ?API_ROOT ++ Path;
api_url(Path, Params) -> ?API_ROOT ++ Path ++ "?" ++ uri_string:compose_query(Params).

class_matches(<<"all">>, _Class) -> true;
class_matches(<<>>, _Class) -> true;
class_matches(Class, Class) -> true;
class_matches(_Wanted, _Class) -> false.

text_matches(<<>>, _Item) -> true;
text_matches(Query, Item) ->
    Name = string:casefold(maps:get(<<"name">>, Item, <<>>)),
    Path = string:casefold(maps:get(<<"canonical_id">>, Item, <<>>)),
    binary:match(Name, Query) =/= nomatch orelse binary:match(Path, Query) =/= nomatch.

displayable(#{<<"name">> := Name, <<"external_id">> := Id}) ->
    is_binary(Name) andalso byte_size(Name) > 0 andalso
        user_facing_name(Name) andalso is_integer(Id) andalso Id > 0;
displayable(_Item) -> false.

user_facing_name(<<"/", _/binary>>) -> false;
user_facing_name(<<"????">>) -> false;
user_facing_name(_Name) -> true.

item_before(A, B) ->
    string:casefold(maps:get(<<"name">>, A)) =<
        string:casefold(maps:get(<<"name">>, B)).

item_class(Categories, _Record, <<"<ARCHWING> ", _/binary>>) ->
    case lists:member(<<"melee">>, Categories) of
        true -> <<"archmelee">>;
        false -> <<"archwing">>
    end;
item_class(Categories, Record, _Name) ->
    Preferred = [<<"warframe">>, <<"archwing">>, <<"archgun">>, <<"archmelee">>,
                 <<"primary">>, <<"secondary">>, <<"melee">>, <<"companion">>,
                 <<"vehicle">>],
    case [Category || Category <- Preferred, lists:member(Category, Categories)] of
        [Class | _] -> Class;
        [] -> fallback_class(maps:get(<<"tag">>, Record, <<"other">>))
    end.

fallback_class(Tag) when is_binary(Tag) -> string:lowercase(Tag);
fallback_class(_Tag) -> <<"other">>.

display_name(<<"<ARCHWING> ", Name/binary>>) -> Name;
display_name(Name) -> Name.

scope_binary(Value) when is_atom(Value) -> atom_to_binary(Value);
scope_binary(Value) when is_binary(Value) -> Value.

positive_integer(Value) when is_integer(Value), Value > 0 -> {ok, Value};
positive_integer(_Value) -> error.

nullable(undefined) -> null;
nullable(Value) -> Value.

decode_json_map(Json) ->
    try jsone:decode(Json, [{object_format, map}]) of
        Map when is_map(Map) -> {ok, Map};
        Other -> {error, {invalid_database_root, Other}}
    catch error:Reason -> {error, {invalid_database_json, Reason}}
    end.

take_js_string(<<>>, _Acc) -> {error, unterminated_js_string};
take_js_string(Binary, Acc) ->
    case binary:match(Binary, [<<"\\">>, <<"'">>]) of
        nomatch -> {error, unterminated_js_string};
        {Position, 1} ->
            Prefix = binary:part(Binary, 0, Position),
            TailPosition = Position + 1,
            Tail = binary:part(Binary, TailPosition,
                               byte_size(Binary) - TailPosition),
            case binary:at(Binary, Position) of
                $' -> {ok, iolist_to_binary(lists:reverse([Prefix | Acc]))};
                $\\ when byte_size(Tail) > 0 ->
                    Escaped = binary:part(Tail, 0, 1),
                    Rest = binary:part(Tail, 1, byte_size(Tail) - 1),
                    take_js_string(Rest, [Escaped, <<"\\">>, Prefix | Acc]);
                $\\ -> {error, unterminated_js_escape}
            end
    end.

unescape_js(<<>>, Acc) -> {ok, iolist_to_binary(lists:reverse(Acc))};
unescape_js(Binary, Acc) ->
    case binary:match(Binary, <<"\\">>) of
        nomatch -> {ok, iolist_to_binary(lists:reverse([Binary | Acc]))};
        {Position, 1} ->
            Prefix = binary:part(Binary, 0, Position),
            EscapeStart = Position + 1,
            Tail = binary:part(Binary, EscapeStart,
                               byte_size(Binary) - EscapeStart),
            case decode_js_escape(Tail) of
                {ok, Value, Rest} -> unescape_js(Rest, [Value, Prefix | Acc]);
                {error, _Reason} = Error -> Error
            end
    end.

decode_js_escape(<<>>) -> {error, unterminated_js_escape};
decode_js_escape(<<$\\, Rest/binary>>) -> {ok, <<$\\>>, Rest};
decode_js_escape(<<$', Rest/binary>>) -> {ok, <<$'>>, Rest};
decode_js_escape(<<$", Rest/binary>>) -> {ok, <<$">>, Rest};
decode_js_escape(<<$b, Rest/binary>>) -> {ok, <<8>>, Rest};
decode_js_escape(<<$f, Rest/binary>>) -> {ok, <<12>>, Rest};
decode_js_escape(<<$n, Rest/binary>>) -> {ok, <<10>>, Rest};
decode_js_escape(<<$r, Rest/binary>>) -> {ok, <<13>>, Rest};
decode_js_escape(<<$t, Rest/binary>>) -> {ok, <<9>>, Rest};
decode_js_escape(<<$v, Rest/binary>>) -> {ok, <<11>>, Rest};
decode_js_escape(<<$0, Rest/binary>>) -> {ok, <<0>>, Rest};
decode_js_escape(<<$\n, Rest/binary>>) -> {ok, <<>>, Rest};
decode_js_escape(<<$\r, $\n, Rest/binary>>) -> {ok, <<>>, Rest};
decode_js_escape(<<$\r, Rest/binary>>) -> {ok, <<>>, Rest};
decode_js_escape(<<$x, A, B, Rest/binary>>) ->
    case hex_codepoint([A, B]) of
        {ok, Codepoint} -> {ok, <<Codepoint>>, Rest};
        error -> {error, invalid_js_hex_escape}
    end;
decode_js_escape(<<$u, A, B, C, D, Rest/binary>>) ->
    case hex_codepoint([A, B, C, D]) of
        {ok, High} when High >= 16#D800, High =< 16#DBFF ->
            decode_surrogate(High, Rest);
        {ok, Codepoint} ->
            {ok, unicode:characters_to_binary([Codepoint]), Rest};
        error -> {error, invalid_js_unicode_escape}
    end;
decode_js_escape(<<Character, Rest/binary>>) -> {ok, <<Character>>, Rest}.

decode_surrogate(High, <<$\\, $u, A, B, C, D, Rest/binary>>) ->
    case hex_codepoint([A, B, C, D]) of
        {ok, Low} when Low >= 16#DC00, Low =< 16#DFFF ->
            Codepoint = 16#10000 + ((High - 16#D800) bsl 10) + (Low - 16#DC00),
            {ok, unicode:characters_to_binary([Codepoint]), Rest};
        _ -> {error, invalid_js_surrogate}
    end;
decode_surrogate(_High, _Rest) -> {error, missing_js_low_surrogate}.

hex_codepoint(Bytes) ->
    try {ok, lists:foldl(fun(Byte, Acc) -> Acc * 16 + hex_digit(Byte) end,
                         0, Bytes)}
    catch error:badarg -> error
    end.

hex_digit(Byte) when Byte >= $0, Byte =< $9 -> Byte - $0;
hex_digit(Byte) when Byte >= $a, Byte =< $f -> Byte - $a + 10;
hex_digit(Byte) when Byte >= $A, Byte =< $F -> Byte - $A + 10;
hex_digit(_Byte) -> error(badarg).

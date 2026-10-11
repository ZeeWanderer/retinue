-module(wfcli_wfcd).

-export([update/0]).
-ifdef(TEST).
-export([fetch/0]).
-endif.

-define(REGISTRY, "https://registry.npmjs.org/@wfcd/items/latest").
-define(MAX_DOWNLOAD, 64 * 1024 * 1024).

update() ->
    case fetch() of
        {ok, Items, Meta} ->
            Enemies = [Item || Item <- Items, category(Item) =:= <<"Enemy">>],
            case wfcli_item_catalog:store(Items, Meta) of
                ok -> wfcli_knowledge:store_wfcd(Enemies, Meta);
                Error -> Error
            end;
        Error -> Error
    end.

fetch() ->
    application:ensure_all_started(inets),
    application:ensure_all_started(ssl),
    case attempt(fun latest_version/0) of
        {error, Reason} -> {error, {wfcd_version_failed, Reason}};
        Version -> fetch_mirrors([unpkg, jsdelivr], Version, [])
    end.

fetch_mirrors([], Version, Errors) ->
    {error, {wfcd_sources_failed, Version, lists:reverse(Errors)}};
fetch_mirrors([Mirror | Rest], Version, Errors) ->
    case attempt(fun() -> catalog(Mirror, Version) end) of
        {ok, _, _} = Result -> Result;
        {error, Reason} ->
            logger:warning(#{event => wfcd_mirror_failed, mirror => Mirror,
                             version => Version, reason => Reason}),
            fetch_mirrors(Rest, Version, [{Mirror, Reason} | Errors])
    end.

attempt(Fun) ->
    try Fun()
    catch
        throw:{wfcd, Reason} -> {error, Reason};
        error:Reason -> {error, {invalid_wfcd_data, Reason}}
    end.

latest_version() ->
    Body = get(?REGISTRY, 1024 * 1024),
    validate(?REGISTRY, fun() ->
        Metadata = decode(Body),
        ensure(is_map(Metadata) andalso
               maps:get(<<"name">>, Metadata, undefined) =:= <<"@wfcd/items">>,
               invalid_package_identity),
        Version = maps:get(<<"version">>, Metadata, undefined),
        ensure(is_binary(Version) andalso
               re:run(Version, <<"^[0-9]+\\.[0-9]+\\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\\+[0-9A-Za-z.-]+)?$">>,
                      [{capture, none}]) =:= match, invalid_package_version),
        Version
    end).

catalog(Mirror, Version) ->
    {Base, Url} = urls(Mirror, Version),
    Body = get(Url, 1024 * 1024),
    Files = validate(Url, fun() -> manifest(Mirror, decode(Body), Version) end),
    case Mirror of
        jsdelivr ->
            Package = lists:keyfind(<<"/package.json">>, 1, Files),
            PackageBody = file_body(Base, Package),
            validate(Url, fun() ->
                Identity = decode(PackageBody),
                ensure(is_map(Identity) andalso
                       maps:get(<<"name">>, Identity, undefined) =:= <<"@wfcd/items">> andalso
                       maps:get(<<"version">>, Identity, undefined) =:= Version,
                       package_identity_mismatch)
            end);
        unpkg -> ok
    end,
    Selected = validate(Url, fun() -> select_files(Files) end),
    Items = lists:append([begin
        FileBody = file_body(Base, File),
        validate(file_url(Base, element(1, File)), fun() -> decode_items(FileBody) end)
    end || File <- Selected]),
    Normalized = validate(Url, fun() -> normalize(Items) end),
    Signature = jsone:encode([[Path, Hash] || {Path, _, Hash} <- Selected]),
    {ok, Normalized, (metadata(list_to_binary(Url), Signature))#{<<"packageVersion">> => Version}}.

urls(unpkg, Version) ->
    Base = "https://unpkg.com/@wfcd/items@" ++ binary_to_list(Version),
    {Base, Base ++ "/data/json/?meta"};
urls(jsdelivr, Version) ->
    Package = "@wfcd/items@" ++ binary_to_list(Version),
    {"https://cdn.jsdelivr.net/npm/" ++ Package,
     "https://data.jsdelivr.com/v1/package/npm/" ++ Package ++ "/flat"}.

manifest(unpkg, #{<<"package">> := <<"@wfcd/items">>, <<"version">> := Version,
                  <<"prefix">> := <<"/data/json/">>, <<"files">> := Files}, Version)
  when is_list(Files) ->
    [descriptor(maps:get(<<"path">>, F), maps:get(<<"size">>, F),
                maps:get(<<"integrity">>, F)) || F <- Files];
manifest(jsdelivr, #{<<"files">> := Files}, _Version) when is_list(Files) ->
    [descriptor(maps:get(<<"name">>, F), maps:get(<<"size">>, F),
                <<"sha256-", (maps:get(<<"hash">>, F))/binary>>) || F <- Files];
manifest(_, _, _) -> fail(invalid_file_manifest).

descriptor(Path, Size, <<"sha256-", Hash/binary>>)
  when is_binary(Path), is_integer(Size), Size >= 0 ->
    ensure(byte_size(base64:decode(Hash)) =:= 32, invalid_file_hash),
    {Path, Size, Hash};
descriptor(_, _, _) -> fail(invalid_file_descriptor).

select_files(Files) ->
    Names = [Path || {Path, _, _} <- Files],
    ensure(length(Names) =:= length(lists:usort(Names)), duplicate_catalog_file),
    Selected = lists:sort([F || {Path, _, _} = F <- Files, data_file(Path)]),
    ensure(Selected =/= [], no_catalog_files),
    ensure(length(Selected) =< 1024, catalog_file_limit),
    ensure(lists:sum([Size || {_, Size, _} <- Selected]) =< ?MAX_DOWNLOAD,
           catalog_size_limit),
    Selected.

data_file(Path) ->
    filename:dirname(Path) =:= <<"/data/json">> andalso
        filename:extension(Path) =:= <<".json">> andalso
        filename:basename(Path) =/= <<"All.json">>.

file_body(Base, {Path, Size, Hash}) ->
    Url = file_url(Base, Path),
    Body = get(Url, min(Size, ?MAX_DOWNLOAD)),
    ensure(byte_size(Body) =:= Size, {file_size_mismatch, Url, Size, byte_size(Body)}),
    ensure(base64:encode(crypto:hash(sha256, Body)) =:= Hash, {file_integrity_mismatch, Url}),
    Body;
file_body(_, _) -> fail(missing_package_identity).

file_url(Base, Path) -> Base ++ binary_to_list(uri_string:quote(Path, "/")).

normalize(Items) ->
    Categories = lists:usort([category(Item) || Item <- Items]),
    Missing = [<<"Warframes">>, <<"Mods">>, <<"Enemy">>, <<"Relics">>] -- Categories,
    ensure(Missing =:= [], {incomplete_catalog, Missing}),
    {Components, Standalone} = lists:partition(
        fun(Item) -> category(Item) =:= <<"Components">> end, Items),
    Index = maps:from_list([{maps:get(<<"uniqueName">>, Item), Item}
                           || Item <- Components ++ Standalone]),
    %% Components remain nested so their parent identity wins in player joins.
    [resolve_components(Item, Index) || Item <- Standalone].

resolve_components(#{<<"components">> := Refs} = Item, Index) ->
    ensure(is_list(Refs), invalid_components),
    Item#{<<"components">> => [resolve_component(Ref, Index) || Ref <- Refs]};
resolve_components(Item, _Index) -> Item.

resolve_component(#{<<"uniqueName">> := Identity} = Ref, Index) ->
    case maps:is_key(<<"name">>, Ref) orelse maps:is_key(<<"imageName">>, Ref)
         orelse maps:is_key(<<"drops">>, Ref) of
        true -> Ref;
        false ->
            case maps:find(Identity, Index) of
                {ok, Entry} ->
                    Expanded = maps:merge(maps:remove(<<"parentUniqueNames">>, Entry), Ref),
                    Expanded#{<<"itemCount">> => maps:get(<<"itemCount">>, Ref, 1)};
                error -> fail({unresolved_component, Identity})
            end
    end;
resolve_component(_, _) -> fail(invalid_component).

decode_items(Body) ->
    Items = decode(Body),
    ensure(is_list(Items), invalid_catalog_root),
    lists:foreach(fun(Item) ->
        Id = case is_map(Item) of true -> maps:get(<<"uniqueName">>, Item, undefined); false -> undefined end,
        ensure(valid_item(Item), {invalid_catalog_item, Id})
    end, Items),
    Items.

valid_item(#{<<"uniqueName">> := Id, <<"name">> := Name}) ->
    is_binary(Id) andalso byte_size(Id) > 0 andalso
        is_binary(Name) andalso byte_size(Name) > 0;
valid_item(_) -> false.

category(Item) -> maps:get(<<"category">>, Item, undefined).

metadata(Url, Body) ->
    #{<<"source">> => Url,
      <<"version">> => binary:encode_hex(crypto:hash(sha256, Body), lowercase),
      <<"fetchedAt">> => erlang:system_time(second)}.

decode(Body) ->
    try jsone:decode(Body, [{object_format, map}])
    catch error:_ -> fail(invalid_json)
    end.

validate(Location, Fun) ->
    try Fun()
    catch
        throw:{wfcd, Reason} -> fail({invalid_data, Location, Reason});
        error:Reason -> fail({invalid_data, Location, Reason})
    end.

get(Url, Limit) ->
    Headers = [{"user-agent", "wfcli (+https://github.com/ZeeWanderer/wfcli)"}] ++
              case Url of ?REGISTRY -> [{"cache-control", "no-cache"}]; _ -> [] end,
    Result = case application:get_env(retinue_daemon, wfcd_http_fun) of
                 {ok, Fun} when is_function(Fun, 2) -> Fun(Url, Headers);
                 _ -> http_get(Url, Headers)
             end,
    case Result of
        {ok, 200, Body} when is_binary(Body), byte_size(Body) =< Limit -> Body;
        {ok, 200, _} -> fail({download_size_limit, Url});
        {ok, Status, _} -> fail({http_status, Url, Status});
        {error, Reason} -> fail({http_failed, Url, Reason})
    end.

http_get(Url, Headers) ->
    case httpc:request(get, {Url, Headers},
                       [{connect_timeout, 10000}, {timeout, 120000}],
                       [{body_format, binary}]) of
        {ok, {{_, Status, _}, _, Body}} -> {ok, Status, Body};
        {error, Reason} -> {error, Reason}
    end.

ensure(true, _) -> ok;
ensure(false, Reason) -> fail(Reason).

fail(Reason) -> throw({wfcd, Reason}).

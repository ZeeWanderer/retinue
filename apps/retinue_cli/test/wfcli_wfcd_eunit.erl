-module(wfcli_wfcd_eunit).
-include_lib("eunit/include/eunit.hrl").

-define(REGISTRY, "https://registry.npmjs.org/@wfcd/items/latest").
-define(VERSION, <<"1.2.3">>).

published_json_resolves_components_test() ->
    with_http(http(?VERSION, files(catalog())), fun() ->
        {ok, Items, Meta} = wfcli_wfcd:fetch(),
        ?assertEqual(5, length(Items)),
        ?assertEqual(list_to_binary(manifest_url(unpkg, ?VERSION)), maps:get(<<"source">>, Meta)),
        ?assertEqual(?VERSION, maps:get(<<"packageVersion">>, Meta)),
        [Frame] = [Item || #{<<"category">> := <<"Warframes">>} = Item <- Items],
        [Part, Resource, Resolved] = maps:get(<<"components">>, Frame),
        ?assertEqual(<<"Chassis">>, maps:get(<<"name">>, Part)),
        ?assertEqual(<<"chassis.png">>, maps:get(<<"imageName">>, Part)),
        ?assertEqual(2, maps:get(<<"itemCount">>, Part)),
        ?assertNot(maps:is_key(<<"parentUniqueNames">>, Part)),
        ?assertEqual(<<"Resource">>, maps:get(<<"name">>, Resource)),
        ?assertEqual(7, maps:get(<<"itemCount">>, Resource)),
        ?assertEqual(<<"Inline part">>, maps:get(<<"name">>, Resolved)),
        ?assertEqual(<<"inline.png">>, maps:get(<<"imageName">>, Resolved)),
        Compacted = [wfcli_item_catalog:compact(Item) || Item <- Items],
        Index = wfcli_item_catalog:index(Compacted),
        ?assertMatch(#{<<"component">> := true, <<"parentName">> := <<"Frame">>},
                     wfcli_item_catalog:lookup(<<"/part">>, Index)),
        ?assertMatch(#{<<"component">> := false, <<"name">> := <<"Resource">>},
                     wfcli_item_catalog:lookup(<<"/resource">>, Index))
    end).

each_refresh_discovers_latest_version_and_new_categories_test() ->
    with_cache(fun() ->
        with_http(http(?VERSION, files(catalog())), fun() ->
            ?assertEqual(ok, wfcli_wfcd:update())
        end),
        New = item("New item", "New category", "/new"),
        Catalog = (catalog())#{"New category" => [New]},
        with_http(http(<<"2.0.0">>, files(Catalog)), fun() ->
            ?assertEqual(ok, wfcli_wfcd:update()),
            {ok, Body} = file:read_file(wfcli_paths:cache_file("WFCDItems.json")),
            Saved = jsone:decode(Body),
            ?assertEqual(<<"2.0.0">>, maps:get(<<"packageVersion">>, Saved)),
            ?assert(lists:any(fun(I) -> maps:get(<<"uniqueName">>, I) =:= <<"/new">> end,
                             maps:get(<<"entries">>, Saved)))
        end)
    end).

refresh_pins_one_version_even_if_latest_changes_mid_download_test() ->
    Http = http(?VERSION, files(catalog())),
    put(registry_reads, 0),
    try
        with_http(fun(?REGISTRY = Url, Headers) ->
                      ?assertEqual("no-cache", proplists:get_value("cache-control", Headers)),
                      put(registry_reads, get(registry_reads) + 1),
                      case get(registry_reads) of
                          1 -> Http(Url, Headers);
                          _ -> {ok, 200, registry(<<"2.0.0">>)}
                      end;
                     (Url, Headers) -> Http(Url, Headers)
                  end,
                  fun() ->
                      {ok, _, Meta} = wfcli_wfcd:fetch(),
                      ?assertEqual(?VERSION, maps:get(<<"packageVersion">>, Meta)),
                      ?assertEqual(1, get(registry_reads))
                  end)
    after erase(registry_reads)
    end.

failed_primary_uses_same_version_mirror_test() ->
    Http = http(?VERSION, files(catalog())),
    with_http(fun("https://unpkg.com/" ++ _, _) -> {ok, 404, <<>>};
                 (Url, Headers) -> Http(Url, Headers)
              end,
              fun() ->
                  {ok, _, Meta} = wfcli_wfcd:fetch(),
                  ?assertEqual(list_to_binary(manifest_url(jsdelivr, ?VERSION)),
                               maps:get(<<"source">>, Meta)),
                  ?assertEqual(?VERSION, maps:get(<<"packageVersion">>, Meta))
              end).

mirror_does_not_change_content_version_test() ->
    Http = http(?VERSION, files(catalog())),
    {ok, _, Primary} = with_http(Http, fun wfcli_wfcd:fetch/0),
    {ok, _, Secondary} = with_http(
        fun("https://unpkg.com/" ++ _, _) -> {error, timeout};
           (Url, Headers) -> Http(Url, Headers)
        end, fun wfcli_wfcd:fetch/0),
    ?assertEqual(maps:get(<<"version">>, Primary), maps:get(<<"version">>, Secondary)).

fallback_cannot_return_an_older_package_test() ->
    Old = registry(<<"1.0.0">>),
    Files = (files(catalog()))#{<<"/package.json">> => Old},
    Http = http(?VERSION, Files),
    Manifest = manifest_url(jsdelivr, ?VERSION),
    Package = base(jsdelivr, ?VERSION) ++ "/package.json",
    with_http(fun("https://unpkg.com/" ++ _, _) -> {ok, 404, <<>>};
                 (Url, _) when Url =:= Manifest -> {ok, 200, manifest(jsdelivr, ?VERSION, Files)};
                 (Url, _) when Url =:= Package -> {ok, 200, Old};
                 (Url, Headers) -> Http(Url, Headers)
              end, fun() ->
                  ?assertMatch({error, {wfcd_sources_failed, ?VERSION,
                                       [{unpkg, _}, {jsdelivr, {invalid_data, _, package_identity_mismatch}}]}},
                               wfcli_wfcd:fetch())
              end).

invalid_primary_uses_current_mirror_test_() ->
    [?_test(begin
        Files = files(catalog()),
        Http = http(?VERSION, Files),
        Manifest = manifest_url(unpkg, ?VERSION),
        Mods = base(unpkg, ?VERSION) ++ "/data/json/Mods.json",
        with_http(fun(Url, Headers) ->
            case {Url, Fault} of
                {Manifest, old_version} -> {ok, 200, manifest(unpkg, <<"1.0.0">>, Files)};
                {Mods, checksum} ->
                    Body = maps:get(<<"/data/json/Mods.json">>, Files),
                    {ok, 200, binary:replace(Body, <<"Mod">>, <<"Bad">>)};
                {Mods, download} -> {ok, 503, <<>>};
                _ -> Http(Url, Headers)
            end
        end, fun() ->
            {ok, _, Meta} = wfcli_wfcd:fetch(),
            ?assertEqual(list_to_binary(manifest_url(jsdelivr, ?VERSION)),
                         maps:get(<<"source">>, Meta)),
            ?assertEqual(?VERSION, maps:get(<<"packageVersion">>, Meta))
        end)
    end) || Fault <- [old_version, checksum, download]].

invalid_layout_preserves_existing_caches_test_() ->
    [?_test(with_cache(fun() ->
        Paths = cache_paths(),
        lists:foreach(fun(Path) -> ok = file:write_file(Path, <<"existing snapshot">>) end, Paths),
        Files = files(catalog()),
        Bad = case Fault of
            missing_category -> maps:remove(<<"/data/json/Mods.json">>, Files);
            unresolved_component -> maps:remove(<<"/data/json/Components.json">>, Files);
            _ -> Files#{<<"/data/json/Mods.json">> := Fault}
        end,
        with_http(http(?VERSION, Bad), fun() ->
            ?assertMatch({error, {wfcd_sources_failed, ?VERSION,
                                 [{unpkg, {invalid_data, _, _}},
                                  {jsdelivr, {invalid_data, _, _}}]}}, wfcli_wfcd:update()),
            lists:foreach(fun(Path) ->
                ?assertEqual({ok, <<"existing snapshot">>}, file:read_file(Path))
            end, Paths)
        end)
    end)) || Fault <- [missing_category, unresolved_component, <<"not json">>, <<"[]">>,
                       <<"{}">>, <<"[false]">>, <<"[{\"uniqueName\":\"/bad\"}]">>]].

latest_version_failure_never_fetches_an_unversioned_fallback_test() ->
    with_cache(fun() ->
        Paths = cache_paths(),
        lists:foreach(fun(Path) -> ok = file:write_file(Path, <<"existing snapshot">>) end, Paths),
        with_http(fun(?REGISTRY, _) -> {error, timeout};
                     (Url, _) -> self() ! {unexpected_fetch, Url}, {error, blocked}
                  end,
                  fun() ->
                      ?assertMatch({error, {wfcd_version_failed, {http_failed, ?REGISTRY, timeout}}},
                                   wfcli_wfcd:update()),
                      receive {unexpected_fetch, _} -> ?assert(false) after 0 -> ok end,
                      lists:foreach(fun(Path) ->
                          ?assertEqual({ok, <<"existing snapshot">>}, file:read_file(Path))
                      end, Paths)
                  end)
    end).

invalid_version_is_not_used_in_urls_test_() ->
    [?_test(with_http(fun(?REGISTRY, _) -> {ok, 200, registry(Version)};
                        (Url, _) -> self() ! {unexpected_fetch, Url}, {error, blocked}
                     end,
                     fun() ->
                         ?assertMatch({error, {wfcd_version_failed, {invalid_data, _, _}}},
                                      wfcli_wfcd:fetch()),
                         receive {unexpected_fetch, _} -> ?assert(false) after 0 -> ok end
                     end)) || Version <- [<<"latest">>, <<"1.2.3/../old">>, <<"1.2.3?x">>, null]].

duplicate_manifest_file_is_rejected_test() ->
    Files = files(catalog()),
    Http = http(?VERSION, Files),
    Unpkg = manifest_url(unpkg, ?VERSION),
    Jsdelivr = manifest_url(jsdelivr, ?VERSION),
    with_http(fun(Url, Headers) ->
        case Url =:= Unpkg orelse Url =:= Jsdelivr of
            true ->
                {ok, 200, Body} = Http(Url, Headers),
                Data = jsone:decode(Body),
                [First | _] = Entries = maps:get(<<"files">>, Data),
                {ok, 200, jsone:encode(Data#{<<"files">> := [First | Entries]})};
            false -> Http(Url, Headers)
        end
    end, fun() ->
        ?assertMatch({error, {wfcd_sources_failed, _,
                             [{_, {invalid_data, _, duplicate_catalog_file}},
                              {_, {invalid_data, _, duplicate_catalog_file}}]}}, wfcli_wfcd:fetch())
    end).

update_writes_one_snapshot_to_both_catalogs_test() ->
    with_cache(fun() ->
        with_http(http(?VERSION, files(catalog())), fun() ->
            ?assertEqual(ok, wfcli_wfcd:update()),
            {ok, ItemBody} = file:read_file(wfcli_paths:cache_file("WFCDItems.json")),
            {ok, EnemyBody} = file:read_file(wfcli_paths:cache_file("WFCDEnemy.json")),
            Items = jsone:decode(ItemBody),
            Enemies = jsone:decode(EnemyBody),
            ?assertEqual(maps:remove(<<"entries">>, Items), maps:remove(<<"entries">>, Enemies)),
            ?assertMatch([#{<<"name">> := <<"Enemy">>}], maps:get(<<"entries">>, Enemies)),
            [Frame] = [I || #{<<"name">> := <<"Frame">>} = I <- maps:get(<<"entries">>, Items)],
            [Part | _] = maps:get(<<"components">>, Frame),
            ?assertEqual(2, maps:get(<<"itemCount">>, Part)),
            ?assertMatch([#{<<"location">> := <<"Lith A1 Relic">>}], maps:get(<<"drops">>, Part))
        end)
    end).

catalog() ->
    #{"Warframes" => [(item("Frame", "Warframes", "/frame"))#{<<"components">> => [
         #{<<"uniqueName">> => <<"/part">>, <<"itemCount">> => 2},
         #{<<"uniqueName">> => <<"/resource">>, <<"itemCount">> => 7},
         #{<<"uniqueName">> => <<"/inline">>, <<"name">> => <<"Inline part">>,
           <<"imageName">> => <<"inline.png">>, <<"itemCount">> => 1}]}],
      "Components" => [(item("Chassis", "Components", "/part"))#{
          <<"imageName">> => <<"chassis.png">>, <<"parentUniqueNames">> => [<<"/frame">>],
          <<"drops">> => [#{<<"location">> => <<"Lith A1 Relic">>, <<"chance">> => 0.1}]},
          item("Duplicate resource", "Components", "/resource")],
      "Resources" => [item("Resource", "Resources", "/resource")],
      "Mods" => [item("Mod", "Mods", "/mod")],
      "Relics" => [item("Relic", "Relics", "/relic")],
      "Enemy" => [item("Enemy", "Enemy", "/enemy")]}.

item(Name, Category, Id) ->
    #{<<"name">> => list_to_binary(Name), <<"category">> => list_to_binary(Category),
      <<"uniqueName">> => list_to_binary(Id)}.

files(Catalog) ->
    maps:from_list([{list_to_binary("/data/json/" ++ Name ++ ".json"), jsone:encode(Items)}
                   || {Name, Items} <- maps:to_list(Catalog)]).

registry(Version) -> jsone:encode(#{<<"name">> => <<"@wfcd/items">>, <<"version">> => Version}).

manifest(Mirror, Version, Files) ->
    Entries = [case Mirror of
        unpkg -> #{<<"path">> => Path, <<"size">> => byte_size(Body),
                   <<"integrity">> => <<"sha256-", (hash(Body))/binary>>};
        jsdelivr -> #{<<"name">> => Path, <<"size">> => byte_size(Body), <<"hash">> => hash(Body)}
    end || {Path, Body} <- maps:to_list(Files)],
    jsone:encode(case Mirror of
        unpkg -> #{<<"package">> => <<"@wfcd/items">>, <<"version">> => Version,
                   <<"prefix">> => <<"/data/json/">>, <<"files">> => Entries};
        jsdelivr -> #{<<"files">> => Entries}
    end).

hash(Body) -> base64:encode(crypto:hash(sha256, Body)).

http(Version, Files0) ->
    Files = Files0#{<<"/package.json">> => registry(Version),
                   <<"/data/json/i18n/en.json">> => <<"ignored locale">>,
                   <<"/data/json/All.json">> => <<"ignored aggregate">>,
                   <<"/index.js">> => <<"not fetched">>},
    Unpkg = manifest_url(unpkg, Version),
    Jsdelivr = manifest_url(jsdelivr, Version),
    fun(?REGISTRY, _) -> {ok, 200, registry(Version)};
       (Url, _) when Url =:= Unpkg -> {ok, 200, manifest(unpkg, Version, Files)};
       (Url, _) when Url =:= Jsdelivr -> {ok, 200, manifest(jsdelivr, Version, Files)};
       (Url, _) ->
            Matches = [list_to_binary(uri_string:unquote(lists:nthtail(length(Base), Url)))
                       || Base <- [base(M, Version) || M <- [unpkg, jsdelivr]],
                          lists:prefix(Base ++ "/", Url)],
            case Matches of
                [Path] ->
                    ?assertNot(lists:member(Path, [<<"/index.js">>, <<"/data/json/i18n/en.json">>,
                                                  <<"/data/json/All.json">>])),
                    case maps:find(Path, Files) of
                        {ok, Body} -> {ok, 200, Body};
                        error -> {ok, 404, <<>>}
                    end;
                [] -> error({unexpected_fetch, Url})
            end
    end.

base(unpkg, Version) -> "https://unpkg.com/@wfcd/items@" ++ binary_to_list(Version);
base(jsdelivr, Version) -> "https://cdn.jsdelivr.net/npm/@wfcd/items@" ++ binary_to_list(Version).

manifest_url(unpkg, Version) -> base(unpkg, Version) ++ "/data/json/?meta";
manifest_url(jsdelivr, Version) ->
    "https://data.jsdelivr.com/v1/package/npm/@wfcd/items@" ++ binary_to_list(Version) ++ "/flat".

with_http(Fun, Test) ->
    application:set_env(retinue_daemon, wfcd_http_fun, Fun),
    try Test()
    after application:unset_env(retinue_daemon, wfcd_http_fun)
    end.

cache_paths() -> [wfcli_paths:cache_file(Name) || Name <- ["WFCDItems.json", "WFCDEnemy.json"]].

with_cache(Test) ->
    Root = filename:join("/tmp", "wfcli-wfcd-cache-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    Previous = os:getenv("XDG_CACHE_HOME"),
    os:putenv("XDG_CACHE_HOME", Root),
    ok = filelib:ensure_dir(wfcli_paths:cache_file("placeholder")),
    try Test()
    after
        case Previous of
            false -> os:unsetenv("XDG_CACHE_HOME");
            _ -> os:putenv("XDG_CACHE_HOME", Previous)
        end,
        file:del_dir_r(Root)
    end.

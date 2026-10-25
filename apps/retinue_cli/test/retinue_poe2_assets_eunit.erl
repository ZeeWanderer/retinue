-module(retinue_poe2_assets_eunit).
-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

catalog_and_source_validation_test() ->
    Entries = [entry(<<"future-coin">>, <<"/gen/image/new.png">>),
               entry(<<"perfect-exalted-orb">>, <<"https://web.poecdn.com/image/coin.png">>),
               entry(<<"bad">>, <<"https://other.invalid/image.png">>)],
    {ok, Index} = retinue_poe2_assets:index(#{<<"result">> => [#{<<"entries">> => Entries}]}),
    ?assertEqual(2, map_size(Index)),
    ?assertEqual(<<"III">>, maps:get(<<"tier">>, maps:get(<<"perfect-exalted-orb">>, Index))),
    lists:foreach(fun(Path) -> ?assertEqual(error, retinue_poe2_assets:image_path(Path)) end,
        [<<"//other.invalid/image/a">>, <<"/image/../secret">>, <<"file:///tmp/a.png">>, <<"/image/a\r\nx:1">>]),
    ?assertMatch({error, _}, retinue_poe2_assets:index(#{<<"result">> => []})),
    ?assertMatch({ok, _}, retinue_poe2_assets:png(png(64))),
    ?assertMatch({error, _}, retinue_poe2_assets:png(png(10000))),
    ?assertMatch({error, _}, retinue_poe2_assets:png(<<"<html>not an image</html>">>)).

stale_delivery_coalesces_and_retains_good_image_test() ->
    Parent = self(),
    Data = #{<<"id">> => <<"exalted">>, <<"path">> => <<"cached.png">>},
    Loader = fun(_, Server) ->
        Parent ! {loading, self()},
        receive cached -> Server ! {cached, self(), Data} end,
        receive finish -> {error, unavailable} end
    end,
    {ok, Server} = retinue_poe2_assets:start_link(#{name => undefined, loader => Loader}),
    try
        spawn(fun() -> Parent ! {first, retinue_poe2_assets:resolve(Server, <<"exalted">>)} end),
        Worker = receive {loading, Pid} -> Pid after 1000 -> error(no_loader) end,
        spawn(fun() -> Parent ! {second, retinue_poe2_assets:resolve(Server, <<"exalted">>)} end),
        Worker ! cached,
        receive {first, Reply} -> ?assertEqual({ok, Data}, Reply) after 1000 -> error(no_stale_reply) end,
        receive {second, Reply2} -> ?assertEqual({ok, Data}, Reply2) after 1000 -> error(no_coalesced_reply) end,
        ?assertEqual({ok, Data}, retinue_poe2_assets:resolve(Server, <<"exalted">>)),
        ?assertEqual(1, map_size(maps:get(workers, sys:get_state(Server)))),
        Worker ! finish,
        await_idle(Server, 100),
        ?assertEqual({ok, Data}, retinue_poe2_assets:resolve(Server, <<"exalted">>)),
        ?assertMatch(#{last_error := unavailable}, maps:get(<<"exalted">>, maps:get(entries, sys:get_state(Server)))),
        sys:replace_state(Server, fun(S) -> S#{loader => fun(_, _) -> {ok, Data} end, entries => #{}} end),
        ?assertEqual({ok, Data}, retinue_poe2_assets:resolve(Server, <<"exalted">>)),
        ?assertMatch(#{last_error := undefined}, maps:get(<<"exalted">>, maps:get(entries, sys:get_state(Server))))
    after gen_server:stop(Server) end.

disk_cache_and_digest_changes_test() ->
    Root = filename:join("/tmp", "retinue-currency-" ++ integer_to_list(erlang:unique_integer([positive]))),
    Parent = self(),
    Catalog = fun() -> {ok, iolist_to_binary(json:encode(#{<<"result">> => [#{<<"entries">> => [
        entry(<<"exalted">>, <<"/image/one.png">>), entry(<<"perfect-exalted-orb">>, <<"/image/one.png">>)]}]}))} end,
    Opts = #{root => Root, catalog => Catalog, image => fun(_) -> {ok, png(64)} end},
    try
        {ok, First} = retinue_poe2_assets:load(<<"exalted">>, Parent, Opts),
        ?assert(filelib:is_file(maps:get(<<"path">>, First))),
        Offline = Opts#{catalog => fun() -> error(unexpected_fetch) end, image => fun(_) -> error(unexpected_fetch) end},
        ?assertEqual({ok, First}, retinue_poe2_assets:load(<<"exalted">>, Parent, Offline)),
        receive {cached, _, First} -> ok after 0 -> error(no_disk_stale_reply) end,
        {ok, Tiered} = retinue_poe2_assets:load(<<"perfect-exalted-orb">>, Parent, Offline),
        ?assertEqual(maps:get(<<"path">>, First), maps:get(<<"path">>, Tiered)),
        ?assertEqual(<<"III">>, maps:get(<<"tier">>, Tiered)),
        receive {cached, _, Tiered} -> ok after 0 -> error(no_shared_source_reply) end,
        [Source] = filelib:wildcard(filename:join([Root, "sources", "*.json"])),
        age(Source, 8 * 86400),
        {ok, Second} = retinue_poe2_assets:load(<<"exalted">>, Parent, Opts#{image => fun(_) -> {ok, png(32)} end}),
        receive {cached, _, First} -> ok after 0 -> error(no_prior_identity) end,
        ?assertNotEqual(maps:get(<<"path">>, First), maps:get(<<"path">>, Second)),
        ?assertNotEqual(maps:get(<<"digest">>, First), maps:get(<<"digest">>, Second)),
        {ok, OldPixels} = file:read_file(maps:get(<<"path">>, First)),
        ?assertEqual({ok, maps:get(<<"digest">>, First)}, retinue_poe2_assets:png(OldPixels)),
        {ok, MetaBytes} = file:read_file(filename:join(Root, "static.json")),
        {ok, OldIndex} = retinue_poe2_assets:index(json:decode(MetaBytes)),
        ?assert(maps:is_key(<<"exalted">>, OldIndex)),
        ok = file:write_file(filename:join(Root, "static.json"), json:encode(#{<<"result">> => [#{<<"entries">> => [entry(<<"exalted">>, <<"/image/two.png">>)]}]})),
        {ok, Third} = retinue_poe2_assets:load(<<"exalted">>, Parent, Opts#{image => fun(_) -> {ok, png(48)} end}),
        ?assertNotEqual(maps:get(<<"path">>, Second), maps:get(<<"path">>, Third)),
        receive {cached, _, _} -> error(old_source_delivered_after_url_change) after 0 -> ok end,
        age(maps:get(<<"path">>, First), 2 * 86400),
        age(filename:join(Root, "static.json"), 2 * 86400),
        {ok, Again} = retinue_poe2_assets:load(<<"exalted">>, Parent, Opts#{image => fun(_) -> {ok, png(32)} end}),
        ?assertEqual(Second, Again),
        receive {cached, _, Third} -> ok after 0 -> error(no_last_cached_source) end,
        ?assertNot(filelib:is_file(maps:get(<<"path">>, First))),
        ?assert(filelib:is_file(maps:get(<<"path">>, Third))),
        ?assertEqual(1, length(filelib:wildcard(filename:join([Root, "sources", "*.json"]))))
    after file:del_dir_r(Root) end.

age(Path, Seconds) ->
    ok = file:write_file_info(Path, #file_info{mtime = erlang:system_time(second) - Seconds}, [{time, posix}]).

await_idle(_, 0) -> error(worker_not_finished);
await_idle(Server, Attempts) ->
    case map_size(maps:get(workers, sys:get_state(Server))) of
        0 -> ok;
        _ -> timer:sleep(5), await_idle(Server, Attempts - 1)
    end.
entry(Id, Image) -> #{<<"id">> => Id, <<"text">> => <<"Currency">>, <<"image">> => Image}.
png(Size) -> <<137,80,78,71,13,10,26,10, 13:32, "IHDR", Size:32, Size:32, 8,6,0,0,0>>.

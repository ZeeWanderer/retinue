%%%-------------------------------------------------------------------
%% EUnit coverage for daemon-owned catalog image caching.
%%%-------------------------------------------------------------------
-module(wfcli_asset_service_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

asset_service_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(_Root) -> [
         fun fetches_valid_asset_once/0,
         fun fetches_batch_concurrently/0,
         fun bounds_parallel_fetches/0,
         fun deduplicates_inflight_fetches/0,
         fun drops_dead_resolve_waiters/0,
         fun prewarms_without_refetching/0,
         fun stale_cache_returns_before_refresh/0,
         fun accepts_market_sub_icon/0,
         fun accepts_mastery_rank_icon/0,
         fun rejects_untrusted_image_name/0,
         fun reloads_persisted_descriptor/0,
         fun removes_orphan_cache_objects/0,
         fun refuses_clear_while_active/0,
         fun reports_usage_and_clears_cache/0
     ] end}.

cache_relocation_test_() ->
    {foreach, fun setup/0, fun cleanup/1,
     [fun(Root) -> ?_test(relocates_cached_descriptor(Root, relative)) end,
      fun(Root) -> ?_test(relocates_cached_descriptor(Root, legacy_absolute)) end,
      fun(Root) -> ?_test(does_not_fall_back_to_old_root(Root)) end,
      fun(Root) -> ?_test(rejects_invalid_cached_paths(Root)) end,
      fun(Root) -> ?_test(rejects_symlinked_object(Root)) end,
      fun(Root) -> ?_test(rejects_symlinked_object_directory(Root)) end]}.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-assets-" ++ os:getpid() ++ "-" ++
             integer_to_list(erlang:unique_integer([positive]))),
    Counter = atomics:new(1, []),
    application:set_env(retinue_daemon, asset_cache_dir, Root),
    application:set_env(retinue_daemon, asset_workers, 2),
    application:set_env(retinue_daemon, asset_test_counter, Counter),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          atomics:add(Counter, 1, 1),
          {ok, 200, [{"etag", "\"fixture\""}], fixture_png()}
      end),
    {ok, _Pid} = wfcli_asset_service:start_link(),
    Root.

cleanup(Root) ->
    case whereis(wfcli_asset_service) of
        undefined -> ok;
        _Pid -> gen_server:stop(wfcli_asset_service)
    end,
    application:unset_env(retinue_daemon, asset_cache_dir),
    application:unset_env(retinue_daemon, asset_workers),
    application:unset_env(retinue_daemon, asset_test_counter),
    application:unset_env(retinue_daemon, asset_http_fun),
    application:unset_env(retinue_daemon, asset_fresh_ms),
    _ = file:del_dir_r(Root),
    ok.

fetches_valid_asset_once() ->
    Request = [#{<<"id">> => <<"forma">>,
                 <<"image_name">> => <<"Forma2.png">>}],
    {ok, [First]} = wfcli_asset_service:resolve(Request),
    ?assertEqual(true, maps:get(<<"ok">>, First)),
    ?assert(filelib:is_file(binary_to_list(maps:get(<<"path">>, First)))),
    {ok, Counter} = application:get_env(retinue_daemon, asset_test_counter),
    ?assertEqual(1, atomics:get(Counter, 1)),
    {ok, [Second]} = wfcli_asset_service:resolve(Request),
    ?assertEqual(maps:get(<<"digest">>, First), maps:get(<<"digest">>, Second)),
    ?assertEqual(1, atomics:get(Counter, 1)).

fetches_batch_concurrently() ->
    Test = self(),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {asset_fetch_started, self()},
          receive continue ->
              {ok, 200, [{"etag", "\"fixture\""}], fixture_png()}
          end
      end),
    Resolver = spawn(fun() ->
        Test ! {asset_batch_result, wfcli_asset_service:resolve([
            #{<<"id">> => <<"parallel-a">>, <<"image_name">> => <<"a.png">>},
            #{<<"id">> => <<"parallel-b">>, <<"image_name">> => <<"b.png">>}
        ])}
    end),
    First = receive {asset_fetch_started, Pid1} -> Pid1 after 1000 -> timeout end,
    Second = receive {asset_fetch_started, Pid2} -> Pid2 after 1000 -> timeout end,
    ?assert(is_pid(First)),
    ?assert(is_pid(Second)),
    First ! continue,
    Second ! continue,
    receive
        {asset_batch_result, {ok, Results}} -> ?assertEqual(2, length(Results))
    after 1000 ->
        error({asset_batch_timeout, Resolver})
    end,
    {ok, Counter} = application:get_env(retinue_daemon, asset_test_counter),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          atomics:add(Counter, 1, 1),
          {ok, 200, [{"etag", "\"fixture\""}], fixture_png()}
      end).

bounds_parallel_fetches() ->
    Test = self(),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {bounded_fetch_started, self()},
          receive continue -> {ok, 200, [], fixture_png()} end
      end),
    Resolver = spawn(fun() ->
        Test ! {bounded_result, wfcli_asset_service:resolve([
            #{<<"id">> => <<"bounded-a">>, <<"image_name">> => <<"bounded-a.png">>},
            #{<<"id">> => <<"bounded-b">>, <<"image_name">> => <<"bounded-b.png">>},
            #{<<"id">> => <<"bounded-c">>, <<"image_name">> => <<"bounded-c.png">>}
        ])}
    end),
    First = receive {bounded_fetch_started, Pid1} -> Pid1 after 1000 -> timeout end,
    Second = receive {bounded_fetch_started, Pid2} -> Pid2 after 1000 -> timeout end,
    ?assert(is_pid(First)),
    ?assert(is_pid(Second)),
    receive {bounded_fetch_started, _Pid} -> error(worker_limit_exceeded)
    after 100 -> ok
    end,
    First ! continue,
    Third = receive {bounded_fetch_started, Pid3} -> Pid3 after 1000 -> timeout end,
    ?assert(is_pid(Third)),
    Second ! continue,
    Third ! continue,
    receive
        {bounded_result, {ok, Results}} -> ?assertEqual(3, length(Results))
    after 1000 -> error({bounded_batch_timeout, Resolver})
    end,
    restore_http_fun().

deduplicates_inflight_fetches() ->
    Test = self(),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {dedupe_fetch_started, self()},
          receive continue -> {ok, 200, [], fixture_png()} end
      end),
    Request = [#{<<"id">> => <<"dedupe">>, <<"image_name">> => <<"dedupe.png">>}],
    _FirstResolver = spawn(fun() ->
        Test ! {dedupe_result, first, wfcli_asset_service:resolve(Request)}
    end),
    Worker = receive {dedupe_fetch_started, Pid} -> Pid after 1000 -> timeout end,
    ?assert(is_pid(Worker)),
    _SecondResolver = spawn(fun() ->
        Test ! {dedupe_result, second, wfcli_asset_service:resolve(Request)}
    end),
    receive {dedupe_fetch_started, _Pid} -> error(duplicate_fetch_started)
    after 100 -> ok
    end,
    Worker ! continue,
    receive {dedupe_result, first, {ok, [_]}} -> ok after 1000 -> error(first_timeout) end,
    receive {dedupe_result, second, {ok, [_]}} -> ok after 1000 -> error(second_timeout) end,
    restore_http_fun().

drops_dead_resolve_waiters() ->
    Test = self(),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {abandoned_fetch_started, self()},
          receive continue -> {ok, 200, [], fixture_png()} end
      end),
    Resolver = spawn(fun() ->
        wfcli_asset_service:resolve(
          [#{<<"id">> => <<"abandoned">>, <<"image_name">> => <<"abandoned.png">>}])
    end),
    Worker = receive {abandoned_fetch_started, Pid} -> Pid after 1000 -> timeout end,
    ?assert(is_pid(Worker)),
    exit(Resolver, kill),
    wait_for_status(waiting_calls, 0, 100),
    Worker ! continue,
    wait_until_idle(100),
    restore_http_fun().

prewarms_without_refetching() ->
    {ok, Counter} = application:get_env(retinue_daemon, asset_test_counter),
    Before = atomics:get(Counter, 1),
    Request = [#{<<"id">> => <<"prewarm">>, <<"image_name">> => <<"prewarm.png">>}],
    ok = wfcli_asset_service:prewarm(Request),
    wait_until_idle(100),
    ?assertEqual(Before + 1, atomics:get(Counter, 1)),
    {ok, [Result]} = wfcli_asset_service:resolve(Request),
    ?assertEqual(true, maps:get(<<"ok">>, Result)),
    ?assertEqual(Before + 1, atomics:get(Counter, 1)).

stale_cache_returns_before_refresh() ->
    Request = [#{<<"id">> => <<"stale">>,
                 <<"image_name">> => <<"stale.png">>}],
    {ok, [Initial]} = wfcli_asset_service:resolve(Request),
    InitialDigest = maps:get(<<"digest">>, Initial),
    application:set_env(retinue_daemon, asset_fresh_ms, 0),
    Test = self(),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {stale_refresh_started, self()},
          receive continue ->
              {ok, 200, [{"etag", "\"changed\""}],
               <<(fixture_png())/binary, 0>>}
          end
      end),
    {ok, Subscription} = wfcli_asset_service:subscribe(self()),
    _Resolver = spawn(fun() ->
        Test ! {stale_resolve_result, wfcli_asset_service:resolve(Request)}
    end),
    Worker = receive
        {stale_refresh_started, Pid} -> Pid
    after 1000 -> error(stale_refresh_not_started)
    end,
    Stale = receive
        {stale_resolve_result, {ok, [Value]}} -> Value
    after 100 -> error(stale_resolve_blocked_on_refresh)
    end,
    ?assertEqual(true, maps:get(<<"stale">>, Stale)),
    ?assertEqual(InitialDigest, maps:get(<<"digest">>, Stale)),
    Worker ! continue,
    receive
        {wfcli_asset, Subscription, Refreshed} ->
            ?assertNotEqual(InitialDigest, maps:get(<<"digest">>, Refreshed)),
            ?assertEqual(<<"stale.png">>, maps:get(<<"image_name">>, Refreshed))
    after 1000 -> error(asset_refresh_not_streamed)
    end,
    ok = wfcli_asset_service:unsubscribe(Subscription),
    wait_until_idle(100),
    application:unset_env(retinue_daemon, asset_fresh_ms),
    restore_http_fun().

accepts_market_sub_icon() ->
    {ok, [Result]} = wfcli_asset_service:resolve(
                       [#{<<"id">> => <<"prime-barrel">>,
                          <<"source">> => <<"market">>,
                          <<"image_name">> =>
                              <<"sub_icons/weapon/prime_barrel_128x128.png">>}]),
    ?assertEqual(true, maps:get(<<"ok">>, Result)).

accepts_mastery_rank_icon() ->
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          {ok, 200, [], <<"RIFF", 4:32/little, "WEBP", 0:32>>}
      end),
    {ok, [Result]} = wfcli_asset_service:resolve(
                       [#{<<"id">> => <<"mastery-rank:18">>,
                          <<"source">> => <<"mastery">>,
                          <<"image_name">> => <<"18.webp">>}]),
    ?assertEqual(true, maps:get(<<"ok">>, Result)),
    ?assertEqual(<<"image/webp">>, maps:get(<<"media_type">>, Result)),
    ?assertEqual(<<".webp">>,
                 filename:extension(maps:get(<<"path">>, Result))),
    restore_http_fun().

rejects_untrusted_image_name() ->
    {ok, [Result]} = wfcli_asset_service:resolve(
                       [#{<<"id">> => <<"bad">>,
                          <<"image_name">> => <<"../secret.png">>}]),
    ?assertEqual(false, maps:get(<<"ok">>, Result)).

reloads_persisted_descriptor() ->
    Before = wfcli_asset_service:status(),
    ok = gen_server:stop(wfcli_asset_service),
    {ok, Root} = application:get_env(retinue_daemon, asset_cache_dir),
    #{entries := Entries} = read_index(Root),
    maps:foreach(fun(_Key, #{path := Name, digest := Digest, media_type := MediaType}) ->
        ?assertEqual(relative, filename:pathtype(Name)),
        ?assertEqual(Name, filename:basename(Name)),
        Extension = case MediaType of
            <<"image/png">> -> <<".png">>;
            <<"image/jpeg">> -> <<".jpg">>;
            <<"image/webp">> -> <<".webp">>
        end,
        ?assertEqual(<<Digest/binary, Extension/binary>>, Name)
    end, Entries),
    {ok, _Pid} = wfcli_asset_service:start_link(),
    After = wfcli_asset_service:status(),
    ?assertEqual(maps:get(objects, Before), maps:get(objects, After)).

removes_orphan_cache_objects() ->
    {ok, Root} = application:get_env(retinue_daemon, asset_cache_dir),
    Orphan = filename:join([Root, "objects", "orphan.png"]),
    ok = filelib:ensure_dir(Orphan),
    ok = file:write_file(Orphan, fixture_png()),
    wfcli_asset_service ! maintain_cache,
    wait_for_file_removal(Orphan, 100).

refuses_clear_while_active() ->
    Test = self(),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          Test ! {clear_fetch_started, self()},
          receive continue -> {ok, 200, [], fixture_png()} end
      end),
    Resolver = spawn(fun() ->
        Test ! {clear_fetch_result,
                wfcli_asset_service:resolve(
                  [#{<<"id">> => <<"clear-active">>,
                     <<"image_name">> => <<"clear-active.png">>}])}
    end),
    Worker = receive {clear_fetch_started, Pid} -> Pid after 1000 -> timeout end,
    ?assert(is_pid(Worker)),
    ?assertEqual({error, asset_cache_busy}, wfcli_asset_service:clear()),
    Worker ! continue,
    receive {clear_fetch_result, {ok, [_]}} -> ok
    after 1000 -> error({clear_fetch_timeout, Resolver})
    end,
    restore_http_fun().

reports_usage_and_clears_cache() ->
    {ok, Root} = application:get_env(retinue_daemon, asset_cache_dir),
    Status = wfcli_asset_service:status(),
    Files = filelib:wildcard(filename:join([Root, "objects", "*"])),
    Sizes = [Size || Path <- Files,
                     {ok, #file_info{type = regular, size = Size}} <- [file:read_file_info(Path)]],
    ?assertEqual(length(Sizes), maps:get(objects, Status)),
    ?assertEqual(lists:sum(Sizes), maps:get(bytes, Status)),
    {ok, Cleared} = wfcli_asset_service:clear(),
    ?assertEqual(0, maps:get(objects, Cleared)),
    ?assertEqual(0, maps:get(bytes, Cleared)),
    ?assertEqual(0, maps:get(entries, Cleared)).

relocates_cached_descriptor(Root, Format) ->
    with_relocated_cache(Root, fun(Request, Original, Index, Moved) ->
        OldPath = maps:get(<<"path">>, Original),
        Name = filename:basename(OldPath),
        NewPath = filename:join([Moved, "objects", binary_to_list(Name)]),
        Stored = case Format of
            relative -> Index;
            legacy_absolute -> absolute_index(Index, OldPath)
        end,
        ok = write_index(Moved, Stored),
        ok = filelib:ensure_dir(NewPath),
        {ok, _} = file:copy(OldPath, NewPath),
        {ok, OldIndex} = file:read_file(filename:join(Root, "index.term")),
        ok = file:change_mode(Root, 0),
        try
            {ok, _Pid} = wfcli_asset_service:start_link(),
            {ok, [Immediate]} = wfcli_asset_service:resolve(Request),
            ?assertEqual(true, maps:get(<<"ok">>, Immediate)),
            ?assertEqual(list_to_binary(NewPath), maps:get(<<"path">>, Immediate)),
            maintain_cache(),
            {ok, [AfterMaintenance]} = wfcli_asset_service:resolve(Request),
            ?assertEqual(Immediate, AfterMaintenance),
            ?assertEqual({ok, fixture_png()}, file:read_file(NewPath)),
            {ok, Counter} = application:get_env(retinue_daemon, asset_test_counter),
            ?assertEqual(1, atomics:get(Counter, 1)),
            ok = gen_server:stop(wfcli_asset_service),
            #{entries := Entries} = read_index(Moved),
            [Entry] = maps:values(Entries),
            ?assertEqual(Name, maps:get(path, Entry))
        after
            ok = file:change_mode(Root, 8#700)
        end,
        ?assertEqual({ok, fixture_png()}, file:read_file(OldPath)),
        ?assertEqual({ok, OldIndex}, file:read_file(filename:join(Root, "index.term")))
    end).

does_not_fall_back_to_old_root(Root) ->
    with_relocated_cache(Root, fun(Request, Original, Index, Moved) ->
        OldPath = maps:get(<<"path">>, Original),
        ok = write_index(Moved, absolute_index(Index, OldPath)),
        application:set_env(retinue_daemon, asset_http_fun,
                            fun(_, _) -> {error, offline} end),
        {ok, _Pid} = wfcli_asset_service:start_link(),
        maintain_cache(),
        {ok, [Result]} = wfcli_asset_service:resolve(Request),
        ?assertEqual(false, maps:get(<<"ok">>, Result)),
        ?assertEqual({ok, fixture_png()}, file:read_file(OldPath))
    end).

rejects_invalid_cached_paths(Root) ->
    {Request, _Original, Index = #{entries := Entries}} = cached_fixture(Root),
    [Entry = #{path := Name, digest := Digest}] = maps:values(Entries),
    Unrelated = filename:join(Root, "keep.txt"),
    ok = file:write_file(Unrelated, <<"unrelated">>),
    Invalid = [Entry#{path := <<"../", Name/binary>>},
               Entry#{path := <<"objects/", Name/binary>>},
               Entry#{path := <<"./", Name/binary>>},
               Entry#{path := list_to_binary(Unrelated)},
               Entry#{path := <<Digest/binary, ".jpg">>},
               Entry#{digest := binary:copy(<<"g">>, 64)},
               Entry#{digest := <<"short">>},
               Entry#{media_type := <<"text/plain">>}, not_an_entry],
    BadEntries = maps:from_list(
                   [{{<<"wfcd">>, integer_to_binary(N)}, Value}
                    || {N, Value} <- lists:enumerate(Invalid)]),
    ok = write_index(Root, Index#{entries := maps:merge(Entries, BadEntries)}),
    {ok, _Pid} = wfcli_asset_service:start_link(),
    maintain_cache(),
    ?assertMatch(#{entries := 1}, wfcli_asset_service:status()),
    {ok, [Result]} = wfcli_asset_service:resolve(Request),
    ?assertEqual(true, maps:get(<<"ok">>, Result)),
    ?assertEqual({ok, <<"unrelated">>}, file:read_file(Unrelated)),
    ok = gen_server:stop(wfcli_asset_service),
    ?assertEqual(Index, read_index(Root)).

rejects_symlinked_object(Root) ->
    with_relocated_cache(Root, fun(Request, Original, Index, Moved) ->
        OldPath = maps:get(<<"path">>, Original),
        NewPath = filename:join([Moved, "objects", binary_to_list(filename:basename(OldPath))]),
        ok = filelib:ensure_dir(NewPath),
        ok = file:make_symlink(OldPath, NewPath),
        ok = write_index(Moved, Index),
        application:set_env(retinue_daemon, asset_http_fun,
                            fun(_, _) -> {error, offline} end),
        {ok, _Pid} = wfcli_asset_service:start_link(),
        maintain_cache(),
        {ok, [Result]} = wfcli_asset_service:resolve(Request),
        ?assertEqual(false, maps:get(<<"ok">>, Result)),
        ?assertMatch({ok, _}, file:read_link(NewPath)),
        ?assertEqual({ok, fixture_png()}, file:read_file(OldPath))
    end).

rejects_symlinked_object_directory(Root) ->
    with_relocated_cache(Root, fun(Request, Original, Index, Moved) ->
        OldPath = maps:get(<<"path">>, Original),
        Unrelated = filename:join([Root, "objects", "keep.txt"]),
        ok = file:write_file(Unrelated, <<"unrelated">>),
        ok = write_index(Moved, Index),
        ok = file:make_symlink(filename:join(Root, "objects"), filename:join(Moved, "objects")),
        {ok, _Pid} = wfcli_asset_service:start_link(),
        maintain_cache(),
        {ok, [Result]} = wfcli_asset_service:resolve(Request),
        ?assertEqual(false, maps:get(<<"ok">>, Result)),
        ?assertEqual({ok, fixture_png()}, file:read_file(OldPath)),
        ?assertEqual({ok, <<"unrelated">>}, file:read_file(Unrelated))
    end).

with_relocated_cache(Root, Fun) ->
    {Request, Original, Index} = cached_fixture(Root),
    Moved = Root ++ "-moved",
    application:set_env(retinue_daemon, asset_cache_dir, Moved),
    try
        Fun(Request, Original, Index, Moved)
    after
        case whereis(wfcli_asset_service) of
            undefined -> ok;
            Pid -> gen_server:stop(Pid)
        end,
        application:set_env(retinue_daemon, asset_cache_dir, Root),
        _ = file:del_dir_r(Moved)
    end.

cached_fixture(Root) ->
    Request = [#{<<"id">> => <<"relocation">>, <<"image_name">> => <<"relocation.png">>}],
    {ok, [Descriptor]} = wfcli_asset_service:resolve(Request),
    ok = gen_server:stop(wfcli_asset_service),
    {Request, Descriptor, read_index(Root)}.

absolute_index(Index = #{entries := Entries}, Path) ->
    Index#{entries := maps:map(fun(_Key, Entry) -> Entry#{path := Path} end, Entries)}.

read_index(Root) ->
    {ok, Binary} = file:read_file(filename:join(Root, "index.term")),
    binary_to_term(Binary, [safe]).

write_index(Root, Index) ->
    Path = filename:join(Root, "index.term"),
    ok = filelib:ensure_dir(Path),
    file:write_file(Path, term_to_binary(Index)).

maintain_cache() ->
    wfcli_asset_service ! maintain_cache,
    _ = wfcli_asset_service:status(),
    ok.

restore_http_fun() ->
    {ok, Counter} = application:get_env(retinue_daemon, asset_test_counter),
    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) ->
          atomics:add(Counter, 1, 1),
          {ok, 200, [{"etag", "\"fixture\""}], fixture_png()}
      end).

wait_until_idle(0) -> error(asset_service_busy);
wait_until_idle(Attempts) ->
    case wfcli_asset_service:status() of
        #{pending := 0, fetching := 0} -> ok;
        _ -> timer:sleep(10), wait_until_idle(Attempts - 1)
    end.

wait_for_status(Key, Expected, 0) ->
    ?assertEqual(Expected, maps:get(Key, wfcli_asset_service:status()));
wait_for_status(Key, Expected, Attempts) ->
    case maps:get(Key, wfcli_asset_service:status()) of
        Expected -> ok;
        _ -> timer:sleep(10), wait_for_status(Key, Expected, Attempts - 1)
    end.

wait_for_file_removal(Path, 0) -> ?assertNot(filelib:is_file(Path));
wait_for_file_removal(Path, Attempts) ->
    case filelib:is_file(Path) of
        false -> ok;
        true -> timer:sleep(10), wait_for_file_removal(Path, Attempts - 1)
    end.

fixture_png() ->
    base64:decode(
      <<"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
        "YAAAAAYAAjCB0C8AAAAASUVORK5CYII=">>).

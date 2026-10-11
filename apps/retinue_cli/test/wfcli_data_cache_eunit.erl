%%%-------------------------------------------------------------------
%% EUnit tests for data cache helper.
%%%-------------------------------------------------------------------
-module(wfcli_data_cache_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

load_uses_cache_test() ->
    {Cache, _} = temp_cache(),
    ok = file:write_file(Cache, <<"cached">>),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> erlang:error(fetch_called) end,
    {ok, Data, cached} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => false,
        fetch_fun => Fetch,
        decode_fun => Decode
    }),
    ?assertEqual(<<"cached">>, Data).

load_refreshes_test() ->
    {Cache, _} = temp_cache(),
    ok = file:write_file(Cache, <<"cached">>),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> {ok, <<"fetched">>} end,
    {ok, Data, fetched} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode
    }),
    ?assertEqual(<<"fetched">>, Data).

lock_timeout_on_contention_test() ->
    {Cache, _} = temp_cache(),
    Lock = Cache ++ ".lock",
    ok = file:write_file(Lock, <<"lock">>),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> {ok, <<"fetched">>} end,
    Res = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode,
        lock_retries => 1,
        lock_sleep_ms => 1,
        lock_timeout => 999999
    }),
    _ = file:delete(Lock),
    ?assertEqual({error, lock_timeout}, Res).

stale_lock_is_reclaimed_test() ->
    {Cache, _} = temp_cache(),
    Lock = Cache ++ ".lock",
    ok = file:write_file(Lock, <<"lock">>),
    {ok, Info0} = file:read_file_info(Lock),
    Now = calendar:datetime_to_gregorian_seconds(calendar:universal_time()),
    OldTime = calendar:gregorian_seconds_to_datetime(max(Now - 1000, 0)),
    ok = file:write_file_info(Lock, Info0#file_info{mtime = OldTime}),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> {ok, <<"fetched">>} end,
    {ok, Data, fetched} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode,
        now_fun => fun() -> calendar:datetime_to_gregorian_seconds(calendar:universal_time()) end
    }),
    _ = file:delete(Lock),
    ?assertEqual(<<"fetched">>, Data).

decode_error_does_not_clobber_cache_test() ->
    {Cache, _} = temp_cache(),
    ok = file:write_file(Cache, <<"cached">>),
    Decode = fun(_Bin) -> {error, bad_decode} end,
    Fetch = fun() -> {ok, <<"fetched">>} end,
    {error, bad_decode} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode
    }),
    {ok, Bin} = file:read_file(Cache),
    ?assertEqual(<<"cached">>, Bin).

fetch_error_propagates_test() ->
    {Cache, _} = temp_cache(),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> {error, failed} end,
    ?assertEqual({error, failed}, retinue_data_cache:load(#{
        cache => Cache,
        ttl => 0,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode
    })).

lock_waits_for_release_test() ->
    {Cache, _} = temp_cache(),
    Lock = Cache ++ ".lock",
    ok = file:write_file(Lock, <<"lock">>),
    spawn(fun() ->
        timer:sleep(10),
        _ = file:delete(Lock)
    end),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> {ok, <<"fetched">>} end,
    {ok, Data, fetched} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 0,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode,
        lock_retries => 50,
        lock_sleep_ms => 2,
        lock_timeout => 999999
    }),
    ?assertEqual(<<"fetched">>, Data).

temp_cache() ->
    BaseTmp = case os:getenv("TMPDIR") of false -> "/tmp"; undefined -> "/tmp"; V -> V end,
    Tmp = filename:join([BaseTmp, "wfcli_data_cache_test.bin"]),
    {Tmp, BaseTmp}.

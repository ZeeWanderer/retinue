%%%-------------------------------------------------------------------
%% Common Test for data cache helper.
%%%-------------------------------------------------------------------
-module(retinue_data_cache_SUITE).

-export([all/0,
         load_fetches_and_writes/1,
         load_uses_cache_after_fetch/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [load_fetches_and_writes,
     load_uses_cache_after_fetch].

load_fetches_and_writes(_Config) ->
    {Cache, _} = temp_cache(),
    file:delete(Cache),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> {ok, <<"payload">>} end,
    {ok, Data, fetched} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 0,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode
    }),
    {ok, Bin} = file:read_file(Cache),
    ?assertEqual(<<"payload">>, Data),
    ?assertEqual(<<"payload">>, Bin).

load_uses_cache_after_fetch(_Config) ->
    {Cache, _} = temp_cache(),
    file:delete(Cache),
    put(fetch_count, 0),
    Decode = fun(Bin) -> {ok, Bin} end,
    Fetch = fun() -> put(fetch_count, get(fetch_count) + 1), {ok, <<"payload">>} end,
    {ok, _, fetched} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => true,
        fetch_fun => Fetch,
        decode_fun => Decode
    }),
    {ok, _, cached} = retinue_data_cache:load(#{
        cache => Cache,
        ttl => 9999,
        refresh => false,
        fetch_fun => Fetch,
        decode_fun => Decode
    }),
    ?assertEqual(1, get(fetch_count)).

temp_cache() ->
    BaseTmp = case os:getenv("TMPDIR") of false -> "/tmp"; undefined -> "/tmp"; V -> V end,
    Tmp = filename:join([BaseTmp, "wf_data_cache_ct.bin"]),
    {Tmp, BaseTmp}.

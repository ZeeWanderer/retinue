%%%-------------------------------------------------------------------
%% EUnit coverage for public market response validation.
%%%-------------------------------------------------------------------
-module(wfcli_market_api_eunit).

-include_lib("eunit/include/eunit.hrl").

market_api_validation_test_() ->
    {setup, fun setup/0, fun cleanup/1, fun(_State) -> fun exercise/0 end}.

setup() ->
    application:set_env(retinue_daemon, market_request_interval_ms, 0).

cleanup(_State) ->
    application:unset_env(retinue_daemon, market_request_interval_ms),
    application:unset_env(retinue_daemon, market_http_fun).

exercise() ->
    set_payload(#{<<"error">> => null, <<"data">> => []}),
    ?assertMatch({error, invalid_market_manifest, _}, wfcli_market_api:fetch_items(0)),

    set_payload(#{<<"error">> => null,
                  <<"data">> => #{<<"sell">> => #{}, <<"buy">> => []}}),
    ?assertMatch({error, invalid_market_quote_orders, _},
                 wfcli_market_api:fetch_quote(<<"item">>, 0)),

    set_payload(#{<<"error">> => null,
                  <<"data">> => #{<<"sell">> => [#{<<"id">> => <<"bad">>}],
                                    <<"buy">> => []}}),
    {ok, Quote, _Next} = wfcli_market_api:fetch_quote(<<"item">>, 0),
    ?assertEqual(undefined, maps:get(lowest_sell, Quote)),
    ?assertEqual(undefined, maps:get(highest_buy, Quote)),

    set_payload(#{<<"error">> => null,
                  <<"data">> => #{<<"slug">> => <<"item">>,
                                    <<"setParts">> => [<<"part">>]}}),
    {ok, Detail, _} = wfcli_market_api:fetch_item(<<"item">>, 0),
    ?assertEqual([<<"part">>], maps:get(<<"setParts">>, Detail)),

    set_payload(#{<<"error">> => null, <<"data">> => []}),
    ?assertMatch({error, invalid_market_item, _},
                 wfcli_market_api:fetch_item(<<"item">>, 0)),

    application:set_env(retinue_daemon, market_request_interval_ms, 10),
    set_delayed_payload(
      #{<<"error">> => null,
        <<"data">> => #{<<"sell">> => [], <<"buy">> => []}}, 30),
    {ok, _TimedQuote, Next} = wfcli_market_api:fetch_quote(<<"item">>, 0),
    ?assert(Next =< erlang:monotonic_time(millisecond)).

set_payload(Payload) ->
    application:set_env(retinue_daemon, market_http_fun,
                        fun(_Url, _Headers) -> {ok, 200, jsone:encode(Payload)} end).

set_delayed_payload(Payload, Delay) ->
    application:set_env(
      retinue_daemon, market_http_fun,
      fun(_Url, _Headers) ->
          timer:sleep(Delay),
          {ok, 200, jsone:encode(Payload)}
      end).

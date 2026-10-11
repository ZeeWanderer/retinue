%%%-------------------------------------------------------------------
%% Public Warframe Market HTTP contract and response normalization.
%%%-------------------------------------------------------------------
-module(wfcli_market_api).

-export([fetch_items/1, fetch_item/2, fetch_quote/2, fetch_quote/3, context/0]).

-define(ITEMS_URL, "https://api.warframe.market/v2/items").
-define(ITEM_URL, "https://api.warframe.market/v2/item/").
-define(ORDERS_URL, "https://api.warframe.market/v2/orders/item/").
-define(REQUEST_INTERVAL_MS, 334).

-doc "Fetch English item metadata while advancing the shared rate-limit deadline.".
-spec fetch_items(integer()) -> {ok, [map()], integer()} | {error, term(), integer()}.
fetch_items(NextRequestAt) ->
    case fetch_json(?ITEMS_URL, NextRequestAt) of
        {ok, Envelope, Next} ->
            case envelope_data(Envelope) of
                {ok, [_ | _] = Items} -> {ok, Items, Next};
                {ok, _Other} -> {error, invalid_market_manifest, Next};
                {error, Reason} -> {error, Reason, Next}
            end;
        {error, _Reason, _Next} = Error -> Error
    end.

-doc "Fetch one item detail record, including set membership.".
-spec fetch_item(binary(), integer()) -> {ok, map(), integer()} | {error, term(), integer()}.
fetch_item(Slug, NextRequestAt) ->
    Url = ?ITEM_URL ++ retinue_text:to_list(Slug),
    case fetch_json(Url, NextRequestAt) of
        {ok, Envelope, Next} ->
            case envelope_data(Envelope) of
                {ok, Item} when is_map(Item) -> {ok, Item, Next};
                {ok, _Other} -> {error, invalid_market_item, Next};
                {error, Reason} -> {error, Reason, Next}
            end;
        {error, _Reason, _Next} = Error -> Error
    end.

-doc "Fetch and normalize top online orders for one exact market slug.".
-spec fetch_quote(binary(), integer()) -> {ok, map(), integer()} | {error, term(), integer()}.
fetch_quote(Slug, NextRequestAt) ->
    fetch_quote(Slug, #{}, NextRequestAt).

-doc "Fetch top online orders matching exact rank, charge, star, or subtype filters.".
-spec fetch_quote(binary(), map(), integer()) ->
    {ok, map(), integer()} | {error, term(), integer()}.
fetch_quote(Slug, Filters, NextRequestAt) ->
    Url = quote_url(Slug, Filters),
    case fetch_json(Url, NextRequestAt) of
        {ok, Envelope, Next} ->
            case parse_quote(Slug, Envelope) of
                {ok, Quote} -> {ok, Quote, Next};
                {error, Reason} -> {error, Reason, Next}
            end;
        {error, _Reason, _Next} = Error -> Error
    end.

quote_url(Slug, Filters) ->
    Base = ?ORDERS_URL ++ retinue_text:to_list(Slug) ++ "/top",
    Query = uri_string:compose_query(filter_pairs(Filters)),
    case Query of [] -> Base; _ -> Base ++ "?" ++ Query end.

filter_pairs(Filters) ->
    lists:filtermap(
      fun({Key, Name}) ->
          case maps:get(Key, Filters, undefined) of
              Value when is_integer(Value) ->
                  {true, {Name, integer_to_list(Value)}};
              Value when is_binary(Value), byte_size(Value) > 0 ->
                  {true, {Name, binary_to_list(Value)}};
              _ -> false
          end
      end,
      [{rank, "rank"}, {charges, "charges"}, {amber_stars, "amberStars"},
       {cyan_stars, "cyanStars"}, {subtype, "subtype"}]).

-doc "Return fixed public quote context shared by CLI and native clients.".
-spec context() -> map().
context() -> #{platform => <<"pc">>, crossplay => true, language => <<"en">>}.

fetch_json(Url, Next0) ->
    Now = erlang:monotonic_time(millisecond),
    Wait = case Next0 of 0 -> 0; _ -> max(0, Next0 - Now) end,
    timer:sleep(Wait),
    ok = wfcli_market_limiter:wait(),
    Next = erlang:monotonic_time(millisecond) + request_interval(),
    Headers = [{"user-agent", "wfcli/0.1 (+https://github.com/ZeeWanderer/wfcli)"},
               {"accept", "application/json"}, {"language", "en"},
               {"platform", "pc"}, {"crossplay", "true"}],
    Result = try
        case application:get_env(retinue_daemon, market_http_fun) of
            {ok, Fun} when is_function(Fun, 2) -> Fun(Url, Headers);
            _ -> real_http_get(Url, Headers)
        end
    catch HttpClass:HttpReason -> {error, {market_http_crash, HttpClass, HttpReason}}
    end,
    case Result of
        {ok, 200, Body} -> decode_json(Body, Next);
        {ok, Status, _Body} -> {error, {market_http_status, Status}, Next};
        {error, Reason} -> {error, {market_http_failed, Reason}, Next};
        Other -> {error, {invalid_market_http_result, Other}, Next}
    end.

real_http_get(Url, Headers) ->
    case httpc:request(get, {Url, Headers}, [{timeout, 15000}], [{body_format, binary}]) of
        {ok, {{_Version, Status, _Reason}, _ResponseHeaders, Body}} -> {ok, Status, Body};
        {error, Reason} -> {error, Reason}
    end.

decode_json(Body, Next) ->
    try jsone:decode(iolist_to_binary(Body), [{object_format, map}]) of
        Envelope when is_map(Envelope) -> {ok, Envelope, Next};
        Other -> {error, {invalid_market_json_root, Other}, Next}
    catch error:Reason -> {error, {invalid_market_json, Reason}, Next}
    end.

parse_quote(Slug, Envelope) ->
    case envelope_data(Envelope) of
        {ok, Data} when is_map(Data) ->
            case {maps:get(<<"sell">>, Data, []), maps:get(<<"buy">>, Data, [])} of
                {Sell, Buy} when is_list(Sell), is_list(Buy) ->
                    {ok, #{slug => Slug, platform => <<"pc">>, crossplay => true,
                           lowest_sell => price_min(Sell), highest_buy => price_max(Buy),
                           sell_orders => Sell, buy_orders => Buy,
                           quoted_at => erlang:system_time(millisecond),
                           stale => false, source => fetched}};
                _ -> {error, invalid_market_quote_orders}
            end;
        {ok, _Other} -> {error, invalid_market_quote};
        {error, _Reason} = Error -> Error
    end.

envelope_data(#{<<"error">> := Error}) when Error =/= null ->
    {error, {market_api_error, Error}};
envelope_data(#{<<"data">> := Data}) -> {ok, Data};
envelope_data(_Envelope) -> {error, invalid_market_envelope}.

price_min(Orders) -> price_minimum(order_prices(Orders)).

price_max(Orders) -> price_maximum(order_prices(Orders)).

order_prices(Orders) ->
    [Price || Order <- Orders, is_map(Order),
              {ok, Price} <- [maps:find(<<"platinum">>, Order)],
              is_integer(Price) orelse is_float(Price)].

price_minimum([]) -> undefined;
price_minimum(Prices) -> lists:min(Prices).

price_maximum([]) -> undefined;
price_maximum(Prices) -> lists:max(Prices).

request_interval() ->
    case application:get_env(retinue_daemon, market_request_interval_ms) of
        {ok, Value} when is_integer(Value), Value >= 0 -> Value;
        _ -> ?REQUEST_INTERVAL_MS
    end.

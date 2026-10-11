-module(wfcli_market_cli).

-export([command/0, run/1]).
-import(retinue_cli_args, [option/4, flag/3]).

command() ->
    #{help => "look up Warframe Market top-order prices", handler => {?MODULE, run},
      defaults => #{refresh => false},
      arguments => [flag(refresh, "refresh", "refresh cached prices"),
                    (option(ttl, "ttl", {integer, [{min, 60}]}, "quote freshness in seconds"))#{
                        default => 60},
                    option(limit, "limit", {integer, [{min, 0}, {max, 100}]},
                           "maximum items to quote")] ++
                   retinue_cli_args:format([table, block], table) ++ retinue_cli_args:query()}.

run(#{query_tokens := []}) -> retinue_cli_runtime:fail("market requires a query");
run(Parsed) -> run_quote(Parsed).

run_quote(Parsed) ->
    Request0 = Parsed#{source => market, action => quote_query},
    case retinue_client:one_shot(warframe, Request0) of
        {ok, Result} ->
            wfcli_market_format:print(maps:get(query, Result), maps:get(results, Result),
                                      maps:get(context, Result, #{}),
                                      maps:get(quote_errors, Result, #{}));
        {error, {query_errors, Errors}} -> fail(Errors);
        {error, {market_query_too_broad, Count, Max}} ->
            fail([io_lib:format("market query matched ~p items; refine it or pass --limit N (automatic quote cap: ~p)",
                                [Count, Max])]);
        {error, {market_quote_limit_exceeded, Count, Max}} ->
            fail([io_lib:format("market quote request selected ~p items; maximum is ~p", [Count, Max])]);
        {error, Reason} -> fail([retinue_client:format_error(Reason)])
    end.

fail(Errors) -> retinue_cli_runtime:fail(lists:join("\n", Errors)).

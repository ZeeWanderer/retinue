%%%-------------------------------------------------------------------
%% Terminal presentation for Warframe Market entities and quotes.
%%%-------------------------------------------------------------------
-module(wf_market_format).

-export([print/4]).

-doc "Render market query results without owning fetch or price semantics.".
-spec print(map(), map(), map(), map()) -> ok.
print(#{output_format := json}, Results, Context, Errors) ->
    retinue_output:json(#{results => retinue_output:results(Results), context => Context, errors => Errors});
print(Query, Results, _Context, Errors) ->
    Entries = maps:get(slice, Results, []),
    io:format("Market: PC, cross-play, English; ~p match(es), showing ~p~n~n",
              [maps:get(total, Results, 0), maps:get(shown, Results, 0)]),
    case maps:get(output_format, Query, table) of
        block -> lists:foreach(fun(Entry) -> print_block(Entry, Errors) end, Entries);
        table -> print_table(Entries, Errors)
    end.

print_table([], _Errors) -> io:format("no matches~n");
print_table(Entries, Errors) ->
    Headers = ["Item", "Ducats", "Lowest sell", "Highest buy", "Age", "Source"],
    Rows = [row(Entry, Errors) || Entry <- Entries],
    lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end,
                  retinue_table:render_lines(Headers, Rows, #{})).

row(Entry, Errors) ->
    Slug = maps:get(slug, Entry),
    Ducats = value(ducats, Entry),
    case maps:get(quote, Entry, undefined) of
        undefined ->
            [text(maps:get(name, Entry)), price(Ducats), "-", "-", "-",
             error_text(maps:get(Slug, Errors, not_cached))];
        Quote ->
            [text(maps:get(name, Entry)), price(Ducats),
             price(maps:get(lowest_sell, Quote, undefined)),
             price(maps:get(highest_buy, Quote, undefined)), age(Quote),
             text(maps:get(source, Quote, cached))]
    end.

print_block(Entry, Errors) ->
    Slug = maps:get(slug, Entry),
    io:format("~ts~n  slug: ~ts~n  ducats: ~s~n",
              [maps:get(name, Entry), Slug, price(value(ducats, Entry))]),
    case maps:get(quote, Entry, undefined) of
        undefined -> io:format("  error: ~ts~n~n", [error_text(maps:get(Slug, Errors, not_cached))]);
        Quote ->
            io:format("  lowest sell: ~s platinum~n", [price(maps:get(lowest_sell, Quote, undefined))]),
            io:format("  highest buy: ~s platinum~n", [price(maps:get(highest_buy, Quote, undefined))]),
            io:format("  sell orders: ~p~n", [length(maps:get(sell_orders, Quote, []))]),
            io:format("  buy orders: ~p~n", [length(maps:get(buy_orders, Quote, []))]),
            io:format("  age: ~s; source: ~s~n~n", [age(Quote), text(maps:get(source, Quote, cached))])
    end.

price(undefined) -> "-";
price(Value) -> text(Value).

value(Key, Entry) -> maps:get(Key, maps:get(row_map, Entry, #{}), undefined).

age(Quote) ->
    case maps:get(quoted_at, Quote, undefined) of
        undefined -> "unknown";
        QuotedAt ->
            Seconds = max(0, (erlang:system_time(millisecond) - QuotedAt) div 1000),
            integer_to_list(Seconds) ++ "s"
    end.

error_text(Value) when is_atom(Value) -> atom_to_list(Value);
error_text(Value) -> lists:flatten(io_lib:format("~p", [Value])).

text(Value) -> retinue_text:to_list(Value).

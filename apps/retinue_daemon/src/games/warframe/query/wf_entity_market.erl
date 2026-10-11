%%%-------------------------------------------------------------------
%% Searchable entities projected from Warframe Market item metadata.
%%%-------------------------------------------------------------------
-module(wf_entity_market).

-export([build_entries/2, query_field/2, query_sort_field/2, default_sort/1]).

-doc "Build market item entities, attaching only already-cached quotes.".
-spec build_entries(map(), map()) -> [map()].
build_entries(Snapshot, Opts) ->
    Quotes = maps:get(quotes, Snapshot, #{}),
    [build(Item, maps:get(maps:get(<<"slug">>, Item, <<>>), Quotes, undefined), Opts)
     || Item <- maps:get(items, Snapshot, [])].

build(Item, Quote, Opts) ->
    Slug = maps:get(<<"slug">>, Item, <<>>),
    Name = item_name(Item),
    Data = case Quote of
        undefined -> Item;
        _ -> Item#{<<"quote">> => Quote}
    end,
    Spec = #{row_map_fun => fun(Entry) ->
        EntryData = maps:get(data, Entry),
        EntryQuote = maps:get(<<"quote">>, EntryData, #{}),
        #{name => maps:get(name, Entry), slug => Slug,
          ducats => maps:get(<<"ducats">>, EntryData, undefined),
          lowest_sell => maps:get(lowest_sell, EntryQuote, undefined),
          highest_buy => maps:get(highest_buy, EntryQuote, undefined),
          quoted_at => maps:get(quoted_at, EntryQuote, undefined)}
    end},
    (wf_entity:build(market_item, maps:get(<<"id">>, Item, Slug), Name,
                        Data, Opts#{search_raw => true}, Spec))#{slug => Slug,
                                                               quote => Quote}.

-doc "Resolve market fields; raw item and quote data remain under data.<path>.".
-spec query_field(term(), string() | atom()) -> {ok, map()} | error.
query_field(_Kind, Key0) ->
    KeyText = retinue_text:to_list(Key0),
    Key = string:lowercase(KeyText),
    case Key of
        "name" -> field(name, {entry, name}, string, contains);
        "id" -> field(id, {entry, id}, string, contains);
        "type" -> field(type, {entry, type}, string, eq);
        "slug" -> field(slug, {entry, slug}, string, contains);
        "ducats" -> field(ducats, {row, ducats}, number, eq);
        "lowest_sell" -> field(lowest_sell, {row, lowest_sell}, number, eq);
        "highest_buy" -> field(highest_buy, {row, highest_buy}, number, eq);
        "quoted_at" -> field(quoted_at, {row, quoted_at}, time_point, eq);
        "tag" -> field(tag, {data_path, "tags.*"}, string, contains);
        "tags" -> field(tag, {data_path, "tags.*"}, string, contains);
        _ ->
            case lists:prefix("data.", Key) of
                true -> field({data, string:slice(KeyText, 5)},
                              {data_path, string:slice(KeyText, 5)}, dynamic, contains);
                false -> error
            end
    end.

-doc "Market sort fields use the filter field contract.".
-spec query_sort_field(term(), string() | atom()) -> {ok, map()} | error.
query_sort_field(Kind, Key) -> query_field(Kind, Key).

-doc "Sort market catalog by display name unless query supplies sort.".
-spec default_sort(term()) -> [map()].
default_sort(_Kind) -> [#{key => "name", dir => asc}].

item_name(Item) ->
    I18n = maps:get(<<"i18n">>, Item, #{}),
    English = maps:get(<<"en">>, I18n, #{}),
    maps:get(<<"name">>, English, maps:get(<<"slug">>, Item, <<>>)).

field(Key, Source, Kind, DefaultOp) ->
    {ok, #{key => Key, source => Source, kind => Kind, default_op => DefaultOp}}.

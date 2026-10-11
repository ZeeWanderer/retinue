%%%-------------------------------------------------------------------
%% Pure worldstate result selection for CLI renderers.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_results).

-include_lib("retinue_daemon/include/wfcli_worldstate.hrl").

-export([summary/1, entries/3, query/3, query_parsed/3, apply_sort/3]).

-type ws() :: #ws{}.
-type entry() :: map().
-type parsed_query() :: map().
-type type_filter() :: atom() | undefined.
-type day_filter() :: integer() | undefined.

-doc "Count normalized worldstate entries by type for CLI summaries.".
-spec summary([entry()]) -> #{atom() => non_neg_integer()}.
summary(Entries) ->
    lists:foldl(fun add_count/2, #{}, Entries).

-doc "Return indexed entries after type and calendar-day filters, before text/query filtering.".
-spec entries(ws(), type_filter(), day_filter()) -> [entry()].
entries(Ws, TypeFilter, DayFilter) ->
    Index0 = wfcli_worldstate:index(Ws),
    Now = (maps:get(now_fun, wfcli_worldstate:opts(Ws),
                    fun() -> erlang:system_time(second) end))() * 1000,
    Index = [Entry || Entry <- Index0, current_rotation(Entry, Now)],
    maybe_filter_calendar_day(maybe_filter_type(Index, TypeFilter), DayFilter).

current_rotation(#{type := circuit, data := Data}, Now) ->
    Start = date_millis(maps:get(<<"Activation">>, Data, undefined)),
    End = date_millis(maps:get(<<"Expiry">>, Data, undefined)),
    is_integer(Start) andalso is_integer(End) andalso Start =< Now andalso Now < End;
current_rotation(_, _) -> true.

date_millis(#{<<"$date">> := Date}) -> date_millis(Date);
date_millis(#{<<"$numberLong">> := Value}) when is_binary(Value) ->
    try binary_to_integer(Value) catch error:badarg -> undefined end;
date_millis(Value) when is_integer(Value) -> Value;
date_millis(_) -> undefined.

-doc "Run parsed query filters or fallback text search, then apply explicit/default sorting.".
-spec query(ws(), string() | undefined, day_filter()) -> {[entry()], parsed_query()}.
query(Ws, undefined, DayFilter) ->
    Opts = wfcli_worldstate:opts(Ws),
    Entries = entries(Ws, maps:get(type_filter, Opts, undefined), DayFilter),
    {Entries, #{filters => [], text => [], extracts => [], sort => []}};
query(Ws, Query, DayFilter) ->
    ParsedQuery = wfcli_worldstate_query:parse(Query),
    {query_parsed(Ws, ParsedQuery, DayFilter), ParsedQuery}.

-doc "Evaluate a query parsed at registration time against one indexed worldstate snapshot.".
-spec query_parsed(ws(), parsed_query(), day_filter()) -> [entry()].
query_parsed(Ws, ParsedQuery, DayFilter) ->
    Opts = wfcli_worldstate:opts(Ws),
    TypeFilter = maps:get(type_filter, Opts, undefined),
    Entries0 = query_entries(Ws, TypeFilter, DayFilter, ParsedQuery),
    Matches0 = case wfcli_worldstate_query:has_filters(ParsedQuery) of
        true ->
            [E || E <- Entries0, wfcli_worldstate_query:match(E, ParsedQuery)];
        false ->
            Text = string:join(maps:get(text, ParsedQuery, []), " "),
            wfcli_worldstate:search_entries(Entries0, Text)
    end,
    apply_sort(Matches0, ParsedQuery, Opts).

query_entries(Ws, raw_worldstate, _DayFilter, _ParsedQuery) ->
    [wfcli_worldstate:raw_entry(Ws)];
query_entries(Ws, undefined, DayFilter, ParsedQuery) ->
    Entries = entries(Ws, undefined, DayFilter),
    case targets_raw_worldstate(maps:get(query, ParsedQuery, match_all)) of
        true -> Entries ++ [wfcli_worldstate:raw_entry(Ws)];
        false -> Entries
    end;
query_entries(Ws, TypeFilter, DayFilter, _ParsedQuery) ->
    entries(Ws, TypeFilter, DayFilter).

targets_raw_worldstate({filter, Spec, _Op, Values}) ->
    maps:get(source, Spec, undefined) =:= {entry, type} andalso
        lists:member("raw_worldstate", Values);
targets_raw_worldstate({'not', Query}) -> targets_raw_worldstate(Query);
targets_raw_worldstate({'and', Queries}) -> lists:any(fun targets_raw_worldstate/1, Queries);
targets_raw_worldstate({'or', Queries}) -> lists:any(fun targets_raw_worldstate/1, Queries);
targets_raw_worldstate(_) -> false.

-doc "Apply query sort specs; fall back to per-type default worldstate sort order.".
-spec apply_sort([entry()], parsed_query(), map()) -> [entry()].
apply_sort(Entries, ParsedQuery, Opts) ->
    Sorts0 = maps:get(sort, ParsedQuery, []),
    Sorts = case Sorts0 of
        [] -> wfcli_entity_worldstate:default_sort(maps:get(type_filter, Opts, undefined));
        _ -> Sorts0
    end,
    case Sorts of
        [] -> Entries;
        _ ->
            Indexed = lists:zip(lists:seq(0, length(Entries) - 1), Entries),
            Sorted = lists:sort(
              fun({AIdx, A}, {BIdx, B}) ->
                  case compare_sort_specs(A, B, Sorts, Opts) of
                      eq -> AIdx =< BIdx;
                      lt -> true;
                      gt -> false
                  end
              end,
              Indexed),
            [Entry || {_Idx, Entry} <- Sorted]
    end.

add_count(#{type := Type}, Map) ->
    maps:update_with(Type, fun(C) -> C + 1 end, 1, Map);
add_count(_, Map) ->
    Map.

maybe_filter_type(List, undefined) -> List;
maybe_filter_type(List, Type) ->
    [E || E = #{type := T} <- List, T =:= Type].

maybe_filter_calendar_day(List, undefined) -> List;
maybe_filter_calendar_day(List, Day) ->
    [E || E <- List, calendar_day_matches(E, Day)].

calendar_day_matches(#{type := calendar, data := D}, Day) ->
    calendar_day_value(D) =:= Day;
calendar_day_matches(_, _) ->
    true.

calendar_day_value(D) when is_map(D) ->
    Value = maps:get(<<"Day">>, D, undefined),
    calendar_day_int(Value);
calendar_day_value(_) ->
    undefined.

calendar_day_int(Value) when is_integer(Value) -> Value;
calendar_day_int(Value) when is_binary(Value) ->
    calendar_day_int(binary_to_list(Value));
calendar_day_int(Value) when is_list(Value) ->
    case string:to_integer(Value) of
        {Int, _} -> Int;
        _ -> undefined
    end;
calendar_day_int(_) ->
    undefined.

compare_sort_specs(_A, _B, [], _Opts) -> eq;
compare_sort_specs(A, B, [Spec | Rest], Opts) ->
    Dir = maps:get(dir, Spec, asc),
    Key = maps:get(key, Spec, ""),
    VA = normalize_sort_value(sort_value_for(A, Key, Opts), Dir),
    VB = normalize_sort_value(sort_value_for(B, Key, Opts), Dir),
    case VA =:= VB of
        true -> compare_sort_specs(A, B, Rest, Opts);
        false ->
            case wfcli_query_sort:compare(Dir, VA, VB) of
                true -> lt;
                false -> gt
            end
    end.

-doc "Resolve a sort key from either `data.path` extraction or rendered table columns.".
sort_value_for(Entry, Key0, Opts) ->
    Key = string:trim(retinue_text:to_list(Key0)),
    case lists:prefix("data.", string:lowercase(Key)) of
        true ->
            Path = string:slice(Key, 5),
            case wfcli_worldstate_query:extract(Entry, [Path]) of
                [{_, Value}] -> retinue_text:to_list(Value);
                _ -> ""
            end;
        false ->
            case sort_key_atom(Key) of
                undefined ->
                    retinue_text:to_list(maps:get(name, Entry, ""));
                AtomKey ->
                    RowMap = wfcli_worldstate_projector:table_row_map(Entry, Opts),
                    Value = maps:get(AtomKey, RowMap, ""),
                    case wfcli_worldstate_schema:query_column_spec(AtomKey) of
                        {ok, #{kind := time_point}} -> retinue_time:format_millis(Value, #{utc => true});
                        _ -> retinue_text:to_list(Value)
                    end
            end
    end.

sort_key_atom(Key0) ->
    Key = string:lowercase(retinue_text:to_list(Key0)),
    case lists:dropwhile(
           fun(Spec) ->
               SpecKey = maps:get(key, Spec, undefined),
               SpecLabel = maps:get(label, Spec, ""),
               (SpecKey =:= undefined orelse string:lowercase(atom_to_list(SpecKey)) =/= Key) andalso
                   string:lowercase(retinue_text:to_list(SpecLabel)) =/= Key
           end,
           wfcli_entity_worldstate:columns_spec()) of
        [] -> undefined;
        [Spec | _] -> maps:get(key, Spec, undefined)
    end.

normalize_sort_value(Value, Dir) ->
    Trimmed = string:trim(retinue_text:to_list(Value)),
    case Trimmed =:= "" of
        true ->
            case Dir of
                asc -> "zzzz";
                desc -> ""
            end;
        false -> Trimmed
    end.

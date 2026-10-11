%%%-------------------------------------------------------------------
%% Pure daemon-side worldstate operations.
%%%-------------------------------------------------------------------
-module(wf_worldstate_ops).

-include_lib("retinue_daemon/include/wf_worldstate.hrl").

-export([execute/2, prepare_request/1, prepare_watch/1, evaluate_watch/3]).

-type request() :: map().
-type result() :: map().

-doc "Execute one worldstate list/query/inventory request against an indexed snapshot.".
-spec execute(#ws{}, request()) -> result() | {error, term()}.
execute(#ws{} = Ws, Request) ->
    Opts0 = maps:get(opts, Request, wf_worldstate:opts(Ws)),
    TypeFilter = maps:get(type_filter, Request, maps:get(type_filter, Opts0, undefined)),
    DayFilter = maps:get(day_filter, Request, maps:get(calendar_day, Opts0, undefined)),
    Mode = maps:get(mode, Request, maps:get(mode, Opts0, list)),
    Opts = Opts0#{type_filter => TypeFilter, calendar_day => DayFilter, mode => Mode},
    Ws1 = Ws#ws{opts = Opts},
    Query = maps:get(query, Request, undefined),
    case maps:get(inventory, Request, false) of
        false -> execute_entries(Ws1, Query, DayFilter, TypeFilter, Mode, Opts, Request);
        InventoryType -> execute_inventory(Ws1, InventoryType, Query, TypeFilter, Mode, Opts)
    end.

execute_entries(Ws, undefined, DayFilter, undefined, _Mode, Opts, _Request) ->
    Entries = wf_worldstate_results:entries(Ws, undefined, DayFilter),
    #{kind => summary,
      entries => Entries,
      summary => wf_worldstate_results:summary(Entries),
      parsed_query => empty_query(),
      opts => Opts};
execute_entries(Ws, undefined, DayFilter, TypeFilter, _Mode, Opts, _Request) ->
    Entries = wf_worldstate_results:entries(Ws, TypeFilter, DayFilter),
    #{kind => entries,
      entries => Entries,
      parsed_query => empty_query(),
      opts => Opts};
execute_entries(Ws, Query, DayFilter, _TypeFilter, _Mode, Opts, Request) ->
    Parsed = maps:get(parsed_query, Request, wf_worldstate_query:parse(Query)),
    case query_errors(Parsed) of
        [] ->
            Entries = wf_worldstate_results:query_parsed(Ws, Parsed, DayFilter),
            #{kind => entries, entries => Entries, parsed_query => Parsed,
              query => Query, opts => Opts};
        Errors -> {error, {query_errors, Errors}}
    end.

execute_inventory(Ws, InventoryType, Query, TypeFilter, Mode, Opts) ->
    Entries0 = wf_worldstate:inventory_entries(Ws, InventoryType, Opts),
    Entries = case Query of
        undefined -> Entries0;
        _ -> wf_worldstate:search_entries(Entries0, Query)
    end,
    #{kind => inventory,
      entries => Entries,
      parsed_query => empty_query(),
      query => Query,
      inventory => InventoryType,
      type_filter => TypeFilter,
      mode => Mode,
      opts => Opts}.

-doc "Parse and validate a one-shot query once when the daemon accepts it.".
-spec prepare_request(request()) -> {ok, request()} | {error, term()}.
prepare_request(Request) ->
    case {maps:get(inventory, Request, false), maps:get(query, Request, undefined)} of
        {false, Query} when Query =/= undefined ->
            attach_parsed_query(Request, Query);
        _ -> {ok, Request}
    end.

-doc "Parse and validate all watch queries once when the daemon registers them.".
-spec prepare_watch(request()) -> {ok, request()} | {error, term()}.
prepare_watch(Request) ->
    case prepare_specs(maps:get(specs, Request, []), []) of
        {ok, Specs} -> {ok, Request#{specs => Specs}};
        {error, _Reason} = Error -> Error
    end.

prepare_specs([], Acc) -> {ok, lists:reverse(Acc)};
prepare_specs([Spec | Rest], Acc) ->
    case prepare_spec(Spec) of
        {ok, Prepared} -> prepare_specs(Rest, [Prepared | Acc]);
        {error, _Reason} = Error -> Error
    end.

prepare_spec(Spec) ->
    Query = maps:get(query, Spec, undefined),
    Parsed = case Query of
        undefined -> empty_query();
        _ -> wf_worldstate_query:parse(Query)
    end,
    case query_errors(Parsed) of
        [] -> {ok, Spec#{parsed_query => Parsed}};
        Errors -> {error, {query_errors, Errors}}
    end.

attach_parsed_query(Request, Query) ->
    Parsed = wf_worldstate_query:parse(Query),
    case query_errors(Parsed) of
        [] -> {ok, Request#{parsed_query => Parsed}};
        Errors -> {error, {query_errors, Errors}}
    end.

query_errors(Parsed) -> maps:get(errors, Parsed, []).

-doc "Evaluate all registered watch specs and retain canonical snapshots for change detection.".
-spec evaluate_watch(#ws{}, request(), map()) -> {result(), map()}.
evaluate_watch(#ws{} = Ws, Request, Previous) ->
    {SpecData, Next} = lists:foldl(
      fun(Spec, {DataAcc, PrevAcc}) ->
          Label = maps:get(label, Spec),
          PrevSnapshot = maps:get(Label, Previous, undefined),
          {Data, Snapshot} = evaluate_spec(Ws, Request, Spec, PrevSnapshot),
          {DataAcc ++ [Data], PrevAcc#{Label => Snapshot}}
      end,
      {[], #{}},
      maps:get(specs, Request, [])),
    Changed = lists:any(fun(Data) -> maps:get(changed, Data, false) end, SpecData),
    {#{kind => watch, specs => SpecData, changed => Changed}, Next}.

evaluate_spec(Ws, Request, Spec, PrevSnapshot) ->
    Opts0 = maps:get(opts, Request, wf_worldstate:opts(Ws)),
    Type = maps:get(type_filter, Spec),
    Mode = maps:get(mode, Request, list),
    DayFilter = case Type of
        calendar -> maps:get(calendar_day, Request, undefined);
        _ -> undefined
    end,
    Opts = Opts0#{type_filter => Type, mode => Mode, watch_table => true},
    Ws1 = Ws#ws{opts = Opts},
    %% Resolve relative time bounds once per update, not once per subscription.
    {ok, #{parsed_query := Parsed}} = prepare_spec(Spec),
    Entries = wf_worldstate_results:query_parsed(Ws1, Parsed, DayFilter),
    Extracts = maps:get(extracts, Parsed, []),
    Snapshot = canonical_snapshot(Entries, Extracts),
    Diff = wf_worldstate_diff:diff(PrevSnapshot, Snapshot),
    Changed = wf_worldstate_diff:has_changes(Diff),
    {#{label => maps:get(label, Spec),
       type_filter => Type,
       query => maps:get(query, Spec, undefined),
       entries => Entries,
       parsed_query => Parsed,
       extracts => Extracts,
       opts => Opts,
       changed => Changed,
       initial => PrevSnapshot =:= undefined,
       canonical_diff => Diff},
     Snapshot}.

canonical_snapshot(Entries, Extracts) ->
    lists:foldl(
      fun({Index, Entry}, Acc) ->
          Key = wf_worldstate_diff:entry_key(Entry, Index),
          Value = canonical_value(Entry, Extracts),
          Acc#{Key => #{name => maps:get(name, Entry, ""), value => Value}}
      end,
      #{},
      lists:zip(lists:seq(0, length(Entries) - 1), Entries)).

canonical_value(Entry, []) ->
    {maps:get(type, Entry, undefined),
     maps:get(name, Entry, ""),
     maps:get(data, Entry, undefined)};
canonical_value(Entry, Extracts) ->
    wf_worldstate_query:extract(Entry, Extracts).

empty_query() ->
    #{filters => [], text => [], extracts => [], sort => []}.

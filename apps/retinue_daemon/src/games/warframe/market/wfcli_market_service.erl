%%%-------------------------------------------------------------------
%% Daemon-owned Warframe Market manifest, quote cache, and request queue.
%%%-------------------------------------------------------------------
-module(wfcli_market_service).

-behaviour(gen_server).

-export([start_link/0, submit/2, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-ifdef(TEST).
-export([root_slugs/2, worker_snapshot/2]).
-endif.

-define(SERVER, ?MODULE).
-define(DEFAULT_QUOTE_TTL, 60).
-define(MANIFEST_TTL, 86400).
-define(FAILURE_RETRY_TTL, 60).
-define(MAX_QUOTES, 20).
-define(HARD_MAX_QUOTES, 100).
-define(MAX_RESOLVE_LABELS, 20).
-define(MAX_RESOLVE_LIMIT, 5).
-define(MAX_RELIC_ITEMS, 8).
-define(DETAIL_TTL, 86400).
-define(RELIC_CATALOG_TTL, 86400).
-define(RELIC_CATALOG_RETRY_TTL, 60).
-define(RECOMMENDATION_PRICE_CANDIDATES, 4).
-define(PERSIST_DELAY_MS, 500).
-define(PERSIST_RETRY_MS, 5000).
-define(DEFAULT_READ_WORKERS, 8).

-type state() :: map().

-doc "Start serialized market catalog and quote owner.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Queue market query or quote work; result arrives as `{retinue_daemon, Ref, Reply}`.".
-spec submit(pid(), map()) -> {ok, reference()} | {error, term()}.
submit(Client, Request) ->
    gen_server:call(?SERVER, {submit, Client, Request}).

-doc "Return queue, manifest, quote-cache, and rate-limit state.".
-spec status() -> map().
status() -> gen_server:call(?SERVER, status).

-spec init([]) -> {ok, state()}.
init([]) ->
    Path = wfcli_market_cache:path(),
    {ok, #{queue => queue:new(), current => undefined,
           read_queue => queue:new(), reads => #{}, max_reads => read_worker_limit(),
           monitors => #{},
           cache_path => Path, snapshot => wfcli_market_cache:load(Path),
           next_request_at => 0, cache_error => undefined,
           cache_dirty => false, persist_timer => undefined}}.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call({submit, Client, Request}, _From, State) when is_pid(Client), is_map(Request) ->
    Ref = make_ref(),
    Monitor = erlang:monitor(process, Client),
    Job = #{ref => Ref, client => Client, monitor => Monitor,
            submitted_at => erlang:system_time(millisecond), request => Request},
    retinue_activity:activity_start(),
    State1 = State#{monitors => (maps:get(monitors, State))#{Monitor => Ref}},
    case immediate_request(Request, maps:get(snapshot, State)) of
        true -> {reply, {ok, Ref}, enqueue_read(Job, State1)};
        false ->
            self() ! process_queue,
            {reply, {ok, Ref},
             State1#{queue => queue:in(Job, maps:get(queue, State1))}}
    end;
handle_call(status, _From, State) ->
    Snapshot = maps:get(snapshot, State),
    {reply, #{queued => queue:len(maps:get(queue, State)) +
                         queue:len(maps:get(read_queue, State, queue:new())),
              processing => maps:get(current, State) =/= undefined orelse
                            map_size(maps:get(reads, State, #{})) > 0,
              concurrent_reads => map_size(maps:get(reads, State, #{})),
              items => length(maps:get(items, Snapshot, [])),
              cached_quotes => map_size(maps:get(quotes, Snapshot, #{})),
              cached_variant_quotes => map_size(maps:get(variant_quotes, Snapshot, #{})),
              cached_details => map_size(maps:get(details, Snapshot, #{})),
              cached_relics => map_size(maps:get(relics, Snapshot, #{})),
              manifest_fetched_at => maps:get(manifest_fetched_at, Snapshot, undefined),
              relics_fetched_at => maps:get(relics_fetched_at, Snapshot, undefined),
              cache_path => maps:get(cache_path, State),
              cache_error => maps:get(cache_error, State, undefined)}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast(_Message, State) -> {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info(process_queue, State = #{current := Current}) when Current =/= undefined ->
    {noreply, State};
handle_info(process_queue, State) ->
    case queue:out(maps:get(queue, State)) of
        {empty, Queue} -> {noreply, State#{queue => Queue}};
        {{value, Job}, Queue} ->
            Parent = self(),
            Token = make_ref(),
            Snapshot = worker_snapshot(maps:get(request, Job),
                                       maps:get(snapshot, State)),
            NextRequestAt = maps:get(next_request_at, State),
            {Pid, WorkerMonitor} = spawn_monitor(fun() ->
                {Reply, Updated, Next} = safe_execute(Job, Snapshot, NextRequestAt),
                Parent ! {market_result, Token,
                          {Reply, snapshot_delta(Snapshot, Updated), Next}}
            end),
            Current = #{token => Token, worker_pid => Pid,
                        worker_monitor => WorkerMonitor, job => Job},
            {noreply, State#{queue => Queue, current => Current}}
    end;
handle_info({market_result, Token, {Reply, Delta, NextRequestAt}},
            State = #{current := #{token := Token, worker_monitor := WorkerMonitor,
                                   job := Job}}) ->
    erlang:demonitor(WorkerMonitor, [flush]),
    Snapshot = maps:merge(maps:get(snapshot, State), Delta),
    State1 = complete_job(Job, Reply,
                          State#{current => undefined, snapshot => Snapshot,
                                 next_request_at => NextRequestAt}),
    State2 = mark_cache_dirty(Delta, State1),
    self() ! process_queue,
    {noreply, State2};
handle_info({market_read_result, Token, Reply}, State) ->
    case maps:take(Token, maps:get(reads, State, #{})) of
        error -> {noreply, State};
        {#{worker_monitor := WorkerMonitor, job := Job}, Reads} ->
            erlang:demonitor(WorkerMonitor, [flush]),
            {noreply, start_queued_reads(
                        complete_job(Job, Reply, State#{reads => Reads}))}
    end;
handle_info({'DOWN', Monitor, process, _Pid, Reason},
            State = #{current := #{worker_monitor := Monitor, job := Job}}) ->
    State1 = complete_job(Job, {error, {market_worker_down, Reason}},
                          State#{current => undefined}),
    self() ! process_queue,
    {noreply, State1};
handle_info({'DOWN', Monitor, process, _Pid, Reason}, State) ->
    case take_read_by_monitor(Monitor, maps:get(reads, State, #{})) of
        {ok, Job, Reads} ->
            {noreply, start_queued_reads(
                        complete_job(Job, {error, {market_read_worker_down, Reason}},
                                     State#{reads => Reads}))};
        error ->
            case cancel_client(Monitor, State) of
                {not_found, State1} -> {noreply, State1};
                {canceled, State1} ->
                    self() ! process_queue,
                    {noreply, start_queued_reads(State1)}
            end
    end;
handle_info(persist_cache, State) ->
    {noreply, flush_cache(State#{persist_timer => undefined})};
handle_info(_Message, State) -> {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, State) ->
    stop_worker(maps:get(current, State, undefined)),
    maps:foreach(fun(_Token, Worker) -> stop_worker(Worker) end,
                 maps:get(reads, State, #{})),
    cancel_timer(maps:get(persist_timer, State, undefined)),
    case maps:get(cache_dirty, State, false) of
        true -> _ = wfcli_market_cache:persist(maps:get(cache_path, State),
                                               maps:get(snapshot, State));
        false -> ok
    end,
    ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) ->
    Path = maps:get(cache_path, State, wfcli_market_cache:path()),
    {ok, State#{queue => maps:get(queue, State, queue:new()),
                current => maps:get(current, State, undefined),
                read_queue => maps:get(read_queue, State, queue:new()),
                reads => maps:get(reads, State, #{}),
                max_reads => maps:get(max_reads, State, read_worker_limit()),
                monitors => maps:get(monitors, State, #{}),
                cache_path => Path,
                snapshot => maps:get(snapshot, State, wfcli_market_cache:load(Path)),
                next_request_at => maps:get(next_request_at, State, 0),
                cache_error => maps:get(cache_error, State, undefined),
                cache_dirty => maps:get(cache_dirty, State, false),
                persist_timer => maps:get(persist_timer, State, undefined)}}.

immediate_request(#{action := Action}, Snapshot)
  when Action =:= query; Action =:= resolve_labels; Action =:= describe_items ->
    maps:get(items, Snapshot, []) =/= [];
immediate_request(#{action := quote_items, cache_only := true}, Snapshot) ->
    maps:get(items, Snapshot, []) =/= [];
immediate_request(#{action := relic_recommendations} = Request, Snapshot) ->
    maps:get(fetch_prices, Request, false) =:= false andalso
    maps:get(items, Snapshot, []) =/= [] andalso
    relic_catalog_ready(Snapshot);
immediate_request(_Request, _Snapshot) -> false.

enqueue_read(Job, State) ->
    case map_size(maps:get(reads, State, #{})) < maps:get(max_reads, State) of
        true -> start_read(Job, State);
        false -> State#{read_queue => queue:in(Job, maps:get(read_queue, State))}
    end.

start_read(Job, State) ->
    Parent = self(),
    Token = make_ref(),
    Snapshot = maps:get(snapshot, State),
    {Pid, WorkerMonitor} = spawn_monitor(fun() ->
        Reply = safe_execute_read(maps:get(request, Job), Snapshot),
        Parent ! {market_read_result, Token, Reply}
    end),
    Worker = #{worker_pid => Pid, worker_monitor => WorkerMonitor, job => Job},
    State#{reads => (maps:get(reads, State, #{}))#{Token => Worker}}.

start_queued_reads(State) ->
    case map_size(maps:get(reads, State, #{})) < maps:get(max_reads, State) of
        false -> State;
        true ->
            case queue:out(maps:get(read_queue, State)) of
                {empty, Queue} -> State#{read_queue => Queue};
                {{value, Job}, Queue} ->
                    start_queued_reads(start_read(Job, State#{read_queue => Queue}))
            end
    end.

read_worker_limit() ->
    case application:get_env(retinue_daemon, market_read_workers, ?DEFAULT_READ_WORKERS) of
        Count when is_integer(Count), Count > 0 -> Count;
        _ -> ?DEFAULT_READ_WORKERS
    end.

safe_execute_read(Request, Snapshot) ->
    try execute_read_request(Request, Snapshot)
    catch Class:Reason:Stack ->
        logger:error("market read failed: ~p:~p~n~p", [Class, Reason, Stack]),
        {error, {market_request_failed, Class, Reason}}
    end.

-ifdef(TEST).
execute_read_request(Request, Snapshot) ->
    case application:get_env(retinue_daemon, market_read_execute_fun, undefined) of
        Fun when is_function(Fun, 2) -> Fun(Request, Snapshot);
        undefined -> execute_read(Request, Snapshot)
    end.
-else.
execute_read_request(Request, Snapshot) -> execute_read(Request, Snapshot).
-endif.

execute_read(#{action := query, query_ast := Ast} = Request, Snapshot) ->
    wfcli_market_query:execute(Ast, Request, Snapshot);
execute_read(#{action := resolve_labels} = Request, Snapshot) ->
    {Reply, _Snapshot, _Next} = resolve_labels(Request, Snapshot, 0),
    Reply;
execute_read(#{action := describe_items} = Request, Snapshot) ->
    {Reply, _Snapshot, _Next} = describe_items(Request, Snapshot, 0),
    Reply;
execute_read(#{action := quote_items, items := Items, cache_only := true} = Request,
             Snapshot) ->
    {Reply, _Snapshot, _Next} = quote_items_cached(Items, Request, Snapshot, 0),
    Reply;
execute_read(#{action := relic_recommendations, era := Era} = Request, Snapshot) ->
    Reply = wfcli_relic_recommendations:build(
              Era, maps:get(relics, Snapshot, #{}), maps:get(items, Snapshot, []),
              maps:get(quotes, Snapshot, #{}), player_snapshot(),
              build_options(Request)),
    {ok, Reply#{<<"quote_errors">> => #{}}}.

safe_execute(Job, Snapshot, NextRequestAt) ->
    try execute(Job, Snapshot, NextRequestAt)
    catch Class:Reason:Stack ->
        logger:error("market request failed: ~p:~p~n~p", [Class, Reason, Stack]),
        {{error, {market_request_failed, Class, Reason}}, Snapshot, NextRequestAt}
    end.

execute(Job, Snapshot0, Next0) ->
    Request = maps:get(request, Job),
    SubmittedAt = maps:get(submitted_at, Job),
    case ensure_manifest(Request, SubmittedAt, Snapshot0, Next0) of
        {ok, Snapshot, Next} -> execute_action(Request, SubmittedAt, Snapshot, Next);
        {error, Reason, Snapshot, Next} -> {{error, Reason}, Snapshot, Next}
    end.

execute_action(#{action := query, query_ast := Ast} = Request, _SubmittedAt,
               Snapshot, Next) ->
    {wfcli_market_query:execute(Ast, Request, Snapshot), Snapshot, Next};
execute_action(#{action := quote_query, query_tokens := Tokens} = Request, SubmittedAt,
               Snapshot, Next) ->
    NormalizedTokens = lists:append([string:lexemes(Token, " \t") || Token <- Tokens]),
    case wfcli_query_parse:parse_arguments(NormalizedTokens) of
        {ok, Ast} -> quote_query(Ast, Request, SubmittedAt, Snapshot, Next);
        {error, Error} -> {{error, {query_errors, [Error]}}, Snapshot, Next}
    end;
execute_action(#{action := quote_items, items := Items, cache_only := true} = Request,
               _SubmittedAt, Snapshot, Next) when is_list(Items) ->
    quote_items_cached(Items, Request, Snapshot, Next);
execute_action(#{action := quote_items, items := Items} = Request, SubmittedAt,
               Snapshot, Next) when is_list(Items) ->
    quote_items(Items, Request, SubmittedAt, Snapshot, Next);
execute_action(#{action := quote_variant, item := Item, filters := Filters} = Request,
               SubmittedAt, Snapshot, Next) ->
    quote_variant(Item, Filters, Request, SubmittedAt, Snapshot, Next);
execute_action(#{action := resolve_labels} = Request, _SubmittedAt, Snapshot, Next) ->
    resolve_labels(Request, Snapshot, Next);
execute_action(#{action := describe_items} = Request, _SubmittedAt, Snapshot, Next) ->
    describe_items(Request, Snapshot, Next);
execute_action(#{action := relic_context} = Request, SubmittedAt, Snapshot, Next) ->
    relic_context(Request, SubmittedAt, Snapshot, Next);
execute_action(#{action := relic_recommendations} = Request, SubmittedAt, Snapshot, Next) ->
    relic_recommendations(Request, SubmittedAt, Snapshot, Next);
execute_action(Request, _SubmittedAt, Snapshot, Next) ->
    {{error, {unsupported_market_action, maps:get(action, Request, undefined)}}, Snapshot, Next}.

resolve_labels(Request, Snapshot, Next) ->
    Labels = maps:get(labels, Request, undefined),
    Limit = maps:get(limit, Request, undefined),
    case valid_resolve_labels(Labels) of
        false -> {{error, invalid_market_labels}, Snapshot, Next};
        true when not is_integer(Limit); Limit < 1; Limit > ?MAX_RESOLVE_LIMIT ->
            {{error, invalid_market_limit}, Snapshot, Next};
        true ->
            Resolutions = wfcli_market_resolver:resolve(
                            Labels, maps:get(items, Snapshot, []), Limit),
            Reply = #{context => wfcli_market_api:context(), resolutions => Resolutions},
            {{ok, Reply}, Snapshot, Next}
    end.

valid_resolve_labels(Labels) when is_list(Labels), length(Labels) =< ?MAX_RESOLVE_LABELS ->
    lists:all(fun(Label) -> is_binary(Label) andalso byte_size(Label) > 0 end, Labels);
valid_resolve_labels(_Labels) -> false.

describe_items(Request, Snapshot, Next) ->
    Values = maps:get(items, Request, undefined),
    case valid_describe_items(Values) of
        false -> {{error, invalid_market_items}, Snapshot, Next};
        true ->
            {Items, Missing} = wfcli_market_resolver:describe(
                                 Values, maps:get(items, Snapshot, [])),
            {{ok, #{context => wfcli_market_api:context(), items => Items,
                    missing => Missing}}, Snapshot, Next}
    end.

valid_describe_items(Items) when is_list(Items), length(Items) =< ?HARD_MAX_QUOTES ->
    lists:all(fun(Item) -> is_binary(Item) andalso byte_size(Item) > 0 end, Items);
valid_describe_items(_Items) -> false.

quote_variant(Item, Filters, Request, SubmittedAt, Snapshot, Next) ->
    case resolve_items([Item], maps:get(items, Snapshot, [])) of
        {[{Item, Slug}], []} ->
            Key = {Slug, Filters},
            Quotes = maps:get(variant_quotes, Snapshot, #{}),
            Cached = maps:get(Key, Quotes, undefined),
            case quote_cache_state(Cached, Request, SubmittedAt) of
                fresh ->
                    Source = case maps:get(quoted_at, Cached, 0) >= SubmittedAt of
                        true -> coalesced;
                        false -> cached
                    end,
                    Quote = Cached#{source => Source, stale => false},
                    {variant_reply(Item, Slug, Filters, Quote), Snapshot, Next};
                stale ->
                    {variant_reply(Item, Slug, Filters, Cached), Snapshot, Next};
                fetch ->
                    fetch_variant_quote(Item, Slug, Filters, Key, Cached,
                                        Snapshot, Next)
            end;
        _ -> {{error, market_item_not_found}, Snapshot, Next}
    end.

fetch_variant_quote(Item, Slug, Filters, Key, Cached, Snapshot, Next) ->
    case wfcli_market_api:fetch_quote(Slug, Filters, Next) of
        {ok, Quote, Next1} ->
            Quotes = maps:get(variant_quotes, Snapshot, #{}),
            Updated = Snapshot#{variant_quotes => Quotes#{Key => Quote},
                                updated_at => erlang:system_time(millisecond)},
            {variant_reply(Item, Slug, Filters, Quote), Updated, Next1};
        {error, Reason, Next1} when is_map(Cached) ->
            Stale = Cached#{stale => true, source => stale,
                            stale_reason => retinue_text:to_list(Reason),
                            last_attempt_at => erlang:system_time(millisecond)},
            Quotes = maps:get(variant_quotes, Snapshot, #{}),
            Updated = Snapshot#{variant_quotes => Quotes#{Key => Stale}},
            {variant_reply(Item, Slug, Filters, Stale), Updated, Next1};
        {error, Reason, Next1} -> {{error, Reason}, Snapshot, Next1}
    end.

variant_reply(Item, Slug, Filters, Quote) ->
    {ok, #{item => Item, slug => Slug, quote => Quote, filters => Filters}}.

relic_context(Request, SubmittedAt, Snapshot0, Next0) ->
    Slugs = maps:get(items, Request, undefined),
    case valid_relic_items(Slugs) of
        false -> {{error, invalid_relic_items}, Snapshot0, Next0};
        true ->
            {Snapshot1, Next1, DetailErrors} =
                fetch_details(Slugs, Snapshot0, Next0, #{}),
            Snapshot2 = case ensure_relic_catalog(Snapshot1) of
                {ok, CatalogSnapshot} -> CatalogSnapshot;
                {error, _Reason, CatalogSnapshot} -> CatalogSnapshot
            end,
            QuoteItems = unique(Slugs ++ root_slugs(Slugs, Snapshot2), []),
            {Snapshot3, Next2, QuoteErrors} = fetch_quotes(
                QuoteItems, Request, SubmittedAt, Snapshot2, Next1, #{}),
            Reply = wfcli_relic_context:build(
                Slugs,
                maps:get(items, Snapshot3, []),
                maps:get(details, Snapshot3, #{}),
                maps:get(quotes, Snapshot3, #{}),
                player_snapshot(),
                maps:get(relics, Snapshot3, #{}),
                item_catalog()),
            {{ok, Reply#{<<"detail_errors">> => DetailErrors,
                         <<"quote_errors">> => QuoteErrors}},
             Snapshot3, Next2}
    end.

valid_relic_items(Items) when is_list(Items), length(Items) =< ?MAX_RELIC_ITEMS ->
    lists:all(fun(Item) -> is_binary(Item) andalso byte_size(Item) > 0 end, Items);
valid_relic_items(_Items) -> false.

relic_recommendations(Request, SubmittedAt, Snapshot0, Next0) ->
    Era = maps:get(era, Request, undefined),
    case valid_relic_era(Era) of
        false -> {{error, invalid_relic_era}, Snapshot0, Next0};
        true ->
            case ensure_relic_catalog(Snapshot0) of
                {error, Reason, Snapshot1} ->
                    {{error, Reason}, Snapshot1, Next0};
                {ok, Snapshot1} ->
                    Player = player_snapshot(),
                    {Snapshot2, Next1, QuoteErrors} =
                        maybe_fetch_recommendation_prices(
                          Request, Era, Player, SubmittedAt, Snapshot1, Next0),
                    Reply = wfcli_relic_recommendations:build(
                              Era,
                              maps:get(relics, Snapshot2, #{}),
                              maps:get(items, Snapshot2, []),
                              maps:get(quotes, Snapshot2, #{}),
                              Player,
                              build_options(Request)),
                    {{ok, Reply#{<<"quote_errors">> => QuoteErrors}}, Snapshot2, Next1}
            end
    end.

valid_relic_era(Era) ->
    lists:member(Era, [<<"lith">>, <<"meso">>, <<"neo">>, <<"axi">>,
                       <<"requiem">>, <<"all">>]).

build_options(Request) ->
    #{view => maps:get(view, Request, recommendations),
      limit => maps:get(limit, Request, 32),
      only_owned => maps:get(only_owned, Request, true)}.

maybe_fetch_recommendation_prices(Request, Era, Player, SubmittedAt, Snapshot, Next) ->
    case maps:get(fetch_prices, Request, false) of
        true ->
            Slugs = wfcli_relic_recommendations:price_slugs(
                      Era,
                      maps:get(relics, Snapshot, #{}),
                      maps:get(items, Snapshot, []),
                      Player,
                      ?RECOMMENDATION_PRICE_CANDIDATES,
                      maps:get(only_owned, Request, true)),
            PriceRequest = Request#{ttl => 900, refresh => false},
            fetch_quotes(Slugs, PriceRequest, SubmittedAt, Snapshot, Next, #{});
        false -> {Snapshot, Next, #{}}
    end.

ensure_relic_catalog(Snapshot) ->
    Relics = maps:get(relics, Snapshot, #{}),
    FetchedAt = maps:get(relics_fetched_at, Snapshot, 0),
    AttemptedAt = maps:get(relics_attempted_at, Snapshot, 0),
    Now = erlang:system_time(millisecond),
    Fresh = relic_catalog_ready(Snapshot) andalso
            ((is_integer(FetchedAt) andalso
              Now - FetchedAt =< ?RELIC_CATALOG_TTL * 1000) orelse
             (is_integer(AttemptedAt) andalso
              Now - AttemptedAt =< ?RELIC_CATALOG_RETRY_TTL * 1000)),
    case Fresh of
        true -> {ok, Snapshot};
        false ->
            case wfcli_relic_recommendations:fetch() of
                {ok, NewRelics} ->
                    {ok, Snapshot#{relics => NewRelics,
                                  relics_version =>
                                      wfcli_relic_recommendations:catalog_version(),
                                  relics_fetched_at => Now,
                                  relics_attempted_at => Now,
                                  updated_at => Now}};
                {error, Reason} when map_size(Relics) > 0 ->
                    {ok, Snapshot#{relics_attempted_at => Now,
                                  relics_stale_reason => retinue_text:to_list(Reason)}};
                {error, Reason} ->
                    {error, Reason, Snapshot#{relics_attempted_at => Now}}
            end
    end.

relic_catalog_ready(Snapshot) ->
    map_size(maps:get(relics, Snapshot, #{})) > 0 andalso
    maps:get(relics_version, Snapshot, undefined) =:=
        wfcli_relic_recommendations:catalog_version().

quote_query(Ast, Request, SubmittedAt, Snapshot0, Next0) ->
    QueryRequest = query_request(Request),
    case wfcli_market_query:execute(Ast, QueryRequest, Snapshot0) of
        {ok, Initial} ->
            Results = maps:get(results, Initial),
            Total = maps:get(total, Results),
            ExplicitLimit = maps:is_key(limit, Request),
            case not ExplicitLimit andalso Total > ?MAX_QUOTES of
                true ->
                    {{error, {market_query_too_broad, Total, ?MAX_QUOTES}}, Snapshot0, Next0};
                false ->
                    Entries = maps:get(slice, Results),
                    case length(Entries) > ?HARD_MAX_QUOTES of
                        true ->
                            {{error, {market_quote_limit_exceeded, length(Entries),
                                      ?HARD_MAX_QUOTES}}, Snapshot0, Next0};
                        false ->
                            Slugs = [maps:get(slug, Entry) || Entry <- Entries],
                            {Snapshot, Next, Errors} = fetch_quotes(
                                Slugs, Request, SubmittedAt, Snapshot0, Next0, #{}),
                            {ok, Result} = wfcli_market_query:execute(Ast, QueryRequest, Snapshot),
                            Reply = Result#{context => wfcli_market_api:context(),
                                            quote_errors => Errors},
                            {{ok, Reply}, Snapshot, Next}
                    end
            end;
        {error, _Reason} = Error -> {Error, Snapshot0, Next0}
    end.

query_request(Request) ->
    Base = #{offset => maps:get(offset, Request, 0),
             output_format => maps:get(output_format, Request, table),
             raw => maps:get(raw, Request, false)},
    case maps:find(limit, Request) of
        {ok, Limit} -> Base#{limit => Limit};
        error -> Base#{limit => infinity}
    end.

quote_items(Items, Request, SubmittedAt, Snapshot0, Next0) ->
    {Resolved, Missing} = resolve_items(Items, maps:get(items, Snapshot0, [])),
    case length(Items) > ?HARD_MAX_QUOTES orelse length(Resolved) > ?HARD_MAX_QUOTES of
        true ->
            {{error, {market_quote_limit_exceeded, length(Items), ?HARD_MAX_QUOTES}},
             Snapshot0, Next0};
        false ->
            Slugs = unique([Slug || {_Item, Slug} <- Resolved], []),
            {Snapshot, Next, Errors} = fetch_quotes(
                Slugs, Request, SubmittedAt, Snapshot0, Next0, #{}),
            Quotes = maps:get(quotes, Snapshot),
            Descriptors = descriptors_by_slug(
                            [Slug || {_Item, Slug} <- Resolved],
                            maps:get(items, Snapshot, [])),
            Rows = [#{item => Item, slug => Slug,
                      item_data => maps:get(Slug, Descriptors, undefined),
                      quote => maps:get(Slug, Quotes, undefined),
                      error => maps:get(Slug, Errors, undefined)}
                    || {Item, Slug} <- Resolved],
            Reply = #{context => wfcli_market_api:context(), quotes => Rows, missing => Missing},
            {{ok, Reply}, Snapshot, Next}
    end.

quote_items_cached(Items, _Request, Snapshot, Next) ->
    {Resolved, Missing} = resolve_items(Items, maps:get(items, Snapshot, [])),
    case length(Items) > ?HARD_MAX_QUOTES orelse length(Resolved) > ?HARD_MAX_QUOTES of
        true ->
            {{error, {market_quote_limit_exceeded, length(Items), ?HARD_MAX_QUOTES}},
             Snapshot, Next};
        false ->
            Quotes = maps:get(quotes, Snapshot, #{}),
            Descriptors = descriptors_by_slug(
                            [Slug || {_Item, Slug} <- Resolved],
                            maps:get(items, Snapshot, [])),
            Rows = lists:filtermap(
                     fun({Item, Slug}) ->
                         case maps:find(Slug, Quotes) of
                             {ok, Quote} ->
                                 {true, #{item => Item, slug => Slug,
                                          item_data => maps:get(
                                                         Slug, Descriptors, undefined),
                                          quote => Quote}};
                             error -> false
                         end
                     end,
                     Resolved),
            Reply = #{context => wfcli_market_api:context(), quotes => Rows,
                      missing => Missing},
            {{ok, Reply}, Snapshot, Next}
    end.

resolve_items(Values, Items) ->
    ById = maps:from_list([{string:lowercase(retinue_text:to_list(
                               maps:get(<<"id">>, Item))),
                            maps:get(<<"slug">>, Item)}
                           || Item <- Items, maps:is_key(<<"id">>, Item)]),
    BySlug = maps:from_list([{string:lowercase(retinue_text:to_list(
                                 maps:get(<<"slug">>, Item))),
                              maps:get(<<"slug">>, Item)} || Item <- Items]),
    ByName = maps:from_list([{string:lowercase(retinue_text:to_list(item_name(Item))),
                              maps:get(<<"slug">>, Item)} || Item <- Items]),
    {Found0, Missing0} = lists:foldl(fun(Value, {Found, Missing}) ->
        Key = string:lowercase(string:trim(retinue_text:to_list(Value))),
        case maps:get(Key, ById,
                      maps:get(Key, BySlug, maps:get(Key, ByName, undefined))) of
            undefined -> {Found, [Value | Missing]};
            Slug -> {[{Value, Slug} | Found], Missing}
        end
    end, {[], []}, Values),
    {lists:reverse(Found0), lists:reverse(Missing0)}.

descriptors_by_slug(Slugs, Items) ->
    {Descriptors, _Missing} = wfcli_market_resolver:describe(
                                Slugs, Items),
    maps:from_list([{maps:get(slug, Item), Item} || Item <- Descriptors]).

fetch_quotes([], _Request, _SubmittedAt, Snapshot, Next, Errors) ->
    {Snapshot, Next, Errors};
fetch_quotes([Slug | Rest], Request, SubmittedAt, Snapshot0, Next0, Errors0) ->
    Quotes0 = maps:get(quotes, Snapshot0, #{}),
    Cached = maps:get(Slug, Quotes0, undefined),
    case quote_cache_state(Cached, Request, SubmittedAt) of
        fresh ->
            Source = case maps:get(quoted_at, Cached, 0) >= SubmittedAt of
                true -> coalesced;
                false -> cached
            end,
            Quote = Cached#{source => Source, stale => false},
            Snapshot = Snapshot0#{quotes => Quotes0#{Slug => Quote}},
            fetch_quotes(Rest, Request, SubmittedAt, Snapshot, Next0, Errors0);
        stale ->
            fetch_quotes(Rest, Request, SubmittedAt, Snapshot0, Next0, Errors0);
        fetch ->
            case wfcli_market_api:fetch_quote(Slug, Next0) of
                {ok, Quote, Next} ->
                    Snapshot = Snapshot0#{quotes => Quotes0#{Slug => Quote},
                                          updated_at => erlang:system_time(millisecond)},
                    fetch_quotes(Rest, Request, SubmittedAt, Snapshot, Next, Errors0);
                {error, Reason, Next} ->
                    quote_failure(Rest, Slug, Reason, Cached, Request, SubmittedAt,
                                  Snapshot0, Next, Errors0)
            end
    end.

fetch_details([], Snapshot, Next, Errors) ->
    {Snapshot, Next, Errors};
fetch_details([Slug | Rest], Snapshot0, Next0, Errors0) ->
    Details0 = maps:get(details, Snapshot0, #{}),
    Cached = maps:get(Slug, Details0, undefined),
    case detail_fresh(Cached) of
        true -> fetch_details(Rest, Snapshot0, Next0, Errors0);
        false ->
            case wfcli_market_api:fetch_item(Slug, Next0) of
                {ok, Data, Next} ->
                    Entry = #{data => Data,
                              fetched_at => erlang:system_time(millisecond)},
                    Snapshot = Snapshot0#{details => Details0#{Slug => Entry},
                                          updated_at => erlang:system_time(millisecond)},
                    fetch_details(Rest, Snapshot, Next, Errors0);
                {error, Reason, Next} when is_map(Cached) ->
                    fetch_details(Rest, Snapshot0, Next,
                                  Errors0#{Slug => Reason});
                {error, Reason, Next} ->
                    fetch_details(Rest, Snapshot0, Next,
                                  Errors0#{Slug => Reason})
            end
    end.

detail_fresh(#{fetched_at := FetchedAt}) when is_integer(FetchedAt) ->
    erlang:system_time(millisecond) - FetchedAt =< ?DETAIL_TTL * 1000;
detail_fresh(_Cached) -> false.

root_slugs(Slugs, Snapshot) ->
    Items = maps:get(items, Snapshot, []),
    ById = maps:from_list([{maps:get(<<"id">>, Item), Item}
                           || Item <- Items, maps:is_key(<<"id">>, Item)]),
    Details = maps:get(details, Snapshot, #{}),
    lists:filtermap(
      fun(Slug) ->
          Data = maps:get(data, maps:get(Slug, Details, #{}), #{}),
          PartIds = maps:get(<<"setParts">>, Data, []),
          Roots = [maps:get(<<"slug">>, Item)
                   || Id <- PartIds,
                      Item <- [maps:get(Id, ById, #{})],
                      market_set_root(Item),
                      maps:is_key(<<"slug">>, Item)],
          case Roots of [Root | _] -> {true, Root}; [] -> false end
      end,
      Slugs).

market_set_root(Item) ->
    maps:get(<<"setRoot">>, Item, false) =:= true orelse
    lists:member(<<"set">>, maps:get(<<"tags">>, Item, [])).

item_catalog() ->
    case wfcli_item_catalog:load() of
        {ok, Catalog, _Meta} -> Catalog;
        {error, _Reason} -> []
    end.

player_snapshot() ->
    case whereis(wfcli_player_service) of
        undefined -> #{data => #{}};
        _Pid -> wfcli_player_service:snapshot()
    end.

quote_failure(Rest, Slug, Reason, Cached, Request, SubmittedAt, Snapshot0, Next, Errors0) ->
    case Cached of
        undefined ->
            fetch_quotes(Rest, Request, SubmittedAt, Snapshot0, Next,
                         Errors0#{Slug => Reason});
        _ ->
            Stale = Cached#{stale => true, source => stale,
                            stale_reason => retinue_text:to_list(Reason),
                            last_attempt_at => erlang:system_time(millisecond)},
            Quotes = maps:get(quotes, Snapshot0, #{}),
            Snapshot = Snapshot0#{quotes => Quotes#{Slug => Stale}},
            fetch_quotes(Rest, Request, SubmittedAt, Snapshot, Next, Errors0)
    end.

quote_cache_state(undefined, _Request, _SubmittedAt) -> fetch;
quote_cache_state(Quote, Request, SubmittedAt) ->
    QuotedAt = maps:get(quoted_at, Quote, 0),
    case maps:get(stale, Quote, false) of
        true ->
            LastAttempt = maps:get(last_attempt_at, Quote, 0),
            case is_integer(LastAttempt) andalso
                 erlang:system_time(millisecond) - LastAttempt =< ?FAILURE_RETRY_TTL * 1000 of
                true -> stale;
                false -> fetch
            end;
        false ->
            case is_integer(QuotedAt) of
                false -> fetch;
                true ->
                    case maps:get(refresh, Request, false) of
                        true -> case QuotedAt >= SubmittedAt of true -> fresh; false -> fetch end;
                        false ->
                            Ttl0 = maps:get(ttl, Request, ?DEFAULT_QUOTE_TTL),
                            Ttl = case Ttl0 of
                                Value when is_integer(Value), Value >= 0 -> Value;
                                _ -> ?DEFAULT_QUOTE_TTL
                            end,
                            case erlang:system_time(millisecond) - QuotedAt =< Ttl * 1000 of
                                true -> fresh;
                                false -> fetch
                            end
                    end
            end
    end.

ensure_manifest(_Request, _SubmittedAt, Snapshot, Next0) ->
    FetchedAt = maps:get(manifest_fetched_at, Snapshot, 0),
    AttemptedAt = maps:get(manifest_attempted_at, Snapshot, 0),
    Items = maps:get(items, Snapshot, []),
    Now = erlang:system_time(millisecond),
    Fresh = Items =/= [] andalso
            ((is_integer(FetchedAt) andalso Now - FetchedAt =< ?MANIFEST_TTL * 1000) orelse
             (is_integer(AttemptedAt) andalso
              Now - AttemptedAt =< ?FAILURE_RETRY_TTL * 1000)),
    case Fresh of
        true -> {ok, Snapshot, Next0};
        false ->
            case wfcli_market_api:fetch_items(Next0) of
                {ok, NewItems, Next} ->
                    {ok, Snapshot#{items => NewItems, manifest_fetched_at => Now,
                                  manifest_attempted_at => Now, updated_at => Now}, Next};
                {error, Reason, Next} when Items =/= [] ->
                    {ok, Snapshot#{manifest_stale_reason => retinue_text:to_list(Reason),
                                  manifest_attempted_at => Now}, Next};
                {error, Reason, Next} -> {error, Reason, Snapshot, Next}
            end
    end.

item_name(Item) ->
    maps:get(<<"name">>, maps:get(<<"en">>, maps:get(<<"i18n">>, Item, #{}), #{}),
             maps:get(<<"slug">>, Item, <<>>)).

unique([], Acc) -> lists:reverse(Acc);
unique([Value | Rest], Acc) ->
    case lists:member(Value, Acc) of
        true -> unique(Rest, Acc);
        false -> unique(Rest, [Value | Acc])
    end.

worker_snapshot(Request, Snapshot) ->
    Base = [items, manifest_fetched_at, manifest_attempted_at,
            manifest_stale_reason, updated_at],
    ActionKeys = case maps:get(action, Request, undefined) of
        query -> [quotes];
        quote_query -> [quotes];
        quote_items -> [quotes];
        quote_variant -> [variant_quotes];
        describe_items -> [];
        relic_context -> [quotes, details | relic_keys()];
        relic_recommendations -> [quotes | relic_keys()];
        _ -> []
    end,
    maps:with(Base ++ ActionKeys, Snapshot).

relic_keys() ->
    [relics, relics_version, relics_fetched_at, relics_attempted_at,
     relics_stale_reason].

snapshot_delta(Before, After) ->
    maps:filter(
      fun(Key, Value) -> maps:get(Key, Before, '$missing') =/= Value end,
      After).

mark_cache_dirty(Delta, State) when map_size(Delta) =:= 0 -> State;
mark_cache_dirty(_Delta, State = #{persist_timer := undefined}) ->
    Timer = erlang:send_after(?PERSIST_DELAY_MS, self(), persist_cache),
    State#{cache_dirty => true, persist_timer => Timer};
mark_cache_dirty(_Delta, State) -> State#{cache_dirty => true}.

flush_cache(State = #{cache_dirty := false}) -> State;
flush_cache(State) ->
    case wfcli_market_cache:persist(maps:get(cache_path, State), maps:get(snapshot, State)) of
        ok -> State#{cache_dirty => false, cache_error => undefined};
        {error, Reason} ->
            logger:warning("market cache write failed: ~p", [Reason]),
            Timer = erlang:send_after(?PERSIST_RETRY_MS, self(), persist_cache),
            State#{persist_timer => Timer, cache_error => Reason}
    end.

cancel_timer(undefined) -> ok;
cancel_timer(Timer) ->
    _ = erlang:cancel_timer(Timer),
    ok.

complete_job(#{client := Client, ref := Ref, monitor := Monitor}, Reply, State) ->
    case maps:is_key(Monitor, maps:get(monitors, State)) of
        true -> Client ! {retinue_daemon, Ref, Reply};
        false -> ok
    end,
    erlang:demonitor(Monitor, [flush]),
    retinue_activity:activity_end(),
    State#{monitors => maps:remove(Monitor, maps:get(monitors, State))}.

cancel_client(Monitor, State) ->
    case maps:take(Monitor, maps:get(monitors, State)) of
        error -> {not_found, State};
        {Ref, Monitors} ->
            Queue = queue:filter(fun(Job) -> maps:get(ref, Job) =/= Ref end,
                                 maps:get(queue, State)),
            ReadQueue = queue:filter(fun(Job) -> maps:get(ref, Job) =/= Ref end,
                                     maps:get(read_queue, State)),
            Current = maps:get(current, State, undefined),
            Current1 = case Current of
                #{job := #{ref := Ref}} -> stop_worker(Current), undefined;
                _ -> Current
            end,
            Reads = cancel_read(Ref, maps:get(reads, State, #{})),
            retinue_activity:activity_end(),
            {canceled, State#{queue => Queue, read_queue => ReadQueue,
                              current => Current1, reads => Reads,
                              monitors => Monitors}}
    end.

cancel_read(Ref, Reads) ->
    maps:filter(
      fun(_Token, Worker = #{job := Job}) ->
          case maps:get(ref, Job) =:= Ref of
              true -> stop_worker(Worker), false;
              false -> true
          end
      end,
      Reads).

take_read_by_monitor(Monitor, Reads) ->
    case [{Token, Worker} ||
             {Token, Worker = #{worker_monitor := WorkerMonitor}} <- maps:to_list(Reads),
             WorkerMonitor =:= Monitor] of
        [{Token, #{job := Job}}] -> {ok, Job, maps:remove(Token, Reads)};
        [] -> error
    end.

stop_worker(undefined) -> ok;
stop_worker(Current) ->
    case maps:get(worker_pid, Current, undefined) of
        Pid when is_pid(Pid) -> exit(Pid, kill);
        undefined -> ok
    end,
    erlang:demonitor(maps:get(worker_monitor, Current), [flush]),
    ok.

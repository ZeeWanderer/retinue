%%%-------------------------------------------------------------------
%% Unified-query adapter for daemon diagnostics.
%%%-------------------------------------------------------------------
-module(wf_diagnostics_query).

-export([execute/3, execute/4, query_field/2, query_sort_field/2, default_sort/1]).

-doc "Compile and execute the query AST against current resolution issues.".
-spec execute(term(), map(), [map()]) -> {ok, map()} | {error, term()}.
execute(Ast, Request, Issues) -> execute(diagnostics, Ast, Request, Issues).

execute(Kind, Ast0, Request, Issues) ->
    case wf_query_parse:extract_control(Ast0, sort) of
        {ok, Ast, SortControls} -> execute_sorted(Kind, Ast, SortControls, Request, Issues);
        {error, Error} -> {error, {query_errors, [Error]}}
    end.

execute_sorted(Kind, Ast, SortControls, Request, Issues) ->
    case compile_sorts(SortControls) of
        {ok, []} -> run(Kind, Ast, default_sort(Kind), Request, Issues);
        {ok, Sorts} -> run(Kind, Ast, Sorts, Request, Issues);
        {error, Error} -> {error, {query_errors, [Error]}}
    end.

run(Kind, Ast, Sorts, Request, Issues) ->
    case {wf_entity_query:compile(Ast, ?MODULE, Kind),
          wf_entity_query:compile_sorts(Sorts, ?MODULE, Kind)} of
        {{ok, Compiled}, {ok, CompiledSorts}} ->
            Entries = [entry(Kind, Issue) || Issue <- Issues],
            Results = wf_entity_query:execute(
                        Entries, Compiled, CompiledSorts, ?MODULE, Kind,
                        maps:get(offset, Request, 0),
                        maps:get(limit, Request, infinity)),
            {ok, #{query => #{query => Compiled, compiled_sort => CompiledSorts,
                              output_format => maps:get(output_format, Request, table),
                              raw => maps:get(raw, Request, false)},
                   results => Results#{kind => Kind}}};
        {{error, Errors}, _} -> {error, {query_errors, Errors}};
        {_, {error, Errors}} -> {error, {query_errors, Errors}}
    end.

compile_sorts(Controls) -> compile_sorts(Controls, []).

compile_sorts([], Acc) -> {ok, lists:reverse(Acc)};
compile_sorts([#{op := Op, vals := Values} | Rest], Acc)
  when Op =:= eq; Op =:= default ->
    Parsed = [wf_query_sort:parse(Value) || Value <- Values],
    compile_sorts(Rest, lists:reverse(Parsed) ++ Acc);
compile_sorts([_ | _], _Acc) ->
    {error, "sort supports only '=' or ':'"}.

entry(Kind, Row) when Kind =:= incidents; Kind =:= captures ->
    wf_entity:build(Kind, maps:get(<<"id">>, Row), maps:get(<<"name">>, Row),
                       Row, #{search_raw => true}, #{});
entry(diagnostics, Issue) ->
    Identity = maps:get(<<"identity">>, Issue),
    Name = maps:get(<<"fallback">>, Issue, Identity),
    Kind = maps:get(<<"kind">>, Issue),
    Spec = #{row_map_fun => fun(_Entry) ->
        #{kind => Kind, identity => Identity, name => Name,
          scope => maps:get(<<"scope">>, Issue, <<>>),
          reason => maps:get(<<"reason">>, Issue, <<>>),
          count => maps:get(<<"count">>, Issue, 0),
          last_seen => maps:get(<<"last_seen">>, Issue, 0)}
    end},
    Base = wf_entity:build(resolution_issue, Identity, Name, Issue,
                              #{search_raw => true}, Spec),
    Base#{kind => Kind, identity => Identity,
          scope => maps:get(<<"scope">>, Issue, <<>>),
          reason => maps:get(<<"reason">>, Issue, <<>>),
          fallback => Name,
          class => maps:get(<<"class">>, Issue, <<>>),
          collection => maps:get(<<"collection">>, Issue, <<>>),
          attempts => maps:get(<<"attempts">>, Issue, []),
          catalog_revision => maps:get(<<"catalog_revision">>, Issue, <<>>),
          count => maps:get(<<"count">>, Issue, 0),
          first_seen => maps:get(<<"first_seen">>, Issue, 0),
          last_seen => maps:get(<<"last_seen">>, Issue, 0)}.

-doc "Resolve diagnostics filter fields.".
-spec query_field(term(), string() | atom()) -> {ok, map()} | error.
query_field(Kind, Key0) when Kind =:= incidents; Kind =:= captures ->
    Key = string:lowercase(retinue_text:to_list(Key0)),
    Fields = case Kind of
        incidents -> [{"timestamp", time_point}, {"timestamp_ms", time_point},
                      {"application", string}, {"level", string}, {"event", string},
                      {"message", string}, {"path", string}];
        captures -> [{"timestamp", time_point}, {"source", string}, {"state", string},
                     {"current", string}, {"error", string}, {"directory", string},
                     {"expires_at", time_point}]
    end,
    case lists:keyfind(Key, 1, [{"id", string}, {"name", string} | Fields]) of
        {Key, Type} -> field(list_to_binary(Key), {data, list_to_binary(Key)}, Type, eq);
        false ->
            case retinue_text:to_list(Key0) of
                "data." ++ Path -> field(Key, {data_path, Path}, dynamic, contains);
                _ -> error
            end
    end;
query_field(diagnostics, Key0) ->
    case string:lowercase(retinue_text:to_list(Key0)) of
        "name" -> field(name, {entry, name}, string, contains);
        "kind" -> field(kind, {entry, kind}, string, eq);
        "identity" -> field(identity, {entry, identity}, string, contains);
        "scope" -> field(scope, {entry, scope}, string, eq);
        "reason" -> field(reason, {entry, reason}, string, contains);
        "fallback" -> field(fallback, {entry, fallback}, string, contains);
        "class" -> field(class, {entry, class}, string, eq);
        "collection" -> field(collection, {entry, collection}, string, eq);
        "attempts" -> field(attempts, {entry_values, attempts}, string, eq);
        "catalog_revision" -> field(catalog_revision, {entry, catalog_revision}, string, eq);
        "count" -> field(count, {entry, count}, number, eq);
        "first_seen" -> field(first_seen, {entry, first_seen}, time_point, eq);
        "last_seen" -> field(last_seen, {entry, last_seen}, time_point, eq);
        _ -> error
    end.

-doc "Diagnostics sort fields use the filter-field contract.".
-spec query_sort_field(term(), string() | atom()) -> {ok, map()} | error.
query_sort_field(Kind, Key) -> query_field(Kind, Key).

-doc "Keep diagnostics stable by issue kind and identity.".
-spec default_sort(term()) -> [map()].
default_sort(incidents) -> [#{key => timestamp, dir => desc}];
default_sort(captures) -> [#{key => name, dir => asc}];
default_sort(diagnostics) -> [#{key => kind, dir => asc}, #{key => identity, dir => asc}].

field(Key, Source, Kind, DefaultOp) ->
    {ok, #{key => Key, source => Source, kind => Kind, default_op => DefaultOp}}.

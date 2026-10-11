-module(wf_query_cli).

-export([command/0, player_command/0, run/1]).
-import(retinue_cli_args, [option/4, flag/3]).

command() ->
    #{help => "search the indexed knowledge base", handler => {?MODULE, run},
      defaults => #{refresh => false, raw => false},
      arguments => [flag(refresh, "refresh", "refresh cached data"),
                    (option(ttl, "ttl", {integer, [{min, 60}]}, "cache freshness in seconds"))#{
                        default => 60},
                    option(cache, "cache", string, "worldstate cache file"),
                    option(event_lang, "lang", string, "event language"),
                    option(exports_dir, "exports-dir", string, "official export directory"),
                    option(knowledge_dir, "knowledge-dir", string, "WFCD cache directory"),
                    option(limit, "limit", {integer, [{min, 0}]}, "maximum results"),
                    (option(offset, "offset", {integer, [{min, 0}]}, "skip results"))#{
                        default => 0},
                    flag(raw, "raw", "include raw identifiers")] ++
                   retinue_cli_args:format([table, block], table) ++ retinue_cli_args:query()}.

player_command() ->
    (command())#{help => "inspect or query local player data",
                 defaults => #{dataset => player, refresh => false, raw => false}}.

run(#{dataset := player, query_tokens := [], output_format := json}) ->
    case retinue_client:call(warframe, player_snapshot) of
        {ok, Snapshot} ->
            retinue_output:json(Snapshot#{updated_at := {timestamp, maps:get(updated_at, Snapshot)},
                                       data := {json, maps:get(data, Snapshot)}});
        {error, Reason} -> fail([retinue_client:format_error(Reason)])
    end;
run(#{dataset := player, query_tokens := []}) ->
    case retinue_client:call(warframe, player_snapshot) of
        {ok, Snapshot} when is_map(Snapshot) -> wf_player_format:print_snapshot(Snapshot);
        {error, Reason} -> fail([retinue_client:format_error(Reason)])
    end;
run(#{dataset := player, query_tokens := Tokens} = Parsed) ->
    run_query(Parsed#{query_tokens := ["dataset=player" | Tokens]});
run(#{query_tokens := []}) -> retinue_cli_runtime:fail("query requires an expression");
run(Parsed) -> run_query(Parsed).

run_query(Parsed) ->
    Request = maps:without([dataset], Parsed#{source => query, cwd => filename:absname(".")}),
    case retinue_client:one_shot(warframe, Request) of
        {ok, #{datasets := Datasets, query_tokens := Tokens}} ->
            Success = case maps:get(output_format, Parsed, table) of
                json ->
                    retinue_output:json(#{datasets => [json_dataset(Result) || Result <- Datasets]}),
                    lists:all(fun(#{reply := Reply}) -> element(1, Reply) =:= ok end, Datasets);
                _ ->
                    Query = string:join(Tokens, " "),
                    lists:all(fun(Succeeded) -> Succeeded end,
                              [print_dataset(Result, Query, Parsed) || Result <- Datasets])
            end,
            case Success of true -> ok; false -> halt(1) end;
        {error, {query_errors, Errors}} -> fail(Errors);
        {error, Reason} -> fail([retinue_client:format_error(Reason)])
    end.

json_dataset(#{dataset := Dataset, reply := {error, Reason}}) ->
    #{dataset => Dataset, error => retinue_client:format_error(Reason)};
json_dataset(#{dataset := Dataset, reply := {ok, #{results := Results}}}) ->
    #{dataset => Dataset, results => retinue_output:results(Results)};
json_dataset(#{dataset := Dataset, reply := {ok, Result}}) ->
    (wf_worldstate_output:json_result(Result))#{dataset => Dataset}.

print_dataset(#{dataset := Dataset, reply := {error, Reason}}, _Query, _Parsed) ->
    io:format("== ~s ==~nerror: ~ts~n", [dataset_title(Dataset),
                                          retinue_client:format_error(Reason)]),
    false;
print_dataset(#{dataset := worldstate, reply := {ok, Result}}, Query, Parsed) ->
    io:format("== Worldstate ==~n"),
    print_worldstate_result(Result, Query, Parsed),
    true;
print_dataset(#{dataset := player, reply := {ok, Result}}, _Query, _Parsed) ->
    io:format("== Player ==~n"),
    wf_player_format:print_query(maps:get(query, Result), maps:get(results, Result)),
    true;
print_dataset(#{dataset := market, reply := {ok, Result}}, _Query, _Parsed) ->
    io:format("== Market ==~n"),
    wf_market_format:print(maps:get(query, Result), maps:get(results, Result), #{}, #{}),
    true;
print_dataset(#{dataset := Dataset, reply := {ok, Result}}, _Query, _Parsed)
  when Dataset =:= diagnostics; Dataset =:= incidents; Dataset =:= captures ->
    io:format("== ~s ==~n", [dataset_title(Dataset)]),
    wf_diagnostics_format:print_query(maps:get(query, Result),
                                         maps:get(results, Result)),
    true;
print_dataset(#{dataset := Dataset, reply := {ok, Result}}, _Query, _Parsed) ->
    io:format("== ~s ==~n", [dataset_title(Dataset)]),
    Prepared = maps:get(query, Result),
    Results = maps:get(results, Result),
    case Dataset of
        mods -> wf_exports_format:print("mods", Prepared, Results);
        items -> wf_exports_format:print("items", Prepared, Results);
        _ -> wf_knowledge_format:print(Prepared, Results)
    end,
    true.

print_worldstate_result(Result, Query, Parsed) ->
    case print_worldstate_query_errors(maps:get(parsed_query, Result, #{})) of
        error -> ok;
        ok ->
            wf_worldstate_output:print_daemon_source(Result),
            Matches = maps:get(entries, Result, []),
            case Matches of
                [] -> io:format("no matches for ~ts~n", [Query]);
                _ ->
                    io:format("Matches for ~ts: ~p~n~n", [Query, length(Matches)]),
                    Extracts = maps:get(extracts, maps:get(parsed_query, Result, #{}), []),
                    case Extracts of
                        [] ->
                            Format = maps:get(output_format, Parsed, table),
                            wf_worldstate_output:print_entries(
                              Matches, maps:get(opts, Result, worldstate_opts(Parsed)),
                              Format, worldstate_columns(Format));
                        _ ->
                            wf_worldstate_output:print_query_extracts(Matches, Extracts)
                    end
            end
    end.

dataset_title(worldstate) -> "Worldstate";
dataset_title(mods) -> "Mods";
dataset_title(items) -> "Items";
dataset_title(codex) -> "Codex";
dataset_title(enemies) -> "Enemies";
dataset_title(drops) -> "Drops";
dataset_title(player) -> "Player";
dataset_title(market) -> "Market";
dataset_title(diagnostics) -> "Diagnostics";
dataset_title(incidents) -> "Incidents";
dataset_title(captures) -> "Captures".

fail(Errors) ->
    lists:foreach(fun(Error) -> io:format(standard_error, "error: ~ts~n", [Error]) end, Errors),
    halt(1).

print_worldstate_query_errors(ParsedQuery) ->
    case maps:get(errors, ParsedQuery, []) of
        [] -> ok;
        Errors ->
            lists:foreach(fun(Error) -> io:format(standard_error, "error: ~ts~n", [Error]) end, Errors),
            error
    end.

worldstate_opts(Parsed) ->
    Raw = maps:get(raw, Parsed, false),
    Opts0 = #{refresh => maps:get(refresh, Parsed, false),
              ttl => maps:get(ttl, Parsed, 60),
              resolve_items => not Raw,
              raw => Raw,
              utc => maps:get(utc, Parsed, false),
              search_raw => Raw,
              event_lang => maps:get(event_lang, Parsed, undefined)},
    case maps:get(cache, Parsed, undefined) of
        undefined -> Opts0;
        C -> Opts0#{cache => filename:absname(C)}
    end.

worldstate_columns(table) ->
    wf_worldstate_schema:default_table_columns();
worldstate_columns(_Format) ->
    [].

%%%-------------------------------------------------------------------
%% Query service for official Codex and WFCD knowledge.
%%%-------------------------------------------------------------------
-module(wfcli_knowledge_query).

-export([defaults/1, prepare/2, query/2, load_data/2,
         build_entries/3, execute_entries/4]).

-ifdef(TEST).
-export([decode_daemon_reply/1]).
-endif.

-type command() :: string().

-doc "Return a complete typed request for one knowledge dataset.".
-spec defaults(command()) -> map().
defaults(_Command) ->
    #{filters => [], text => [], query_tokens => [], limit => 50, offset => 0,
      output_format => table, raw => false, sort => [], exports_dir => undefined,
      knowledge_dir => undefined, include_excluded => false, errors => []}.

-doc "Validate and compile a typed knowledge query request.".
-spec prepare(command(), map()) -> {ok, map()} | {error, [iodata()]}.
prepare(Command, Request0) ->
    case command_kind(Command) of
        undefined -> {error, [io_lib:format("unknown knowledge command: ~s", [Command])]};
        _ ->
            Request = maps:merge(defaults(Command), Request0),
            finalize_query(Command, Request)
    end.

-doc "Prepare and run a typed request directly; production callers queue through retinue_daemon.".
-spec query(command(), map()) -> {ok, map(), map()} | {error, [iodata()]}.
query(Command, Request) ->
    case prepare(Command, Request) of
        {ok, Prepared} -> local_query(Command, Prepared);
        Error -> Error
    end.

-ifdef(TEST).
-spec decode_daemon_reply(term()) -> {ok, map(), map()} | {error, [iodata()]}.
decode_daemon_reply(Reply) ->
    case Reply of
        {ok, #{query := Prepared, results := Results}} -> {ok, Prepared, Results};
        {error, {query_errors, Errors}} -> {error, Errors};
        {error, Reason} -> {error, [format_error(Reason)]}
    end.
-endif.

local_query(Command, Prepared) ->
    case load_data(Command, Prepared) of
        {ok, Data, Meta} ->
            Entries = build_entries(Command, Prepared, Data),
            execute_entries(Command, Prepared, Entries, Meta);
        {error, Reason} -> {error, [format_error(Reason)]}
    end.

finalize_query(Command, Request) ->
    case maps:get(errors, Request) of
        [] ->
            case request_ast(Request) of
                {ok, QueryAst0} ->
                    case wfcli_query_parse:extract_control(QueryAst0, sort) of
                        {ok, QueryAst, SortControls} ->
                            finalize_compiled_query(Command, Request, QueryAst, SortControls);
                        {error, Error} -> {error, [Error]}
                    end;
                {error, Error} -> {error, [Error]}
            end;
        Errors -> {error, Errors}
    end.

request_ast(#{query_ast := Ast}) -> {ok, Ast};
request_ast(Request) ->
    wfcli_query_parse:parse_arguments(maps:get(query_tokens, Request, [])).

finalize_compiled_query(Command, Request, QueryAst, SortControls) ->
    Kind = command_kind(Command),
    FlagAst = wfcli_query_parse:and_all(
      [{filter, maps:get(key, Filter), maps:get(op, Filter), maps:get(vals, Filter)}
       || Filter <- maps:get(filters, Request)] ++
      [{term, Text} || Text <- maps:get(text, Request)]),
    Combined = wfcli_query_parse:and_all([FlagAst, QueryAst]),
    case controls_to_sorts(SortControls) of
        {ok, Sorts0} ->
            Sorts = case Sorts0 of [] -> wfcli_entity_knowledge:default_sort(Kind); _ -> Sorts0 end,
            case {wfcli_entity_query:compile(Combined, wfcli_entity_knowledge, Kind),
                  wfcli_entity_query:compile_sorts(Sorts, wfcli_entity_knowledge, Kind)} of
                {{ok, Compiled}, {ok, CompiledSorts}} ->
                    {ok, maps:remove(query_ast,
                          Request#{query => Compiled, compiled_sort => CompiledSorts})};
                {{error, Errors}, _} -> {error, Errors};
                {_, {error, Errors}} -> {error, Errors}
            end;
        {error, Error} -> {error, [Error]}
    end.

controls_to_sorts(Controls) -> controls_to_sorts(Controls, []).

controls_to_sorts([], Acc) -> {ok, lists:reverse(Acc)};
controls_to_sorts([#{op := Op, vals := Values} | Rest], Acc)
  when Op =:= eq; Op =:= default ->
    Parsed = [wfcli_query_sort:parse(Value) || Value <- Values],
    controls_to_sorts(Rest, lists:reverse(Parsed) ++ Acc);
controls_to_sorts([_ | _], _Acc) ->
    {error, "sort supports only '=' or ':'"}.

-doc "Load source records and provenance for one knowledge dataset.".
-spec load_data(command(), map()) -> {ok, [map()], map()} | {error, term()}.
load_data("codex", Query) -> wfcli_knowledge:load_codex(maps:get(exports_dir, Query));
load_data("enemies", Query) -> wfcli_knowledge:load_enemies(maps:get(knowledge_dir, Query));
load_data("drops", Query) -> wfcli_knowledge:load_drops(maps:get(knowledge_dir, Query)).

-doc "Build searchable entities; daemon caches these between requests.".
-spec build_entries(command(), map(), [map()]) -> [map()].
build_entries(Command, Query, Data) ->
    Opts = #{raw => maps:get(raw, Query, false), search_raw => true},
    Builder = case Command of
        "codex" -> fun wfcli_entity_knowledge:build_codex/2;
        "enemies" -> fun wfcli_entity_knowledge:build_enemy/2;
        "drops" -> fun wfcli_entity_knowledge:build_drop/2
    end,
    [Builder(Item, Opts) || Item <- Data].

-doc "Filter, sort, and page daemon-cached knowledge entities.".
-spec execute_entries(command(), map(), [map()], map()) -> {ok, map(), map()}.
execute_entries(Command, Query, Entries, Meta) ->
    Kind = command_kind(Command),
    Included = [Entry || Entry <- Entries, include_entry(Kind, Entry, Query)],
    Result0 = wfcli_entity_query:execute(
      Included, maps:get(query, Query), maps:get(compiled_sort, Query),
      wfcli_entity_knowledge, Kind, maps:get(offset, Query), maps:get(limit, Query)),
    {ok, Query, Result0#{source_meta => Meta}}.

include_entry(codex, Entry, Query) ->
    maps:get(include_excluded, Query, false) orelse
        maps:get(excludeFromCodex, maps:get(data, Entry), false) =/= true;
include_entry(_Kind, _Entry, _Query) -> true.

command_kind("codex") -> codex;
command_kind("enemies") -> enemy;
command_kind("drops") -> drop;
command_kind(_) -> undefined.

-doc "Convert service failures into user-facing knowledge errors.".
-spec format_error(term()) -> iodata().
format_error({knowledge_missing, _Path}) ->
    "WFCD knowledge is not cached; run: wfcli update --wfcd";
format_error({bad_knowledge_cache, Path}) ->
    io_lib:format("invalid WFCD knowledge cache: ~s", [Path]);
format_error({bad_knowledge_json, Path}) ->
    io_lib:format("invalid WFCD knowledge JSON: ~s", [Path]);
format_error(Reason) ->
    io_lib:format("knowledge query failed: ~p", [Reason]).

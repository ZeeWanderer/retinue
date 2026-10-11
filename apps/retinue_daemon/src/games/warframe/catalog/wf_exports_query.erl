%%%-------------------------------------------------------------------
%% Query service for official export catalogs.
%%%-------------------------------------------------------------------
-module(wf_exports_query).

-export([defaults/1, prepare/2, query/2, build_entries/3, execute_entries/3]).

-ifdef(TEST).
-export([decode_daemon_reply/1]).
-endif.

-type command() :: string().

-doc "Return a complete typed request for one export dataset.".
-spec defaults(command()) -> map().
defaults("mods") ->
    #{filters => [], text => [], query_tokens => [], limit => 50, offset => 0,
      output_format => table, raw => false, exports_dir => undefined, sort => [], errors => []};
defaults("items") ->
    #{filters => [], text => [], query_tokens => [], limit => 50, offset => 0,
      output_format => table, raw => false, exports_dir => undefined, files => [],
      sort => [], errors => []}.

-doc "Validate and compile a typed export query request.".
-spec prepare(command(), map()) -> {ok, map()} | {error, [iodata()]}.
prepare(Command, Request0) when Command =:= "mods"; Command =:= "items" ->
    finalize_query(Command, maps:merge(defaults(Command), Request0));
prepare(Command, _Request) ->
    {error, [io_lib:format("unknown export command: ~s", [Command])]}.

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
        {error, Reason} -> {error, [io_lib:format("daemon query failed: ~p", [Reason])]}
    end.
-endif.

local_query("mods", Prepared) ->
    case wf_exports:load_mods(maps:get(exports_dir, Prepared)) of
        {ok, Data} -> execute_query("mods", Prepared, Data);
        {error, Reason} -> {error, [format_error(Reason)]}
    end;
local_query("items", Prepared) ->
    case wf_exports:load_items(maps:get(exports_dir, Prepared), maps:get(files, Prepared)) of
        {ok, Data} -> execute_query("items", Prepared, Data);
        {error, Reason} -> {error, [format_error(Reason)]}
    end.

format_error({export_unavailable, File, Path, Reason}) ->
    io_lib:format("required export ~s is unavailable at ~s: ~p", [File, Path, Reason]);
format_error(Reason) ->
    io_lib:format("export query failed: ~p", [Reason]).

finalize_query(Command, Request) ->
    case maps:get(errors, Request) of
        [] ->
            case request_ast(Request) of
                {ok, QueryAst0} ->
                    case wf_query_parse:extract_control(QueryAst0, sort) of
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
    wf_query_parse:parse_arguments(maps:get(query_tokens, Request, [])).

finalize_compiled_query(Command, Request, QueryAst, SortControls) ->
    Kind = command_kind(Command),
    FlagAst = wf_query_parse:and_all(
      [{filter, maps:get(key, Filter), maps:get(op, Filter), maps:get(vals, Filter)}
       || Filter <- maps:get(filters, Request)] ++
      [{term, Text} || Text <- maps:get(text, Request)]),
    Combined = wf_query_parse:and_all([FlagAst, QueryAst]),
    case controls_to_sorts(SortControls) of
        {ok, Sorts0} ->
            Sorts = case Sorts0 of [] -> wf_entity_exports:default_sort(Kind); _ -> Sorts0 end,
            case {wf_entity_query:compile(Combined, wf_entity_exports, Kind),
                  wf_entity_query:compile_sorts(Sorts, wf_entity_exports, Kind)} of
                {{ok, Compiled}, {ok, CompiledSorts}} ->
                    Request1 = maybe_add_query_files(Command, Combined, Request),
                    {ok, maps:remove(query_ast,
                          Request1#{query => Compiled, compiled_sort => CompiledSorts})};
                {{error, Errors}, _} -> {error, Errors};
                {_, {error, Errors}} -> {error, Errors}
            end;
        {error, Error} -> {error, [Error]}
    end.

command_kind("mods") -> mod;
command_kind("items") -> item.

controls_to_sorts(Controls) -> controls_to_sorts(Controls, []).

controls_to_sorts([], Acc) -> {ok, lists:reverse(Acc)};
controls_to_sorts([#{op := Op, vals := Values} | Rest], Acc)
  when Op =:= eq; Op =:= default ->
    Parsed = [wf_query_sort:parse(Value) || Value <- Values],
    controls_to_sorts(Rest, lists:reverse(Parsed) ++ Acc);
controls_to_sorts([_ | _], _Acc) ->
    {error, "sort supports only '=' or ':'"}.

maybe_add_query_files("items", Ast, Request) ->
    case wf_query_parse:constraint_values(Ast, file, eq) of
        {values, Files} -> Request#{files => unique(maps:get(files, Request) ++ Files)};
        unconstrained -> Request
    end;
maybe_add_query_files(_Command, _Ast, Request) -> Request.

unique(Values) ->
    lists:reverse(lists:foldl(
      fun(Value, Acc) ->
          case lists:member(Value, Acc) of true -> Acc; false -> [Value | Acc] end
      end, [], Values)).

-doc "Build and execute an export query without daemon caching.".
-spec execute_query(command(), map(), [map()]) -> {ok, map(), map()}.
execute_query(Command, Query, Data) ->
    execute_entries(Command, Query, build_entries(Command, Query, Data)).

-doc "Build normalized query entities once for daemon caching.".
-spec build_entries(command(), map(), [map()]) -> [map()].
build_entries("mods", Query, Mods) ->
    [wf_entity_exports:build_mod(M, #{raw => maps:get(raw, Query, false)}) || M <- Mods];
build_entries("items", Query, Items) ->
    [wf_entity_exports:build_item(I, #{raw => maps:get(raw, Query, false)}) || I <- Items].

-doc "Filter, sort, and page daemon-cached export entities.".
-spec execute_entries(command(), map(), [map()]) -> {ok, map(), map()}.
execute_entries("mods", Query, Entries) ->
    Results = wf_entity_query:execute(
      Entries, maps:get(query, Query), maps:get(compiled_sort, Query),
      wf_entity_exports, mod, maps:get(offset, Query), maps:get(limit, Query)),
    {ok, Query, Results};
execute_entries("items", Query, Entries) ->
    Results = wf_entity_query:execute(
      Entries, maps:get(query, Query), maps:get(compiled_sort, Query),
      wf_entity_exports, item, maps:get(offset, Query), maps:get(limit, Query)),
    {ok, Query, Results}.

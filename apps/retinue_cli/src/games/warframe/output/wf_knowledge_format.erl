%%%-------------------------------------------------------------------
%% Rendering for Codex and WFCD knowledge query results.
%%%-------------------------------------------------------------------
-module(wf_knowledge_format).

-export([print/2]).

-doc "Render one prepared knowledge query result.".
-spec print(map(), map()) -> ok.
print(Query, Results) ->
    Format = maps:get(output_format, Query),
    Entries = maps:get(slice, Results),
    case Format of
        json -> print_json(Results, Entries);
        _ ->
            print_source(maps:get(source_meta, Results)),
            io:format("Matches: ~p (showing ~p)~n~n",
                      [maps:get(total, Results), maps:get(shown, Results)]),
            case Format of
                table -> print_table(maps:get(kind, Results), Entries);
                block -> lists:foreach(fun print_block/1, Entries)
            end
    end,
    ok.

print_json(Results, Entries) ->
    Payload = #{count => maps:get(total, Results), shown => maps:get(shown, Results),
                source => wf_catalog_json:record(source, maps:get(source_meta, Results)),
                results => [wf_catalog_json:record(maps:get(kind, Results), maps:get(data, E))
                            || E <- Entries]},
    io:format("~ts~n", [jsone:encode(Payload)]).

print_source(Meta) ->
    io:format("Source: ~ts (version: ~ts)~n", [maps:get(source, Meta, "unknown"),
                                               maps:get(version, Meta, "unknown")]).

print_table(_Kind, []) -> io:format("no entries~n", []);
print_table(Kind, Entries) ->
    Columns = wf_knowledge_schema:columns_for_kind(Kind),
    Headers = [maps:get(label, wf_knowledge_schema:column_spec(C)) || C <- Columns],
    Rows = [[cell(maps:get(C, maps:get(row_map, E), ""), C) || C <- Columns] || E <- Entries],
    Specs = [wf_knowledge_schema:column_spec(C) || C <- Columns],
    Lines = retinue_table:render_lines(Headers, Rows, #{column_specs => Specs}),
    lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end, Lines),
    io:format("~n", []).

cell(Value, chance) when is_integer(Value); is_float(Value) ->
    trim_float(Value) ++ "%";
cell(undefined, _Key) -> "";
cell(Value, _Key) -> retinue_text:to_list(Value).

trim_float(Value) ->
    string:trim(string:trim(lists:flatten(io_lib:format("~.6f", [Value])), trailing, "0"), trailing, ".").

print_block(Entry) ->
    Kind = maps:get(type, Entry),
    Row = maps:get(row_map, Entry),
    Spec = wf_knowledge_presentation:block_spec(Kind),
    TitleKey = case Kind of codex -> name; enemy -> name; drop -> item end,
    TitlePrefix = case Kind of codex -> "Codex"; enemy -> "Enemy"; drop -> "Drop" end,
    io:format("~s: ~ts~n", [TitlePrefix, maps:get(TitleKey, Row, "")]),
    lists:foreach(
      fun({Label, Key}) ->
          Value = cell(maps:get(Key, Row, ""), Key),
          case retinue_text:value_present(Value) of
              true -> io:format("  ~s: ~ts~n", [Label, Value]);
              false -> ok
          end
      end, maps:get(fields, Spec)),
    io:format("~n", []).

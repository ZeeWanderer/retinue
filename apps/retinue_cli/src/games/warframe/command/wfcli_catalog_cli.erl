-module(wfcli_catalog_cli).

-export([commands/0, run/1, request/1]).
-import(retinue_cli_args, [option/4, flag/3]).

commands() ->
    [catalog("mods", "query mod exports",
             [filter(type, eq), filter(polarity, eq), filter(rarity, eq),
              filter(compat, contains), exports_dir()]),
     catalog("items", "query export item names", [filter(file, eq), exports_dir()]),
     catalog("codex", "query official Codex knowledge",
             [filter(category, eq), filter(file, eq), exports_dir(),
              flag(include_excluded, "include-excluded", "include hidden Codex records")]),
     catalog("enemies", "query WFCD enemy knowledge",
             [filter(faction, eq), knowledge_dir()]),
     catalog("drops", "find WFCD enemy drops by item or enemy",
             [filter(enemy, contains), filter(rarity, eq), knowledge_dir()])].

catalog(Name, Help, Extra) ->
    {Name, #{help => Help, handler => {?MODULE, run},
             defaults => #{command => Name, filters => [], text => [], sort => [],
                           raw => false, include_excluded => false},
             arguments => [filter(name, contains),
                           (option(text, "text", string, "searchable text (repeatable)"))#{
                               action => append},
                           (option(limit, "limit", {integer, [{min, 0}]}, "maximum results"))#{
                               short => $l, default => 50},
                           (option(offset, "offset", {integer, [{min, 0}]}, "skip results"))#{
                               short => $o, default => 0},
                           flag(raw, "raw", "include raw identifiers")] ++ Extra ++
                          retinue_cli_args:format([table, block, json], table) ++
                          retinue_cli_args:query()}}.

filter(Key, Op) ->
    Type = {custom, fun(Value) ->
        #{key => Key, op => Op,
          vals => [string:trim(V) || V <- string:split(Value, "|", all), V =/= ""]}
    end},
    (option(filters, atom_to_list(Key), Type,
            atom_to_list(Key) ++ " filter (repeatable)"))#{action => append}.

exports_dir() -> option(exports_dir, "exports-dir", string, "official export directory").
knowledge_dir() -> option(knowledge_dir, "knowledge-dir", string, "WFCD cache directory").

request(#{command := "items", filters := Filters} = Args) ->
    Args#{files => lists:append([Vals || #{key := file, vals := Vals} <- Filters])};
request(Args) -> Args.

run(#{command := Command} = Args) ->
    case wfcli_catalog_client:query(Command, request(Args)) of
        {ok, Prepared, Results} when Command =:= "mods"; Command =:= "items" ->
            wfcli_exports_format:print(Command, Prepared, Results);
        {ok, Prepared, Results} -> wfcli_knowledge_format:print(Prepared, Results);
        {error, Errors} -> retinue_cli_runtime:fail(lists:join("\n", Errors))
    end.

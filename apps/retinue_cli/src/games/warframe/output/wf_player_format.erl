%%%-------------------------------------------------------------------
%% Terminal presentation for player snapshots and query results.
%%%-------------------------------------------------------------------
-module(wf_player_format).

-export([print_snapshot/1, print_query/2]).

-doc "Render current player snapshot for the dedicated player command.".
-spec print_snapshot(map()) -> ok.
print_snapshot(Snapshot) ->
    Data = maps:get(data, Snapshot, #{}),
    io:format("Player dataset~n"),
    io:format("  revision: ~p~n", [maps:get(revision, Snapshot, 0)]),
    io:format("  updated: ~s~n", [retinue_output:timestamp(maps:get(updated_at, Snapshot, undefined))]),
    io:format("  sources: ~s~n", [source_list(maps:keys(Data))]),
    print_game(maps:get(<<"game">>, Data, undefined)),
    print_collector(maps:get(<<"collector">>, Data, undefined)),
    print_inventory(maps:get(<<"inventory">>, Data, undefined),
                    maps:get(projection_summary, Snapshot, #{})),
    ok.

-doc "Render player entities returned by unified query engine.".
-spec print_query(map(), map()) -> ok.
print_query(Query, Results) ->
    Entries = maps:get(slice, Results, []),
    io:format("Matches: ~p (showing ~p)~n~n",
              [maps:get(total, Results, 0), maps:get(shown, Results, 0)]),
    case maps:get(output_format, Query, table) of
        block -> lists:foreach(fun print_entry_block/1, Entries);
        table -> print_entry_table(Entries)
    end,
    ok.

print_game(undefined) -> ok;
print_game(Game) ->
    io:format("~nGame~n"),
    print_field("phase", maps:get(<<"phase">>, Game, undefined)),
    print_field("pid", maps:get(<<"pid">>, Game, undefined)),
    print_field("compat_data", maps:get(<<"compat_data">>, Game, undefined)).

print_collector(undefined) -> ok;
print_collector(Collector) ->
    io:format("~nCollector~n"),
    print_field("DBWIN active", maps:get(<<"debug_output_active">>, Collector, undefined)),
    print_field("inventory collector active",
                maps:get(<<"inventory_active">>, Collector, undefined)),
    print_field("inventory updates",
                maps:get(<<"inventory_updates_observed">>, Collector, undefined)),
    print_field("last observed", retinue_output:timestamp(maps:get(<<"last_observed_at">>, Collector, undefined))).

print_inventory(undefined, _Summary) -> ok;
print_inventory(Inventory, Summary) when map_size(Summary) > 0 ->
    io:format("~nInventory~n"),
    print_field("observation schema", maps:get(<<"schema">>, Inventory, undefined)),
    print_field("projection schema", maps:get(schema, Summary, undefined)),
    print_field("collected", retinue_output:timestamp(maps:get(<<"collected_at">>, Inventory, undefined))),
    print_field("equipment", maps:get(equipment, Summary, 0)),
    print_field("items", maps:get(items, Summary, 0)),
    print_field("stacks", maps:get(stacks, Summary, 0)),
    print_field("upgrades", maps:get(upgrades, Summary, 0)),
    print_field("configs", maps:get(configs, Summary, 0)),
    print_field("loadouts", maps:get(loadouts, Summary, 0)),
    print_field("mastery records", maps:get(mastery, Summary, 0)),
    print_field("pending recipes", maps:get(pending_recipes, Summary, 0)),
    print_field("schema issues", maps:get(schema_issues, Summary, 0));
print_inventory(Inventory, _Summary) ->
    Index = maps:get(<<"index">>, Inventory, #{}),
    io:format("~nInventory~n"),
    print_field("schema", maps:get(<<"schema">>, Inventory, undefined)),
    print_field("collected", retinue_output:timestamp(maps:get(<<"collected_at">>, Inventory, undefined))),
    print_field("equipment", list_size(maps:get(<<"equipment">>, Index, []))),
    print_field("stacks", list_size(maps:get(<<"stacks">>, Index, []))),
    print_field("mastery records", list_size(maps:get(<<"mastery">>, Index, []))),
    print_field("pending recipes", list_size(maps:get(<<"pending_recipes">>, Index, []))).

print_field(_Label, undefined) -> ok;
print_field(Label, Value) -> io:format("  ~s: ~s~n", [Label, value(Value)]).

print_entry_table([]) -> io:format("no entries~n");
print_entry_table(Entries) ->
    Headers = ["Type", "Name", "Collection", "Count", "XP", "Rank"],
    Rows = [[value(maps:get(type, Entry)), value(maps:get(name, Entry)),
             value(maps:get(collection, Entry, "")),
             value(maps:get(count, Entry, "")),
             value(maps:get(xp, Entry, "")),
             value(maps:get(rank, Entry, ""))] || Entry <- Entries],
    Lines = retinue_table:render_lines(Headers, Rows, #{}),
    lists:foreach(fun(Line) -> io:format("~ts~n", [Line]) end, Lines).

print_entry_block(Entry) ->
    io:format("~s (~s)~n", [value(maps:get(name, Entry)), value(maps:get(type, Entry))]),
    io:format("  source: ~s~n", [value(maps:get(source, Entry))]),
    io:format("  data: ~tp~n~n", [maps:get(data, Entry, #{})]).

source_list([]) -> "none";
source_list(Sources) -> string:join([value(Source) || Source <- lists:sort(Sources)], ", ").

list_size(Value) when is_list(Value) -> length(Value);
list_size(_Value) -> 0.

value(undefined) -> "unknown";
value(null) -> "unknown";
value(Value) -> retinue_text:to_list(Value).

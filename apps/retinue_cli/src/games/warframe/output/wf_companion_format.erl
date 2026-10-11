-module(wf_companion_format).

-export([collectors/2, metadata/1, captures/2, diagnostics/1]).

collectors(#{collector := Collector} = Player, Local) when map_size(Collector) > 0 ->
    Current = current(Collector, Local) andalso maps:get(game_active, Player, false),
    [io_lib:format("Collectors (~s)~n", [case Current of true -> "current session"; false -> "last reported" end]),
     collector_details(Collector)];
collectors(_, _) -> "Collectors: no report\n".

collector_details(Collector) ->
    [field("game pid", maps:get(<<"game_pid">>, Collector, undefined)),
     field("session", maps:get(<<"session_generation">>, Collector, undefined)),
     error_line(Collector, <<"session_error">>),
     collector("debug output", Collector, <<"debug_output_active">>, <<"debug_output_lines_observed">>, "lines"),
     inbox("debug inbox", maps:get(<<"debug_output_queue">>, Collector, undefined)),
     error_line(Collector, <<"debug_output_error">>),
     collector("inventory", Collector, <<"inventory_active">>, <<"inventory_updates_observed">>, "receipts"),
     time("last inventory received", maps:get(<<"inventory_received_at">>, Collector, undefined)),
     error_line(Collector, <<"inventory_error">>),
     pipeline(maps:get(<<"inventory_pipeline">>, Collector, undefined)),
     collector("game metadata", Collector, <<"game_metadata_active">>, <<"game_metadata_updates_observed">>, "receipts"),
     field("metadata source", maps:get(<<"game_metadata_source">>, Collector, undefined)),
     time("last metadata received", maps:get(<<"game_metadata_received_at">>, Collector, undefined)),
     error_line(Collector, <<"game_metadata_error">>),
     inbox("relic inbox", maps:get(<<"relic_inbox">>, Collector, undefined)),
     time("report updated", maps:get(<<"last_observed_at">>, Collector, undefined))].

diagnostics(#{<<"state">> := <<"running">>, <<"job">> := Job, <<"snapshot">> := Snapshot} = Data) ->
    Pipeline = case maps:get(<<"inventory_pipeline">>, Snapshot, null) of
        null -> #{}; Value -> Value
    end,
    Poll = maps:get(<<"poll">>, Pipeline, #{}),
    Queue = maps:get(<<"queue">>, Pipeline, #{}),
    [case maps:get(<<"samples">>, Data) of
         1 -> io_lib:format("Watch ~s (inventory; 1 s)~n", [Job]);
         _ -> []
     end,
     io_lib:format("~s  receipts ~p; polls ~p; work ~.2f ms; queued ~p; dropped ~p; skipped ~p~n",
                    [retinue_output:timestamp(maps:get(<<"last_observed_at">>, Snapshot)),
                     maps:get(<<"inventory_updates_observed">>, Snapshot, 0),
                     maps:get(<<"count">>, Poll, 0), maps:get(<<"mean_us">>, Poll, 0) / 1000,
                     maps:get(<<"items">>, Queue, 0), maps:get(<<"dropped_items">>, Queue, 0),
                     maps:get(<<"skipped">>, Data)])];
diagnostics(#{<<"snapshot">> := Snapshot, <<"jobs">> := Jobs}) ->
    ["Live companion collectors\n", collector_details(Snapshot),
     case Jobs of [] -> "Watches: none\n"; _ -> ["Watches\n", [diagnostic_job(Job) || Job <- Jobs]] end];
diagnostics(Data) -> diagnostic_job(Data).

diagnostic_job(#{<<"job">> := Job, <<"state">> := State} = Data) ->
    [io_lib:format("  ~s: ~s~n", [Job, maps:get(<<"job_state">>, Data, State)]),
     field("reason", maps:get(<<"reason">>, Data, undefined))];
diagnostic_job(_) -> [].

inbox(Label, #{<<"items">> := Count} = Stats) ->
    io_lib:format("  ~s: ~p queued; ~p dropped/rejected~n",
                   [Label, Count, maps:get(<<"dropped_items">>, Stats, maps:get(<<"rejected">>, Stats, 0))]);
inbox(_, _) -> [].

collector(Label, Data, Active, Count, Unit) ->
    State = case maps:get(Active, Data, false) of true -> "running"; false -> "stopped" end,
    io_lib:format("  ~s: ~s (~p ~s)~n", [Label, State, maps:get(Count, Data, 0), Unit]).

pipeline(#{<<"poll">> := Poll, <<"interval">> := Interval,
           <<"decode">> := Decode, <<"queue_delay">> := Delay, <<"queue">> := Queue}) ->
    [io_lib:format("  sampling: ~p polls; mean interval ~.2f ms, mean work ~.2f ms, max work ~.2f ms~n",
                   [maps:get(<<"count">>, Poll), ms(Interval, <<"mean_us">>),
                    ms(Poll, <<"mean_us">>), ms(Poll, <<"max_us">>)]),
     io_lib:format("  decode: mean ~.2f ms; mean queue wait ~.2f ms; queued ~p (~p bytes), dropped ~p~n",
                   [ms(Decode, <<"mean_us">>), ms(Delay, <<"mean_us">>),
                    maps:get(<<"items">>, Queue), maps:get(<<"bytes">>, Queue),
                    maps:get(<<"dropped_items">>, Queue)])];
pipeline(_) -> [].

ms(Timing, Key) -> maps:get(Key, Timing) / 1000.

metadata(#{available := Available} = Metadata) ->
    [io_lib:format("Game metadata cache: ~s (revision ~p)~n",
                   [case Available of true -> "available"; false -> "empty" end,
                    maps:get(revision, Metadata, 0)]),
     time("updated", maps:get(updated_at, Metadata, undefined)),
     field("executable SHA256", maps:get(executable_sha256, Metadata, undefined)),
     [io_lib:format("  pools: ~p Warframes, ~p primaries, ~p secondaries, ~p melees~n",
                    [maps:get(Key, Pools, 0) || Key <- [<<"suits">>, <<"primaries">>, <<"secondaries">>, <<"melees">>]])
      || Pools <- [maps:get(pools, Metadata, #{})], map_size(Pools) > 0],
     case maps:get(capture_error, Metadata, undefined) of
         #{<<"reason">> := Reason} -> field("capture warning", Reason);
         _ -> []
     end];
metadata(_) -> "Game metadata cache: unavailable\n".

captures(Player, Local) when is_map(Player) ->
    Request = maps:get(capture, Player, #{}),
    Armed = current(Request, Local) andalso maps:get(<<"state">>, Request, undefined) =:= <<"armed">>,
    ArmedLabel = case {current(Request, Local), Local} of
        {true, _} -> case Armed of true -> "yes"; false -> "no" end;
        {false, #{companions := 0}} -> "no companion connected";
        _ -> "unknown (no current report)"
    end,
    ["Relic-reward evidence capture\n",
     io_lib:format("  armed: ~s~n", [ArmedLabel]),
     case map_size(Request) of
         0 -> "  no request reported\n";
         _ -> [field("last request", maps:get(<<"state">>, Request, undefined)),
               field("request job", maps:get(<<"job">>, Request, undefined)),
               field("output", maps:get(<<"directory">>, Request, undefined)),
               time("request updated", maps:get(<<"updated_at">>, Request, undefined)),
               case Armed of true -> time("expires", maps:get(<<"expires_at">>, Request, undefined)); false -> [] end]
     end,
     case maps:get(capture_result, Player, #{}) of
         Result when map_size(Result) > 0 ->
             [field("last result", maps:get(<<"state">>, Result, undefined)),
              field("result job", maps:get(<<"job">>, Result, undefined)),
              field("result output", maps:get(<<"directory">>, Result, undefined)),
              time("result recorded", maps:get(<<"updated_at">>, Result, undefined)),
              capture_budget(maps:get(<<"budget">>, Result, #{})),
              error_line(Result, <<"error">>)];
         _ -> []
     end];
captures(_, _) -> "Relic-reward evidence capture: unavailable\n".

capture_budget(#{<<"elapsed_ms">> := Elapsed, <<"timeout_ms">> := Timeout,
                 <<"read_bytes_reserved">> := Read, <<"write_bytes_reserved">> := Written}) ->
    io_lib:format("  job budget: ~p/~p ms; ~p read bytes, ~p output bytes reserved~n",
                  [Elapsed, Timeout, Read, Written]);
capture_budget(_) -> [].

current(#{<<"companion_pid">> := Pid}, #{companion_details := Details}) ->
    lists:any(fun(Detail) -> maps:get(os_pid, Detail, undefined) =:= Pid end, Details);
current(_, _) -> false.

error_line(Data, Key) -> field("error", maps:get(Key, Data, undefined)).

field(_Label, Value) when Value =:= undefined; Value =:= null -> [];
field(Label, Value) -> io_lib:format("  ~s: ~ts~n", [Label, retinue_text:to_list(Value)]).

time(_Label, Value) when Value =:= undefined; Value =:= null -> [];
time(Label, Value) -> field(Label, wf_diagnostics_format:timestamp(Value)).

%%%-------------------------------------------------------------------
%% Daemon subscription lifecycle for worldstate watch commands.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_watch_cli).

-export([run/1]).

-doc "Subscribe to daemon watch updates and render changes until completion.".
-spec run(map()) -> ok | no_return().

run(Parsed) ->
    Specs0 = lists:reverse(maps:get(watch_specs, Parsed, [])),
    Specs1 = case Specs0 of
        [] -> wfcli_worldstate_output:default_watch_specs(Parsed);
        _ -> Specs0
    end,
    case Specs1 of
        [] ->
            retinue_cli_runtime:fail("watch requires at least one spec");
        _ -> ok
    end,
    Once = maps:get(once, Parsed, false),
    daemon_watch(Parsed, Specs1, Once).

daemon_watch(Parsed, Specs, Once) ->
    Request = #{source => worldstate,
                opts => wfcli_worldstate_output:load_opts(Parsed),
                specs => Specs,
                interval => maps:get(interval, Parsed, 60),
                once => Once,
                always => maps:get(watch_always, Parsed, false),
                mode => maps:get(mode, Parsed, list),
                calendar_day => maps:get(calendar_day, Parsed, undefined)},
    case retinue_client:subscribe(warframe, Request) of
        {ok, Handle} -> daemon_watch_loop(Handle, Parsed, Once, #{});
        {error, Reason} ->
            io:format(standard_error, "worldstate daemon error: ~ts~n",
                      [retinue_client:format_error(Reason)]),
            halt(1)
    end.

daemon_watch_loop(Handle, Parsed, Once, Previous) ->
    case retinue_client:next(Handle, infinity) of
        {ok, Update} ->
            {SpecData, Snapshots} = decorate_daemon_watch(Update, Parsed, Previous),
            AnyChanged = lists:any(fun(Data) -> maps:get(changed, Data, false) end, SpecData),
            Always = maps:get(watch_always, Parsed, false),
            case Always orelse AnyChanged of
                true ->
                    retinue_output:emit(
                      #{timestamp => {timestamp, erlang:system_time(millisecond)},
                        source => maps:get(source, Update, undefined),
                        stale => maps:get(stale, Update, false),
                        specs => [wfcli_worldstate_output:json_result(
                                    maps:with([label, type_filter, changed, entries, parsed_query], Data))
                                  || Data <- SpecData, Always orelse maps:get(changed, Data)]}, fun() ->
                    maybe_clear_screen(Parsed),
                    io:format("Worldstate watch @ ~s (~ts)~n",
                              [timestamp(Parsed), wfcli_worldstate_output:daemon_source_text(Update)]),
                    wfcli_worldstate_output:maybe_print_stale(Update),
                    io:format("~n", []),
                    lists:foreach(fun(Data) -> maybe_print_watch_spec(Data, Parsed, Always) end, SpecData)
                    end);
                false -> ok
            end,
            case Once of
                true ->
                    _ = retinue_client:unsubscribe(Handle),
                    ok;
                false -> daemon_watch_loop(Handle, Parsed, false, Snapshots)
            end;
        {error, Reason} ->
            _ = retinue_client:unsubscribe(Handle),
            io:format(standard_error, "worldstate daemon error: ~ts~n",
                      [retinue_client:format_error(Reason)]),
            halt(1)
    end.

decorate_daemon_watch(Update, Parsed, Previous) ->
    Format = case maps:get(output_format, Parsed, block) of json -> block; Other -> Other end,
    lists:foldl(
      fun(Data0, {DataAcc, SnapshotAcc}) ->
          Label = maps:get(label, Data0),
          Type = maps:get(type_filter, Data0),
          Opts = maps:get(opts, Data0),
          Entries0 = maps:get(entries, Data0, []),
          ParsedQuery = maps:get(parsed_query, Data0),
          Extracts = maps:get(extracts, ParsedQuery, []),
          Columns0 = wfcli_worldstate_output:resolve_columns(
                       Format, Parsed#{type_filter := Type, inventory := false}),
          Columns = wfcli_worldstate_output:columns_with_extras(Entries0, Columns0, Opts),
          Entries = wfcli_worldstate_output:maybe_sort_watch_entries(Entries0, Columns, Opts),
          PrevSnapshot = maps:get(Label, Previous, undefined),
          Snapshot = wfcli_worldstate_watch:build_snapshot(Entries, Format, Columns, Opts, Extracts),
          Diff = wfcli_worldstate_watch:diff(PrevSnapshot, Snapshot),
          Data = Data0#{entries => Entries, format => Format, columns => Columns,
                        extracts => Extracts, snapshot => Snapshot,
                        prev_snapshot => PrevSnapshot, diff => Diff,
                        changed => wfcli_worldstate_watch:has_changes(Diff),
                        initial => PrevSnapshot =:= undefined},
          {DataAcc ++ [Data], SnapshotAcc#{Label => Snapshot}}
      end,
      {[], #{}},
      maps:get(specs, Update, [])).

maybe_print_watch_spec(Data, Parsed, Always) ->
    case {Always, maps:get(changed, Data)} of
        {false, false} -> ok;
        _ ->
            io:format("== ~ts ==~n", [maps:get(label, Data)]),
            wfcli_worldstate_output:print_watch_results(Data, Parsed),
            io:format("~n", [])
    end.

maybe_clear_screen(Parsed) ->
    case maps:get(clear, Parsed, true) of
        true -> retinue_tty:clear_screen();
        false -> ok
    end.

timestamp(Parsed) ->
    retinue_time:format_millis(erlang:system_time(millisecond), Parsed).

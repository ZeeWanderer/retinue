-module(retinue_doctor).

-export([command/0, command/1, run/1, report/5, format/1]).

command() ->
    (command(all))#{arguments => [#{name => game, required => false,
        type => {atom, [warframe, poe2, arc]}, help => "check one game"}]}.

command(Game) ->
    #{help => "check companion health and memory adapters", defaults => #{game => Game},
      handler => {?MODULE, run},
      notes => "Read-only: does not start, restart or update applications.\n"
               "Checks existing observations; untested adapters are not passes.\n"}.

run(Args) ->
    Scope = maps:get(game, Args),
    Status = retinue_client:status(),
    Live = case Status of
        {running, _, Info} when Scope =:= all; Scope =:= warframe ->
            case companions(warframe, Info) of
                [_] -> retinue_client:inspect(warframe, #{source => companion_diagnostics, action => status});
                _ -> not_requested
            end;
        _ -> not_requested
    end,
    History = retinue_client:check_history(),
    Report = report(Scope, Status, Live, History, erlang:system_time(millisecond)),
    retinue_output:emit(Report, fun() -> io:put_chars(format(Report)) end),
    case maps:get(status, Report) of failed -> halt(1); _ -> ok end.

report(Scope, Status, Live, History, Now) ->
    LiveChecks = case Status of
        {running, _, Info} ->
            daemon_checks(Info) ++
            lists:append([game_checks(Game, Info, Live, Now) || Game <- games(Scope)]);
        {stopped, _} ->
            [check(daemon, fail, "Daemon", "not running",
                   "Run retinue daemon start, then launch the game with its companion.")];
        {error, Reason} ->
            [check(daemon, fail, "Daemon", error_text(Reason),
                   "Run retinue daemon status for connection details.")]
    end,
    Checks = LiveChecks ++ history_checks(Scope, History),
    Failures = length([C || #{state := fail} = C <- Checks]),
    Untested = length([C || #{state := State} = C <- Checks, State =:= untested orelse State =:= waiting]),
    Result = case {Failures, Untested} of
        {0, 0} -> passed;
        {0, _} -> incomplete;
        _ -> failed
    end,
    #{scope => Scope, checked_at => {timestamp, Now}, status => Result,
      failures => Failures, unverified => Untested, checks => Checks}.

history_checks(Scope, {ok, Report}) ->
    Open = [C || C <- maps:get(checks, Report), maps:get(<<"last_result">>, C) =:= <<"open">>,
                 in_scope(Scope, maps:get(<<"component">>, C))],
    Storage = case maps:get(storage_error, Report, undefined) of
        undefined -> [];
        Error -> history_error(Error)
    end,
    case {Open, Storage} of
        {[], []} -> [check(daemon, pass, "Incident history", "no unresolved adapter failures", "")];
        _ -> [recorded_failure(Scope, C) || C <- Open] ++ Storage
    end;
history_checks(_, {error, Reason}) -> history_error(Reason).

in_scope(all, _) -> true;
in_scope(Game, Component) ->
    Prefix = <<(atom_to_binary(Game))/binary, ".">>,
    binary:match(Component, Prefix) =:= {0, byte_size(Prefix)}.

recorded_failure(Scope, C) ->
    Context = maps:get(<<"context">>, C),
    (check(Scope, fail,
        ["Recorded failure: ", maps:get(<<"component">>, C), " / ", maps:get(<<"check">>, C)],
        [maps:get(<<"error">>, C), "; no successful rerun recorded (Retinue ",
         text(maps:get(<<"version">>, Context, null)), ")"],
        "Run the affected feature, then rerun doctor. See diagnostics incidents for history."))#{
            kind => incident, context => Context,
            observed_at => {timestamp, maps:get(<<"checked_at">>, C)}}.

history_error(Reason) ->
    [check(daemon, fail, "Incident history", ["could not verify stored failures: ", error_text(Reason)],
           "Run retinue diagnostics incidents for details.")].

games(all) -> [warframe, poe2, arc];
games(Game) -> [Game].

daemon_checks(Info) ->
    Offered = maps:get(erlang_distribution_rpc, maps:get(protocols, Info, #{}), #{}),
    [check(daemon, pass, "Daemon", "running", "")] ++
    case retinue_protocol:compatibility(retinue_protocol:contract(), Offered) of
        ok -> [];
        {error, _} -> [check(daemon, fail, "Daemon interfaces", "running build does not match this client's contract",
                           "Update or rebuild the Retinue apps together, then update the running daemon.")]
    end.

game_checks(arc, _Info, _Live, _Now) ->
    [check(arc, unsupported, "ARC Raiders", "launch only; memory observation is not implemented", "")];
game_checks(Game, Info, Live, Now) ->
    Name = name(Game),
    Data = game_data(Game, Info),
    case companions(Game, Info) of
        [] ->
            Active = Game =:= warframe andalso case maps:get(player, Data, undefined) of
                #{game_active := true} -> true;
                _ -> false
            end,
            [check(Game, case Active of true -> fail; false -> waiting end, Name,
                   "no companion connected; adapters not checked",
                   "Launch the game with " ++ companion(Game) ++ " and rerun doctor.")];
        [Client] ->
            version_check(Game, Client, Info) ++
            case Game of
                warframe -> warframe(Live, Client, Now);
                poe2 -> poe2(Data, Now)
            end;
        _ -> [check(Game, fail, Name, "multiple companions connected",
                    "Close the duplicate companion before checking this game.")]
    end.

companions(Game, Info) ->
    Local = maps:get(local_api, Info, unavailable),
    case Local of
        #{clients := Clients} ->
            [C || C <- Clients, maps:get(game, C, undefined) =:= atom_to_binary(Game),
                  maps:get(client, C, undefined) =:= list_to_binary(companion(Game)),
                  maps:get(mode, C, undefined) =/= <<"preview">>];
        _ -> []
    end.

version_check(Game, Client, Info) ->
    Actual = maps:get(version, Client, undefined),
    Expected = maps:get(version, Info, undefined),
    State = case {Actual, Expected} of
        {undefined, _} -> untested;
        {_, undefined} -> untested;
        _ -> case retinue_text:to_list(Actual) =:= retinue_text:to_list(Expected) of
                 true -> pass; false -> fail
             end
    end,
    [check(Game, State, name(Game) ++ " companion", ["connected; version ", text(Actual)],
           case State of pass -> ""; _ -> "Rebuild or update all Retinue apps together, then restart the companion." end)].

warframe({ok, #{<<"snapshot">> := Snapshot}}, Client, Now) ->
    case maps:get(<<"companion_pid">>, Snapshot, undefined) =:= maps:get(os_pid, Client, missing) of
        false -> [check(warframe, fail, "Live diagnostics", "response belongs to a different companion", "Rerun doctor.")];
        true -> warframe_snapshot(Snapshot, Now)
    end;
warframe({error, Reason}, _Client, _Now) ->
    [check(warframe, fail, "Live diagnostics", error_text(Reason),
           "Run retinue warframe diagnostics status; restart the companion if it does not answer.")];
warframe(_, _Client, _Now) ->
    [check(warframe, fail, "Live diagnostics", "no valid runtime report", "Restart the companion and rerun doctor.")].

warframe_snapshot(Snapshot, Now) ->
    case maps:get(<<"game_pid">>, Snapshot, null) of
        Pid when is_integer(Pid), Pid > 0 ->
            [(check(warframe, pass, "Game session", io_lib:format("PID ~p; live diagnostic reply", [Pid]), ""))#{
                evidence => maps:with([<<"game_pid">>, <<"game_started">>, <<"session_generation">>,
                                      <<"game_build">>, <<"version">>], Snapshot)}] ++
            collector(Snapshot, "Game events", <<"debug_output_active">>, <<"debug_output_error">>,
                      <<"debug_output_lines_observed">>) ++
            collector(Snapshot, "Inventory collector", <<"inventory_active">>, <<"inventory_error">>,
                      <<"inventory_updates_observed">>) ++
            metadata(Snapshot) ++
            [adapter(Key, Label, Hint, maps:get(<<"adapters">>, Snapshot, #{}), Now)
             || {Key, Label, Hint} <- adapter_checks()];
        _ -> case maps:get(<<"session_error">>, Snapshot, null) of
            null -> [check(warframe, waiting, "Game session", "companion is waiting for Warframe",
                           "Enter the orbiter, then rerun doctor.")];
            Reason -> [check(warframe, fail, "Game session", text(Reason),
                             "Run retinue warframe diagnostics logs.")]
        end
    end.

collector(Snapshot, Label, Active, Error, Count) ->
    case maps:get(Error, Snapshot, null) of
        null ->
            case {maps:get(Active, Snapshot, false), maps:get(Count, Snapshot, 0)} of
                {true, N} when N > 0 -> [check(warframe, pass, Label, "receiving data in this session", "")];
                {true, _} -> [check(warframe, untested, Label, "running; no data received yet",
                                   "Enter the orbiter and allow the game to finish loading.")];
                _ -> [check(warframe, fail, Label, "collector is stopped", "Run retinue warframe diagnostics logs.")]
            end;
        Reason -> [check(warframe, fail, Label, text(Reason), "Run retinue warframe diagnostics logs.")]
    end.

metadata(Snapshot) ->
    case {maps:get(<<"game_metadata_source">>, Snapshot, null),
          maps:get(<<"game_metadata_error">>, Snapshot, null)} of
        {<<"cache">>, null} ->
            [check(warframe, untested, "Equipment pools", "cache matches executable; memory reader not rerun", "")];
        {_, null} ->
            case maps:get(<<"game_metadata_updates_observed">>, Snapshot, 0) of
                N when N > 0 -> [check(warframe, pass, "Equipment pools", "captured in this session", "")];
                _ -> [check(warframe, untested, "Equipment pools", "not captured in this session",
                            "Open an Archimedea screen, then rerun doctor.")]
            end;
        {_, Reason} -> [check(warframe, fail, "Equipment pools", text(Reason), "Run retinue warframe diagnostics logs.")]
    end.

adapter_checks() ->
    [{<<"ui_bindings">>, "UI memory bindings", "Allow startup discovery to finish."},
     {<<"http_inventory">>, "HTTP inventory decoding", "Enter the orbiter or finish a mission."},
     {<<"native_inventory">>, "Native inventory memory", "Enter the orbiter and wait for an inventory observation."},
     {<<"account">>, "Account memory", "Enter the orbiter."},
     {<<"relic_selection">>, "Relic selection memory", "Open relic selection for a fissure."},
     {<<"relic_rewards">>, "Relic reward memory", "Complete a fissure and reach the reward screen."},
     {<<"ducat_bindings">>, "Tooltip memory bindings", "Allow startup discovery to finish."},
     {<<"ducat_kiosk">>, "Ducat kiosk detection", "Open the ducat kiosk."},
     {<<"ducat_tooltip">>, "Ducat tooltip memory", "Hover a prime part in the ducat kiosk."},
     {<<"ducat_inventory">>, "Ducat item metadata", "Open the ducat kiosk."},
     {<<"ducat_market">>, "Ducat tooltip pricing", "Hover a prime part until its price loads."}].

adapter(Key, Label, Hint, Adapters, Now) ->
    case maps:get(Key, Adapters, #{}) of
        #{<<"state">> := <<"pass">>, <<"checked_at">> := Time}
          when Key =:= <<"native_inventory">>, Now - Time > 10000 ->
            check(warframe, fail, Label, "last successful read is stale; periodic reader may have stopped",
                  "Run retinue warframe diagnostics status.");
        #{<<"state">> := <<"pass">>, <<"checked_at">> := Time} ->
            (check(warframe, pass, Label, "last attempt succeeded this session", ""))#{observed_at => {timestamp, Time}};
        #{<<"state">> := <<"fail">>, <<"error">> := Error, <<"checked_at">> := Time} ->
            (check(warframe, fail, Label, text(Error),
                   Hint ++ " Then rerun doctor."))#{observed_at => {timestamp, Time}};
        _ -> check(warframe, untested, Label, "not exercised in this session", Hint)
    end.

poe2(Data, Now) ->
    Receipt = maps:get(<<"receipt">>, Data, #{}),
    Age = Now - min(maps:get(<<"received_at">>, Receipt, 0), maps:get(<<"observed_at">>, Data, 0)),
    case {maps:get(<<"connected">>, Receipt, false), Age >= 0 andalso Age =< 65000} of
        {true, false} -> [check(poe2, fail, "Native observation", "connected producer stopped sending fresh samples",
                               "Run retinue poe2 diagnostics status.")];
        {false, _} -> [check(poe2, untested, "Native observation", "no fresh sample from a connected producer",
                        "Enter town, wait up to 30 seconds, then rerun doctor.")];
        {true, true} -> poe2_observation(Data)
    end.

poe2_observation(#{<<"status">> := <<"observed">>, <<"current">> := Current}) when is_map(Current) ->
    Issues = maps:get(<<"issues">>, Current, []),
    Base = [check(poe2, pass, "Native state", ["fresh memory sample; scene ", text(maps:get(<<"scene">>, Current, null))], "")],
    Fields = [{<<"player">>, "Character memory"}, {<<"area">>, "Zone memory"}, {<<"panels">>, "Panel memory"}],
    Base ++ [poe2_field(Key, Label, Current, Issues) || {Key, Label} <- Fields] ++
    [check(poe2, fail, text(maps:get(<<"field">>, I)), text(maps:get(<<"reason">>, I)),
           "Run retinue poe2 diagnostics status.") || I <- Issues,
       not lists:keymember(maps:get(<<"field">>, I), 1, Fields)];
poe2_observation(Data) ->
    case maps:get(<<"error">>, Data, null) of
        null -> [check(poe2, waiting, "Native state", text(maps:get(<<"status">>, Data, <<"waiting">>)), "Enter town and rerun doctor.")];
        Error -> [check(poe2, fail, "Native state", text(Error), "Run retinue poe2 diagnostics status.")]
    end.

poe2_field(Key, Label, Current, Issues) ->
    Errors = [text(maps:get(<<"reason">>, I, <<"read failed">>)) || I <- Issues,
              maps:get(<<"field">>, I, undefined) =:= Key],
    case {Errors, maps:get(Key, Current, null)} of
        {[_ | _], _} -> check(poe2, fail, Label, lists:join("; ", Errors), "Run retinue poe2 diagnostics status.");
        {[], null} -> check(poe2, untested, Label, "not available in this scene", "Enter town and rerun doctor.");
        {[], Map} when is_map(Map) ->
            case lists:any(fun(Value) -> Value =/= null end, maps:values(Map)) of
                true -> check(poe2, pass, Label, "read in the latest sample", "");
                false -> check(poe2, untested, Label, "no observation", "Enter town and rerun doctor.")
            end;
        _ -> check(poe2, pass, Label, "read in the latest sample", "")
    end.

game_data(Game, Info) ->
    case maps:get(Game, maps:get(games, Info, #{}), #{}) of
        Data when is_map(Data) -> Data;
        _ -> #{}
    end.

check(Game, State, Label, Message, Hint) ->
    #{game => Game, state => State, label => bin(Label), message => bin(Message), hint => bin(Hint)}.

format(#{checks := Checks, status := Status, unverified := Unverified}) ->
    ["Retinue doctor\n",
     [[io_lib:format("  [~-8s] ~ts: ~ts~n", [tag(maps:get(state, C)), maps:get(label, C), maps:get(message, C)]),
       case maps:get(hint, C) of <<>> -> []; Hint -> ["             ", Hint, "\n"] end] || C <- Checks],
     case Status of
         failed -> "Problems found. Fix the reported failures and rerun doctor.\n";
         incomplete -> io_lib:format("No failures found; ~p checks have not run or are waiting for the game.\n",
                                     [Unverified]);
         passed -> "All available checks passed.\n"
     end].

tag(pass) -> "OK";
tag(fail) -> "FAIL";
tag(waiting) -> "WAIT";
tag(untested) -> "UNTESTED";
tag(unsupported) -> "N/A".

name(warframe) -> "Warframe";
name(poe2) -> "PoE2".
companion(warframe) -> "wfcompanion";
companion(poe2) -> "poe2companion".
text(undefined) -> "unknown";
text(null) -> "unknown";
text(Value) -> retinue_text:to_list(Value).
bin(Value) -> unicode:characters_to_binary(Value).
error_text(Reason) -> retinue_client:format_error(Reason).

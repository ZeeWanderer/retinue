-module(wf_runtime_observations).

-export([incidents/0, captures/0, capture_rows/3, incident_rows/1]).

incidents() ->
    Player = wf_player_service:status(),
    CompanionPath = maps:get(<<"incident_log">>, maps:get(collector, Player, #{}),
                             retinue_incidents:path(companion)),
    Reports = retinue_incidents:retained(daemon, retinue_incident_log:path()) ++
              retinue_incidents:retained(companion, CompanionPath),
    Errors = [maps:without([entries], Report) || Report <- Reports, maps:is_key(error, Report)],
    case Errors of
        [] -> {ok, #{entries => incident_rows(Reports),
                     logs => [maps:without([entries], Report) || Report <- Reports]}};
        _ -> {error, {incident_logs_unavailable, Errors}}
    end.

incident_rows(Reports) ->
    [Entry#{<<"id">> => iolist_to_binary([Path, $:, integer_to_list(Index)]),
            <<"name">> => maps:get(<<"event">>, Entry, <<"log">>),
            <<"application">> => atom_to_binary(App), <<"path">> => Path,
            <<"timestamp">> => maps:get(<<"timestamp_ms">>, Entry, null)}
     || #{application := App, path := Path, entries := Entries} <- Reports,
        {Index, Entry} <- lists:enumerate(Entries)].

captures() ->
    {ok, #{entries => capture_rows(wf_player_service:status(), retinue_local_api:status(),
                                   wf_game_metadata_service:status())}}.

capture_rows(Player, Local, Metadata) ->
    Reports = [{collector, <<"Collectors">>}, {capture, <<"Relic reward request">>},
               {capture_result, <<"Relic reward result">>}],
    [report_row(Source, Name, Data, Player, Local)
     || {Source, Name} <- Reports,
        Data <- [maps:get(Source, Player, #{})], map_size(Data) > 0] ++
    [metadata_row(Metadata)].

report_row(Source, Name, Data, Player, Local) ->
    Pid = maps:get(<<"companion_pid">>, Data, undefined),
    Connected = Pid =/= undefined andalso lists:any(
        fun(Detail) -> maps:get(os_pid, Detail, undefined) =:= Pid end,
        maps:get(companion_details, Local, [])),
    Current = Connected andalso (Source =/= collector orelse maps:get(game_active, Player, false)),
    Data#{<<"id">> => atom_to_binary(Source), <<"source">> => atom_to_binary(Source),
          <<"name">> => Name, <<"current">> => Current,
          <<"state">> => maps:get(<<"state">>, Data, <<"reported">>),
          <<"timestamp">> => maps:get(<<"updated_at">>, Data,
                                     maps:get(<<"last_observed_at">>, Data, null))}.

metadata_row(Metadata) ->
    Available = maps:get(available, Metadata, false),
    #{<<"id">> => <<"game_metadata">>, <<"source">> => <<"game_metadata">>,
      <<"name">> => <<"Game metadata cache">>,
      <<"state">> => case Available of true -> <<"cached">>; false -> <<"empty">> end,
      <<"timestamp">> => maps:get(updated_at, Metadata, null),
      <<"revision">> => maps:get(revision, Metadata, 0),
      <<"executable_sha256">> => maps:get(executable_sha256, Metadata, null),
      <<"pools">> => maps:get(pools, Metadata, #{}),
      <<"error">> => case maps:get(capture_error, Metadata, undefined) of
          #{<<"reason">> := Reason} -> Reason;
          _ -> null
      end}.

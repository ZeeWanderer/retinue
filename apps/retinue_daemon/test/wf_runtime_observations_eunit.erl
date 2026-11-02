-module(wf_runtime_observations_eunit).
-include_lib("eunit/include/eunit.hrl").

capture_reports_preserve_session_and_history_test() ->
    Player = #{game_active => true,
               collector => #{<<"companion_pid">> => 7, <<"last_observed_at">> => 1000},
               capture => #{<<"companion_pid">> => 7, <<"state">> => <<"armed">>,
                            <<"directory">> => <<"/next">>, <<"updated_at">> => 2000},
               capture_result => #{<<"companion_pid">> => 6, <<"state">> => <<"complete">>,
                                   <<"directory">> => <<"/previous">>, <<"updated_at">> => 500}},
    Local = #{companion_details => [#{os_pid => 7}]},
    Metadata = #{available => true, revision => 13, updated_at => 800,
                 capture_error => #{<<"reason">> => <<"adapter not found">>}},
    [Collector, Request, Result, Cache] = wf_runtime_observations:capture_rows(Player, Local, Metadata),
    ?assertMatch(#{<<"current">> := true, <<"timestamp">> := 1000}, Collector),
    ?assertMatch(#{<<"current">> := true, <<"state">> := <<"armed">>}, Request),
    ?assertMatch(#{<<"current">> := false, <<"directory">> := <<"/previous">>}, Result),
    ?assertMatch(#{<<"state">> := <<"cached">>, <<"error">> := <<"adapter not found">>}, Cache),
    [Old | _] = wf_runtime_observations:capture_rows(Player, #{}, Metadata),
    ?assertMatch(#{<<"current">> := false}, Old),
    [Inactive | _] = wf_runtime_observations:capture_rows(Player#{game_active := false}, Local, Metadata),
    ?assertMatch(#{<<"current">> := false}, Inactive).

incident_provenance_and_unknown_times_test() ->
    Reports = [#{application => companion, path => <<"/log">>, entries => [
        #{<<"timestamp_ms">> => 1000, <<"event">> => <<"metadata.failed">>,
          <<"message">> => <<"adapter not found">>}, #{<<"message">> => <<"plain line">>}]}],
    [Known, Plain] = wf_runtime_observations:incident_rows(Reports),
    ?assertMatch(#{<<"id">> := <<"/log:1">>, <<"application">> := <<"companion">>,
                   <<"timestamp">> := 1000}, Known),
    ?assertMatch(#{<<"id">> := <<"/log:2">>, <<"timestamp">> := null}, Plain).

daemon_formatter_produces_queryable_json_test() ->
    Event = #{level => warning, msg => {"failure: ~p", [missing]}, meta => #{time => 1234567}},
    Line = iolist_to_binary(retinue_incident_log:format(Event, #{})),
    ?assertMatch(#{<<"timestamp_ms">> := 1234, <<"level">> := <<"warn">>,
                   <<"message">> := <<"failure: missing">>}, json:decode(Line)).

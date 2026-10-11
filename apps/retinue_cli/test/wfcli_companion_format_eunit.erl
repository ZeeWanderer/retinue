-module(wfcli_companion_format_eunit).

-include_lib("eunit/include/eunit.hrl").

collector_reports_receipts_errors_and_session_identity_test() ->
    Data = #{<<"companion_pid">> => 42, <<"game_pid">> => 60,
             <<"inventory_active">> => true, <<"inventory_updates_observed">> => 3,
             <<"inventory_received_at">> => 1000, <<"last_observed_at">> => 2000,
             <<"game_metadata_active">> => true,
             <<"game_metadata_error">> => <<"missing StoreManifest">>},
    Player = #{collector => Data, game_active => true},
    Local = #{companion_details => [#{os_pid => 42}]},
    Text = retinue_output:with(#{utc => true}, fun() ->
        bin(wfcli_companion_format:collectors(Player, Local))
    end),
    contains(Text, <<"current session">>),
    contains(Text, <<"inventory: running (3 receipts)">>),
    contains(Text, <<"last inventory received: 1970-01-01T00:00:01.000Z">>),
    contains(Text, <<"error: missing StoreManifest">>),
    contains(bin(wfcli_companion_format:collectors(Player, #{})), <<"last reported">>),
    contains(bin(wfcli_companion_format:collectors(Player#{game_active => false}, Local)), <<"last reported">>).

old_armed_request_is_not_reported_as_current_test() ->
    Request = #{<<"state">> => <<"armed">>, <<"companion_pid">> => 42,
                <<"directory">> => <<"/capture/next">>, <<"expires_at">> => 3000},
    Result = #{<<"state">> => <<"partial">>, <<"directory">> => <<"/capture/previous">>,
               <<"error">> => <<"UI unavailable">>, <<"updated_at">> => 1000},
    Player = #{capture => Request, capture_result => Result},
    Local = #{companions => 1, companion_details => [#{os_pid => 42}]},
    Text = bin(wfcli_companion_format:captures(Player, Local)),
    contains(Text, <<"armed: yes">>),
    contains(Text, <<"output: /capture/next">>),
    contains(Text, <<"last result: partial">>),
    contains(Text, <<"result output: /capture/previous">>),
    contains(Text, <<"error: UI unavailable">>),
    Disconnected = bin(wfcli_companion_format:captures(Player, #{companions => 0})),
    contains(Disconnected, <<"armed: no companion connected">>),
    ?assertEqual(nomatch, binary:match(Disconnected, <<"armed: yes">>)),
    contains(bin(wfcli_companion_format:captures(Player, Local#{companion_details => [#{os_pid => 43}]})),
             <<"unknown (no current report)">>).

capture_job_identity_and_budget_test() ->
    Player = #{capture => #{<<"state">> => <<"triggered">>, <<"job">> => <<"job-42">>},
               capture_result => #{<<"state">> => <<"writing">>, <<"job">> => <<"job-42">>,
                   <<"budget">> => #{<<"elapsed_ms">> => 1200, <<"timeout_ms">> => 30000,
                                    <<"read_bytes_reserved">> => 4096, <<"write_bytes_reserved">> => 512}}},
    Text = bin(wfcli_companion_format:captures(Player, #{})),
    contains(Text, <<"request job: job-42">>),
    contains(Text, <<"result job: job-42">>),
    contains(Text, <<"last result: writing">>),
    contains(Text, <<"1200/30000 ms; 4096 read bytes, 512 output bytes reserved">>).

sampling_metrics_show_work_backlog_and_loss_test() ->
    Pipeline = #{<<"poll">> => #{<<"count">> => 100, <<"mean_us">> => 120, <<"max_us">> => 500},
                 <<"interval">> => #{<<"mean_us">> => 7320},
                 <<"decode">> => #{<<"mean_us">> => 8000},
                 <<"queue_delay">> => #{<<"mean_us">> => 2500},
                 <<"queue">> => #{<<"items">> => 2, <<"bytes">> => 8192, <<"dropped_items">> => 3}},
    Text = bin(wfcli_companion_format:collectors(#{collector => #{<<"inventory_pipeline">> => Pipeline}}, #{})),
    contains(Text, <<"100 polls; mean interval 7.32 ms, mean work 0.12 ms, max work 0.50 ms">>),
    contains(Text, <<"decode: mean 8.00 ms; mean queue wait 2.50 ms; queued 2 (8192 bytes), dropped 3">>),
    Absent = bin(wfcli_companion_format:collectors(#{collector => #{<<"inventory_pipeline">> => null}}, #{})),
    ?assertEqual(nomatch, binary:match(Absent, <<"sampling:">>)).

metadata_status_exposes_cache_and_warning_test() ->
    Text = bin(wfcli_companion_format:metadata(#{
        available => true, revision => 13, executable_sha256 => <<"abc">>,
        pools => #{<<"suits">> => 119, <<"primaries">> => 84,
                   <<"secondaries">> => 72, <<"melees">> => 112},
        capture_error => #{<<"reason">> => <<"unsupported_executable">>}})),
    contains(Text, <<"available (revision 13)">>),
    contains(Text, <<"119 Warframes, 84 primaries, 72 secondaries, 112 melees">>),
    contains(Text, <<"capture warning: unsupported_executable">>).

live_diagnostics_and_watch_format_test() ->
    Snapshot = #{<<"inventory_pipeline">> => null, <<"last_observed_at">> => 1000,
                 <<"debug_output_queue">> => #{<<"items">> => 2, <<"dropped_items">> => 3}},
    Status = bin(wfcli_companion_format:diagnostics(#{<<"state">> => <<"completed">>,
                        <<"snapshot">> => Snapshot, <<"jobs">> => []})),
    contains(Status, <<"Live companion collectors">>),
    contains(Status, <<"debug inbox: 2 queued; 3 dropped/rejected">>),
    contains(Status, <<"Watches: none">>),
    Watch = retinue_output:with(#{utc => true}, fun() ->
        bin(wfcli_companion_format:diagnostics(#{<<"state">> => <<"running">>,
            <<"job">> => <<"test-job">>, <<"snapshot">> => Snapshot,
            <<"samples">> => 1, <<"skipped">> => 4}))
    end),
    contains(Watch, <<"1970-01-01T00:00:01.000Z">>),
    contains(Watch, <<"skipped 4">>),
    Stopped = bin(wfcli_companion_format:diagnostics(#{<<"state">> => <<"completed">>,
                  <<"job">> => <<"test-job">>, <<"job_state">> => <<"cancelled">>})),
    contains(Stopped, <<"test-job: cancelled">>).

bin(Text) -> unicode:characters_to_binary(Text).
contains(Text, Part) -> ?assertNotEqual(nomatch, binary:match(Text, Part)).

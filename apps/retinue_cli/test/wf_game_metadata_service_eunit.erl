%%%-------------------------------------------------------------------
%% EUnit coverage for daemon-owned game metadata persistence.
%%%-------------------------------------------------------------------
-module(wf_game_metadata_service_eunit).

-include_lib("eunit/include/eunit.hrl").

game_metadata_service_test_() ->
    {setup,
     fun setup/0,
     fun cleanup/1,
     fun(_Path) -> [
         fun persists_valid_metadata/0,
         fun unchanged_publish_keeps_revision/0,
         fun rejects_obsolete_schema/0,
         fun rejects_invalid_executable_identity/0,
         fun failed_capture_retains_catalog_across_restart/0,
         fun successful_capture_clears_warning/0,
         fun failed_capture_without_catalog_stays_unavailable/0,
         fun clear_removes_metadata/0
     ] end}.

setup() ->
    Path = filename:join(
             "/tmp", "wfcli-game-metadata-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    application:set_env(retinue_daemon, game_metadata_cache, Path),
    {ok, _Pid} = wf_game_metadata_service:start_link(),
    Path.

cleanup(Path) ->
    case whereis(wf_game_metadata_service) of
        undefined -> ok;
        _Pid -> gen_server:stop(wf_game_metadata_service)
    end,
    application:unset_env(retinue_daemon, game_metadata_cache),
    _ = file:delete(Path ++ ".tmp"),
    _ = file:delete(Path),
    ok.

persists_valid_metadata() ->
    Data = metadata(),
    {ok, First} = wf_game_metadata_service:publish(Data),
    ok = gen_server:stop(wf_game_metadata_service),
    {ok, _Pid} = wf_game_metadata_service:start_link(),
    Reloaded = wf_game_metadata_service:snapshot(),
    ?assertEqual(maps:get(revision, First), maps:get(revision, Reloaded)),
    ?assertEqual(Data, maps:get(data, Reloaded)).

unchanged_publish_keeps_revision() ->
    {ok, First} = wf_game_metadata_service:publish(metadata()),
    {ok, Second} = wf_game_metadata_service:publish(metadata()),
    ?assertEqual(maps:get(revision, First), maps:get(revision, Second)).

rejects_obsolete_schema() ->
    ?assertEqual({error, invalid_game_metadata},
                 wf_game_metadata_service:publish(
                   (metadata())#{<<"schema">> => 1})).

rejects_invalid_executable_identity() ->
    Invalid = (metadata())#{<<"executable">> => #{<<"sha256">> => <<"no">>}},
    ?assertEqual({error, invalid_game_metadata_executable},
                 wf_game_metadata_service:publish(Invalid)).

failed_capture_retains_catalog_across_restart() ->
    {ok, Original} = wf_game_metadata_service:publish(metadata()),
    Failed = unavailable(),
    {ok, Snapshot} = wf_game_metadata_service:publish(Failed),
    Expected = (metadata())#{
                 <<"capture_error">> =>
                     #{<<"executable">> => maps:get(<<"executable">>, Failed),
                       <<"reason">> => <<"unsupported_executable">>}},
    ?assertEqual(Expected, maps:get(data, Snapshot)),
    ?assertEqual(maps:get(revision, Original) + 1, maps:get(revision, Snapshot)),
    {ok, Repeated} = wf_game_metadata_service:publish(Failed),
    ?assertEqual(Snapshot, Repeated),
    ok = gen_server:stop(wf_game_metadata_service),
    {ok, _Pid} = wf_game_metadata_service:start_link(),
    Reloaded = wf_game_metadata_service:snapshot(),
    ?assertEqual(Snapshot, Reloaded),
    ?assertMatch(#{available := true, pools := #{<<"suits">> := 0},
                   capture_error := #{<<"reason">> := <<"unsupported_executable">>}},
                 wf_game_metadata_service:status()),
    ?assertMatch({ok, _, _, unverified_game_metadata},
                 wf_archimedea_loadout:metadata(Reloaded)).

successful_capture_clears_warning() ->
    Fresh = (metadata())#{<<"executable">> => maps:get(<<"executable">>, unavailable())},
    {ok, Snapshot} = wf_game_metadata_service:publish(Fresh),
    ?assertEqual(Fresh, maps:get(data, Snapshot)),
    ?assertMatch(#{available := true, capture_error := undefined}, wf_game_metadata_service:status()),
    ?assertMatch({ok, _, _, none}, wf_archimedea_loadout:metadata(Snapshot)).

failed_capture_without_catalog_stays_unavailable() ->
    ok = wf_game_metadata_service:clear(),
    {ok, Snapshot} = wf_game_metadata_service:publish(unavailable()),
    ?assertEqual(unavailable(), maps:get(data, Snapshot)),
    ?assertMatch(#{available := false, pools := #{},
                   capture_error := #{<<"reason">> := <<"unsupported_executable">>}},
                 wf_game_metadata_service:status()),
    ?assertEqual({error, unsupported_game_build},
                 wf_archimedea_loadout:metadata(Snapshot)).

clear_removes_metadata() ->
    ok = wf_game_metadata_service:clear(),
    ?assertEqual(#{}, maps:get(data, wf_game_metadata_service:snapshot())).

metadata() ->
    #{<<"schema">> => 2,
      <<"executable">> => #{<<"sha256">> => binary:copy(<<"a">>, 64)},
      <<"archimedea">> =>
          #{<<"catalog">> => #{<<"suits">> => [], <<"primaries">> => [],
                                <<"secondaries">> => [], <<"melees">> => []},
            <<"owned_suit_items">> => [], <<"owned_weapon_items">> => [],
            <<"suit_aliases">> => [], <<"weapon_aliases">> => []}}.

unavailable() ->
    #{<<"schema">> => 2,
      <<"executable">> => #{<<"sha256">> => binary:copy(<<"b">>, 64)},
      <<"unavailable">> => #{<<"reason">> => <<"unsupported_executable">>}}.

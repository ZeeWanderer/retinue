%%%-------------------------------------------------------------------
%% EUnit coverage for companion CLI command surface.
%%%-------------------------------------------------------------------
-module(wf_companion_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

companion_commands_include_lifecycle_setup_and_diagnostics_test() ->
    Commands = maps:keys(maps:get(commands, wf_companion_cli:command())),
    ?assert(lists:all(
              fun(Command) -> lists:member(Command, Commands) end,
              ["start", "stop", "restart", "status", "install", "uninstall",
               "probe", "screenshot", "capture", "relic-ocr", "preview", "logs",
               "show", "hide", "hud"])).

companion_help_explains_global_overlay_visibility_test() ->
    Help = unicode:characters_to_binary(wf_test_cli:help(["companion"])),
    ?assertNotEqual(nomatch, binary:match(Help, <<"disable the entire overlay">>)),
    ?assertNotEqual(nomatch, binary:match(Help, <<"control diagnostic HUD">>)),
    Hud = unicode:characters_to_binary(wf_test_cli:help(["companion", "hud"])),
    ?assertNotEqual(nomatch, binary:match(Hud, <<"enable HUD">>)).

companion_status_formats_negotiated_socket_contract_test() ->
    Status = iolist_to_binary(
               wf_companion_cli:format_local(
                 #{socket => "/tmp/retinue_daemon.sock",
                   contract => #{<<"envelope">> => 1,
                                 <<"interfaces">> =>
                                     #{<<"assets">> => 2, <<"player">> => 3}},
                   connections => 2,
                   companions => 1})),
    ?assertNotEqual(nomatch,
                    binary:match(Status, <<"handshake envelope: 1">>)),
    ?assertNotEqual(nomatch,
                    binary:match(Status, <<"interfaces: assets=2, player=3">>)),
    ?assertEqual(nomatch, binary:match(Status, <<"companion protocol">>)).

preview_directory_tracks_repository_or_portable_root_test() ->
    ?assertEqual("/repo/previews",
                 wf_companion_cli:preview_directory("/repo/dev/bin/wfcompanion")),
    ?assertEqual("/opt/wfcli/previews",
                 wf_companion_cli:preview_directory("/opt/wfcli/bin/wfcompanion")).

armed_capture_directory_is_unique_cache_child_test() ->
    Path = wf_companion_cli:capture_directory(1234),
    ?assertEqual("relic-reward-1234", filename:basename(Path)),
    ?assertEqual("captures", filename:basename(filename:dirname(Path))).

companion_command_waits_for_reconnect_test() ->
    put(companion_command_attempt, 0),
    Call = fun() ->
                   Attempt = get(companion_command_attempt) + 1,
                   put(companion_command_attempt, Attempt),
                   case Attempt of
                       3 -> {ok, {ok, 1}};
                       _ -> {ok, {ok, 0}}
                   end
           end,
    ?assertEqual({ok, {ok, 1}},
                 wf_companion_cli:retry_companion_command(Call, 3, 0)),
    ?assertEqual(3, get(companion_command_attempt)).

companion_command_stops_after_reconnect_window_test() ->
    put(companion_command_attempt, 0),
    Call = fun() ->
                   put(companion_command_attempt, get(companion_command_attempt) + 1),
                   {ok, {ok, 0}}
           end,
    ?assertEqual({ok, {ok, 0}},
                 wf_companion_cli:retry_companion_command(Call, 2, 0)),
    ?assertEqual(3, get(companion_command_attempt)).

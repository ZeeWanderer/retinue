%%%-------------------------------------------------------------------
%% EUnit coverage for reversible Steam launch-option editing.
%%%-------------------------------------------------------------------
-module(wfcli_companion_steam_eunit).

-include_lib("eunit/include/eunit.hrl").

empty_launch_options_round_trip_test() ->
    Content = fixture(<<>>),
    {ok, Plan} = wfcli_companion_steam:plan(Content, "/repo/prod/bin/wfcompanion"),
    Installed = maps:get(proposed, Plan),
    ?assertEqual(<<"'/repo/prod/bin/wfcompanion' launch -- %command%">>, Installed),
    {ok, Restored} = wfcli_companion_steam:restore(
                       maps:get(content, Plan), Installed, maps:get(current, Plan)),
    ?assertEqual(Content, Restored).

existing_command_is_wrapped_without_duplication_test() ->
    Content = fixture(<<"gamemoderun %command% -windowed">>),
    {ok, Plan} = wfcli_companion_steam:plan(Content, "/repo/wf companion"),
    ?assertEqual(
       <<"'/repo/wf companion' launch -- gamemoderun %command% -windowed">>,
       maps:get(proposed, Plan)).

existing_relative_companion_wrapper_is_replaced_test() ->
    Content = fixture(<<"wfcompanion launch -- %command%">>),
    {ok, Plan} = wfcli_companion_steam:plan(Content, "/repo/prod/bin/wfcompanion"),
    ?assertEqual(
       <<"'/repo/prod/bin/wfcompanion' launch -- %command%">>,
       maps:get(proposed, Plan)).

scalar_app_id_before_app_block_is_ignored_test() ->
    Content = <<"\"230410\"\t\t\"binary-cache\"\n", (fixture(<<>>))/binary>>,
    ?assertMatch({ok, _}, wfcli_companion_steam:plan(Content, "/repo/wfcompanion")).

braces_inside_other_quoted_values_do_not_end_app_block_test() ->
    Content =
        <<"\"230410\"\n{\n\t\"Json\"\t\t\"{\\\"nested\\\":true}\"\n",
          "\t\"LaunchOptions\"\t\t\"%command%\"\n}\n">>,
    ?assertMatch({ok, _}, wfcli_companion_steam:plan(Content, "/repo/wfcompanion")).

restore_refuses_user_modified_options_test() ->
    Content = fixture(<<"changed">>),
    ?assertEqual(
       {error, {launch_options_changed, <<"changed">>}},
       wfcli_companion_steam:restore(Content, <<"installed">>, <<>>)).

symlinked_steam_roots_are_one_config_test() ->
    Root = filename:join(
             "/tmp",
             "wfcli-steam-" ++ integer_to_list(erlang:unique_integer([positive]))),
    RealRoot = filename:join(Root, "Steam"),
    Config = filename:join([RealRoot, "userdata", "123", "config", "localconfig.vdf"]),
    AliasRoot = filename:join(Root, "steam-alias"),
    Alias = filename:join([AliasRoot, "userdata", "123", "config", "localconfig.vdf"]),
    try
        ok = filelib:ensure_dir(Config),
        ok = file:write_file(Config, fixture(<<>>)),
        ok = file:make_symlink(RealRoot, AliasRoot),
        ?assertEqual([Config], wfcli_companion_steam:dedupe_configs([Config, Alias]))
    after
        file:del_dir_r(Root)
    end.

fixture(LaunchOptions) ->
    Escaped = binary:replace(LaunchOptions, <<"\"">>, <<"\\\"">>, [global]),
    <<"\"UserLocalConfigStore\"\n{\n\t\"Software\"\n\t{\n",
      "\t\t\"Valve\"\n\t\t{\n\t\t\t\"Steam\"\n\t\t\t{\n",
      "\t\t\t\t\"apps\"\n\t\t\t\t{\n\t\t\t\t\t\"230410\"\n",
      "\t\t\t\t\t{\n\t\t\t\t\t\t\"LaunchOptions\"\t\t\"",
      Escaped/binary, "\"\n\t\t\t\t\t}\n\t\t\t\t}\n\t\t\t}\n",
      "\t\t}\n\t}\n}\n">>.

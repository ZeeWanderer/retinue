%%%-------------------------------------------------------------------
%% EUnit coverage for detached companion command construction.
%%%-------------------------------------------------------------------
-module(wfcli_companion_process_eunit).

-include_lib("eunit/include/eunit.hrl").

staged_binary_is_preferred_beside_escript_test() ->
    lists:foreach(fun(App) ->
        Candidates = retinue_native_program:binary_candidates(
                       App, "/repo/dev/bin/retinue", false),
        Name = atom_to_list(App),
        ?assertEqual(filename:join("/repo/dev/bin", Name), hd(Candidates)),
        ?assert(lists:all(fun(Path) -> filename:basename(Path) =:= Name end, Candidates))
    end, [wfcompanion, poe2companion]).

binary_overrides_are_game_specific_test() ->
    Root = filename:join("/tmp", "retinue-companion-bins-" ++ os:getpid() ++ "-" ++
                         integer_to_list(erlang:unique_integer([positive]))),
    Warframe = filename:join(Root, "warframe companion"),
    Poe2 = filename:join(Root, "poe2 companion"),
    Variables = [{"WFCOMPANION_BIN", Warframe}, {"POE2COMPANION_BIN", Poe2}],
    Previous = [{Name, os:getenv(Name)} || {Name, _} <- Variables],
    ok = file:make_dir(Root),
    try
        lists:foreach(fun({Name, Path}) ->
            ok = file:write_file(Path, <<>>),
            true = os:putenv(Name, Path)
        end, Variables),
        ?assertEqual({ok, Warframe}, retinue_native_program:binary(wfcompanion)),
        ?assertEqual({ok, Warframe}, retinue_native_program:binary(wfcompanion)),
        ?assertEqual({ok, Poe2}, retinue_native_program:binary(poe2companion))
    after
        lists:foreach(fun({Name, false}) -> os:unsetenv(Name);
                         ({Name, Value}) -> os:putenv(Name, Value)
                      end, Previous),
        file:del_dir_r(Root)
    end.

systemd_start_preserves_session_environment_test() ->
    Args = wfcli_companion_process:start_arguments(
             "/repo/dev/bin/wfcompanion",
             "/repo/dev/bin/wfcli",
             [{"WAYLAND_DISPLAY", "wayland-0"}]),
    ?assert(lists:member("--setenv=WAYLAND_DISPLAY=wayland-0", Args)),
    ?assert(lists:member("--setenv=RETINUE_COMMAND=/repo/dev/bin/wfcli", Args)),
    ?assertEqual(["--", "/repo/dev/bin/wfcompanion"], lists:nthtail(length(Args) - 2, Args)).

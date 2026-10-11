-module(wfcli_gui_desktop_eunit).

-include_lib("eunit/include/eunit.hrl").

desktop_entry_uses_stable_identity_and_quoted_executable_test() ->
    Entry = wfcli_gui_desktop:desktop_entry("/opt/wf cli/bin/wfgui"),
    ?assertNotEqual(nomatch,
                    binary:match(Entry, <<"Exec=\"/opt/wf cli/bin/wfgui\"">>)),
    ?assertNotEqual(nomatch, binary:match(Entry, <<"Icon=wfgui\n">>)),
    ?assertNotEqual(nomatch, binary:match(Entry, <<"StartupWMClass=wfgui\n">>)).

desktop_icon_paths_cover_scalable_and_standard_sizes_test() ->
    Paths = wfcli_gui_desktop:paths("/tmp/wfgui-data"),
    Icons = maps:get(icons, Paths),
    ?assertEqual("/tmp/wfgui-data/icons/hicolor/scalable/apps/wfgui.svg",
                 maps:get(icon, Paths)),
    ?assertEqual(9, length(Icons)),
    ?assert(lists:member(
              "/tmp/wfgui-data/icons/hicolor/512x512/apps/wfgui.png", Icons)).

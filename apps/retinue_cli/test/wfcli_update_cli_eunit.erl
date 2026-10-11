-module(wfcli_update_cli_eunit).
-include_lib("eunit/include/eunit.hrl").

selection_test_() ->
    [?_assertEqual(Expected, wfcli_update_cli:metadata_selections(
                                 wfcli_test_cli:parse(["update" | Args])))
     || {Args, Expected} <- [{[], [default]}, {["--default"], [default]},
                             {["--all"], [all]}, {["--all", "--wfcd"], [all]},
                             {["--worldstate"], []}, {["--trader"], []},
                             {["--wfcd"], [wfcd]},
                             {["--wfcd", "--worldstate"], [wfcd]},
                             {["--nodes", "--nodes"], [nodes]}]].

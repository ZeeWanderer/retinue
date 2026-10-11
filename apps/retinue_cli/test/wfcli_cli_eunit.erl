%%%-------------------------------------------------------------------
%% EUnit tests for top-level command organization.
%%%-------------------------------------------------------------------
-module(wfcli_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

command_groups_follow_domain_boundaries_test() ->
    Groups = maps:from_list(wfcli_cli:command_groups()),
    ?assert(command_in("player", maps:get("Data", Groups))),
    ?assert(command_in("market", maps:get("Data", Groups))),
    ?assert(command_in("fissures", maps:get("Worldstate", Groups))),
    ?assert(command_in("daemon", maps:get("Applications", Groups))),
    ?assertNot(command_in("player", maps:get("Tools", Groups))),
    ?assertNot(command_in("market", maps:get("Tools", Groups))).

command_in(Command, Rows) ->
    lists:keymember(Command, 1, Rows).

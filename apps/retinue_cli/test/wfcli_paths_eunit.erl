%%%-------------------------------------------------------------------
%% EUnit tests for stable managed runtime paths.
%%%-------------------------------------------------------------------
-module(wfcli_paths_eunit).

-include_lib("eunit/include/eunit.hrl").

managed_metadata_prefers_user_cache_test() ->
    Name = "wfcli-path-test.json",
    [Preferred | _] = wfcli_worldstate:metadata_paths(Name),
    ?assertEqual(filename:absname(wfcli_paths:cache_file(Name)), Preferred).

missing_export_falls_back_to_user_cache_test() ->
    Name = "DefinitelyMissingWfcliExport.json",
    [{Name, Path}] = wfcli_exports:item_sources(undefined, [Name]),
    ?assertEqual(filename:absname(wfcli_paths:cache_file(Name)), Path).

config_files_stay_under_config_directory_test() ->
    Path = wfcli_paths:config_file("companion-steam.term"),
    ?assertEqual(wfcli_paths:config_dir(), filename:dirname(Path)).

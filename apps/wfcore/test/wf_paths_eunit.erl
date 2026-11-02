%%%-------------------------------------------------------------------
%% EUnit tests for stable managed runtime paths.
%%%-------------------------------------------------------------------
-module(wf_paths_eunit).

-include_lib("eunit/include/eunit.hrl").

managed_metadata_prefers_user_cache_test() ->
    Name = "wfcli-path-test.json",
    [Preferred | _] = wf_worldstate:metadata_paths(Name),
    ?assertEqual(filename:absname(wf_paths:cache_file(Name)), Preferred).

missing_export_falls_back_to_user_cache_test() ->
    Name = "DefinitelyMissingWfcliExport.json",
    [{Name, Path}] = wf_exports:item_sources(undefined, [Name]),
    ?assertEqual(filename:absname(wf_paths:cache_file(Name)), Path).

config_files_stay_under_config_directory_test() ->
    Path = wf_paths:config_file("companion-steam.term"),
    ?assertEqual(wf_paths:config_dir(), filename:dirname(Path)).

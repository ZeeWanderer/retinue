-module(retinue_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

command_tree_test() ->
    Tree = retinue_cli:command(),
    ?assertEqual("retinue", argparse:validate(Tree, #{progname => "retinue"})),
    Commands = maps:get(commands, Tree),
    ?assertEqual(["arc", "completion", "daemon", "diagnostics", "help", "mcp", "paths", "poe2", "warframe"],
                 lists:sort(maps:keys(Commands))),
    Warframe = maps:get("warframe", Commands),
    ?assertEqual(lists:sort(wfcli_cli:command_names()),
                 lists:sort(maps:keys(maps:get(commands, Warframe)))),
    lists:foreach(fun(Name) ->
        ?assertEqual(maps:get(Name, maps:get(commands, wfcli_cli:command())),
                     maps:get(Name, Commands))
    end, ["daemon", "paths"]),
    ?assertNotEqual(maps:get("mcp", maps:get(commands, Warframe)), maps:get("mcp", Commands)),
    ?assertEqual(["logs"], maps:keys(maps:get(commands, maps:get("diagnostics", Commands)))).

warframe_parser_parity_test() ->
    Cases = [["--json", "--utc", "daemon", "status"],
             ["mods", "--name", "help", "-l7", "-fjson"],
             ["query", "first", "--search", "second term", "--limit", "2", "third",
              "--", "--help", "--json", "help"],
             ["completion", "candidates", "--", "mods", "--pol"]],
    lists:foreach(fun(Args) ->
        {ok, Expected, Path, _} = wfcli_test_cli:result(Args),
        {ok, Expected, ["warframe" | Path], _} = parse(["warframe" | Args])
    end, Cases).

global_options_test() ->
    lists:foreach(fun(Args) ->
        ?assertMatch({ok, #{output_format := json, utc := true}, _, _}, parse(Args))
    end, [["--json", "--utc", "poe2", "companion", "status"],
          ["poe2", "--json", "companion", "--utc", "events"],
          ["warframe", "daemon", "status", "--json", "--utc"],
          ["--json", "daemon", "--utc", "status"]]),
    ?assertMatch({error, ["poe2"], _, _}, parse(["poe2", "mods"])),
    ?assertMatch({error, [], _, _}, parse(["mods"])).

help_and_context_test() ->
    Previous = retinue_cli_runtime:context(),
    retinue_cli_runtime:with_context(retinue_cli:context(), fun() ->
        lists:foreach(fun(Path) ->
            ?assert(byte_size(unicode:characters_to_binary(retinue_help:text(Path))) > 20)
        end, paths(retinue_cli:command(), [])),
        Help = unicode:characters_to_binary(retinue_help:text(["warframe", "mods"])),
        ?assertNotEqual(nomatch, binary:match(Help, <<"retinue warframe mods">>)),
        Query = unicode:characters_to_binary(retinue_help:text(["warframe", "query-syntax"])),
        ?assertNotEqual(nomatch, binary:match(Query, <<"retinue warframe query">>)),
        lists:foreach(fun(Ending) ->
            ?assertEqual({help, ["poe2", "companion", "status"]},
                         parse(["poe2", "companion", "status", Ending]))
        end, ["help", "--help", "-h"])
    end),
    ?assertEqual(Previous, retinue_cli_runtime:context()),
    ?assertError(test, retinue_cli_runtime:with_context(retinue_cli:context(), fun() -> error(test) end)),
    ?assertEqual(Previous, retinue_cli_runtime:context()).

scoped_completion_test() ->
    Tree = retinue_cli:command(),
    ?assertEqual(["warframe"], retinue_completion:candidates(["wa"], Tree)),
    ?assertEqual(["poe2"], retinue_completion:candidates(["po"], Tree)),
    lists:foreach(fun(Args) ->
        ?assertEqual(retinue_completion:candidates(Args, wfcli_test_cli:tree()),
                     retinue_completion:candidates(["warframe" | Args], Tree))
    end, [[""], ["mods", "--pol"], ["help", "daemon", "sta"],
          ["query", "--format", "j"], ["query", "--", "--h"]]),
    ?assertEqual(["events"],
                 retinue_completion:candidates(["poe2", "companion", "ev"], Tree)),
    ?assertEqual(["status"],
                 retinue_completion:candidates(["poe2", "help", "companion", "sta"], Tree)).

generated_completion_aliases_test() ->
    File = filename:join("/tmp", "retinue-completion-" ++ os:getpid() ++ "-" ++
                                  integer_to_list(erlang:unique_integer([positive]))),
    Tree = retinue_cli:command(),
    try
        ok = file:write_file(File, retinue_completion:script()),
        lists:foreach(fun({Names, Prefix, Cases}) ->
            lists:foreach(fun(Name) ->
                lists:foreach(fun(Args) ->
                    ?assertEqual(retinue_completion:candidates(Prefix ++ Args, Tree),
                                 bash_candidates(File, Name, Args))
                end, Cases)
            end, Names)
        end, [{["retinue", "retinued", "./retinued"], [],
               [[""], ["wa"], ["poe2", "companion", "ev"],
                ["warframe", "help", "mods", "--pol"],
                ["--json", "warframe", "mods", "--format=j"]]},
              {["wfcli", "wfclid", "./wfclid"], ["warframe"],
               [[""], ["da"], ["help", "mods", "--pol"],
                ["--json", "help", "daemon", "sta"], ["query", "--", "--h"]]},
              {["poe2cli", "poe2clid", "./poe2clid"], ["poe2"],
               [[""], ["companion", "ev"], ["help", "companion", "sta"],
                ["companion", "events", "--j"]]},
              {["arccli", "arcclid", "./arcclid"], ["arc"],
               [[""], ["companion", "sta"], ["help", "companion", "sta"]]}])
    after
        _ = file:delete(File)
    end.

parse(Args) -> retinue_cli_args:parse(Args, retinue_cli:command(), "retinue").

paths(Node, Path) ->
    [Path | lists:append([paths(Child, Path ++ [Name])
                         || {Name, Child} <- maps:to_list(maps:get(commands, Node, #{}))])].

bash_candidates(File, Name, Args) ->
    Command = "set -euo pipefail\nsource \"$1\"\nshift\n"
              "PATH=/no-external-commands\ncompopt() { :; }\n"
              "COMP_WORDS=(\"$@\")\nCOMP_CWORD=$((${#COMP_WORDS[@]} - 1))\n"
              "_retinue_complete\nprintf '%s\\n' \"${COMPREPLY[@]}\"\n",
    Port = open_port({spawn_executable, os:find_executable("bash")},
                     [binary, exit_status, stderr_to_stdout,
                      {args, ["--noprofile", "--norc", "-c", Command, "test", File, Name | Args]}]),
    {Status, Output} = shell_output(Port, []),
    ?assertEqual({0, Output}, {Status, Output}),
    lists:usort(string:lexemes(binary_to_list(Output), "\n")).

shell_output(Port, Acc) ->
    receive
        {Port, {data, Data}} -> shell_output(Port, [Data | Acc]);
        {Port, {exit_status, Status}} -> {Status, iolist_to_binary(lists:reverse(Acc))}
    after 5000 ->
        port_close(Port),
        error(completion_timeout)
    end.

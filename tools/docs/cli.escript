#!/usr/bin/env escript
%%! +S 2:2
-mode(compile).

main([BuildDir, Output]) ->
    Paths = filelib:wildcard(filename:join([BuildDir, "lib", "*", "ebin"])),
    true = Paths =/= [],
    ok = code:add_paths(Paths),
    Context = retinue_cli:context(),
    Entries = retinue_cli_runtime:with_context(Context, fun() ->
        entries([], maps:get(tree, Context))
    end),
    ok = file:write_file(Output, jsone:encode(Entries)).

entries(Path, Node) ->
    case maps:get(help, Node, <<>>) of
        hidden -> [];
        _ ->
            Children = lists:sort(maps:to_list(maps:get(commands, Node, #{}))),
            [#{path => [unicode:characters_to_binary(P) || P <- Path],
               summary => unicode:characters_to_binary(maps:get(help, Node, "Retinue command reference")),
               help => unicode:characters_to_binary(retinue_help:text(Path))} |
             lists:append([entries(Path ++ [Name], Child) || {Name, Child} <- Children])]
    end.

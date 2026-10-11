%%%-------------------------------------------------------------------
%% EUnit coverage for generated shell completion.
%%%-------------------------------------------------------------------
-module(retinue_completion_eunit).

-include_lib("eunit/include/eunit.hrl").

top_level_completion_test() ->
    ?assert(lists:member("daemon", wf_test_cli:candidates(["da"]))),
    ?assert(lists:member("gui", wf_test_cli:candidates(["gu"]))),
    ?assert(lists:member("completion", wf_test_cli:candidates([""]))),
    ?assert(lists:member("paths", wf_test_cli:candidates(["pa"]))).

public_commands_are_unique_test() ->
    Commands = wf_cli:public_command_names(),
    ?assertEqual(length(Commands), length(lists:usort(Commands))).

focused_option_completion_test() ->
    ?assertEqual([], wf_test_cli:candidates(["items", "--pol"])),
    ?assertEqual(["--polarity"], wf_test_cli:candidates(["mods", "--pol"])),
    ?assertEqual([], wf_test_cli:candidates(["mods", "--file"])),
    ?assertEqual(["--file"], wf_test_cli:candidates(["items", "--file"])),
    ?assertEqual([], wf_test_cli:candidates(["forma-plan", "--show"])),
    ?assertEqual([], wf_test_cli:candidates(["fissures", "--deep"])),
    ?assertEqual(["--deep"], wf_test_cli:candidates(["archimedea", "--deep"])),
    ?assertEqual(["circuit"], wf_test_cli:candidates(["circ"])),
    ?assertEqual(["normal"], wf_test_cli:candidates(["circuit", "nor"])),
    ?assertEqual(["steel-path"], wf_test_cli:candidates(["endless-xp", "steel"])).

nested_command_completion_test() ->
    ?assert(lists:member("status", wf_test_cli:candidates(["daemon", ""]))),
    ?assertEqual(
       ["enable"],
       wf_test_cli:candidates(["daemon", "autostart", "en"])),
    ?assertEqual(["retinue_daemon"], wf_test_cli:candidates(["paths", "retinue_d"])),
    ?assertEqual(["wfgui"], wf_test_cli:candidates(["paths", "wfg"])),
    ?assertEqual(["--apps"], wf_test_cli:candidates(["paths", "--a"])),
    ?assertEqual(["inventory"], wf_test_cli:candidates(["baro", "i"])),
    ?assertEqual(["video"], wf_test_cli:candidates(["companion", "preview", "v"])),
    ?assertEqual(["relic-reward"],
                 wf_test_cli:candidates(["companion", "capture", "arm", "r"])),
    ?assertEqual(["install"], wf_test_cli:candidates(["completion", "i"])),
    ?assertEqual(["install"], wf_test_cli:candidates(["gui", "i"])),
    ?assertEqual(["unresolved"],
                 wf_test_cli:candidates(["diagnostics", "u"])),
    ?assertEqual(["--json"],
                 wf_test_cli:candidates(["diagnostics", "unresolved", "--j"])),
    ?assertEqual(["--dir"],
                 wf_test_cli:candidates(["completion", "install", "--d"])).

option_value_completion_test() ->
    Values = wf_test_cli:candidates(["query", "--format", ""]),
    ?assert(lists:member("table", Values)),
    ?assert(lists:member("block", Values)),
    ?assert(lists:member("json", Values)),
    ?assert(lists:member(
              "json",
              wf_test_cli:candidates(["codex", "--format", ""]))),
    ?assertEqual(
       ["html", "image"],
       wf_test_cli:candidates(["visualize", "--viz", ""])),
    ?assertEqual(
       ["html"],
       wf_test_cli:candidates(["forma-plan", "--viz", "h"])),
    ?assertNot(
       lists:member(
         "--target",
         wf_test_cli:candidates(["companion", "screenshot", ""]))).

generated_bash_completes_without_wf_process_test() ->
    Script = iolist_to_binary(retinue_completion:script()),
    ?assertEqual(nomatch, binary:match(Script, <<"$(wfcli">>)),
    ?assertNotEqual(nomatch, binary:match(Script, <<"compgen -V COMPREPLY">>)),
    ?assertEqual(nomatch, binary:match(Script, <<"mapfile">>)),
    ?assertNotEqual(nomatch, binary:match(Script, <<"complete -F _retinue_complete wfcli wfclid">>)).

generated_bash_matches_cli_completion_test() ->
    File = temp_path("script"),
    Cases = [["da"], ["daemon", "autostart", "en"], ["items", "--pol"],
             ["mods", "--name", "daemon", "--format", "j"],
             ["--no-suggest-prompt", "daemon", "sta"],
             ["daemon", "--no-suggest-prompt", "start", "--idle-"],
             ["--json", "daemon", "sta"], ["daemon", "--utc", "sta"],
             ["query", "--j"], ["--utc", "query", "--format", "j"],
             ["companion", "capture", "status", "--u"],
             ["help", "daemon", "autostart", "en"],
             ["--no-suggest-prompt", "help", "daemon", "autostart", "en"],
             ["mods", "--format=j"], ["companion", "preview", "image", "a"],
             ["query", "--", "--h"], ["paths", "w"], ["paths", "wfcli", "w"]],
    try
        ok = file:write_file(File, script_binary()),
        lists:foreach(fun(Args) ->
            ?assertEqual(wf_test_cli:candidates(Args), bash_candidates(File, Args))
        end, Cases),
        ?assertEqual(["json"], bash_candidates(File, ["mods", "--format", "=", "j"])),
        ?assertEqual(["block", "json", "table"],
                     bash_candidates(File, ["mods", "--format", "="]))
    after
        _ = file:delete(File)
    end.

script_binary() -> iolist_to_binary(retinue_completion:script()).

bash_candidates(File, Args) ->
    Command = "set -euo pipefail\n"
              "source \"$1\"\nshift\n"
              "PATH=/no-external-commands\n"
              "compopt() { :; }\n"
              "COMP_WORDS=(wfcli \"$@\")\n"
              "COMP_CWORD=$((${#COMP_WORDS[@]} - 1))\n"
              "_retinue_complete\n"
              "printf '%s\\n' \"${COMPREPLY[@]}\"\n",
    Port = open_port({spawn_executable, os:find_executable("bash")},
                     [binary, exit_status, stderr_to_stdout,
                      {args, ["--noprofile", "--norc", "-c", Command, "test", File | Args]}]),
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

managed_completion_lifecycle_test() ->
    Dir = temp_path("lifecycle_dir"),
    Bashrc = temp_path("lifecycle_bashrc"),
    Clean = <<"export WFCLI_TEST=1\n">>,
    Original = legacy_startup(Clean),
    try
        ok = file:make_dir(Dir),
        ok = file:write_file(Bashrc, Original),
        ?assertEqual({ok, false}, retinue_completion:installed(Dir)),
        ok = retinue_completion:install(Dir, Bashrc),
        ?assertEqual({ok, true}, retinue_completion:installed(Dir)),
        Expected = iolist_to_binary(retinue_completion:script()),
        lists:foreach(fun(Name) ->
            ?assertEqual({ok, Expected}, file:read_file(filename:join(Dir, Name ++ ".bash")))
        end, retinue_completion:names()),
        ?assertEqual({ok, Clean}, file:read_file(Bashrc)),
        ok = file:write_file(filename:join(Dir, "wfcli.bash"), <<"stale">>),
        ?assertEqual({ok, false}, retinue_completion:installed(Dir)),
        ok = retinue_completion:install(Dir, Bashrc),
        ?assertEqual({ok, true}, retinue_completion:installed(Dir)),
        ok = retinue_completion:uninstall(Dir, Bashrc),
        ?assertEqual({ok, false}, retinue_completion:installed(Dir)),
        ?assertEqual({ok, Clean}, file:read_file(Bashrc))
    after
        cleanup_completion_dir(Dir),
        _ = file:delete(Bashrc)
    end.

malformed_completion_block_is_not_modified_test() ->
    Dir = temp_path("malformed_dir"),
    Bashrc = temp_path("malformed_bashrc"),
    Content = <<"# >>> wfcli completion >>>\n">>,
    try
        ok = file:make_dir(Dir),
        ok = file:write_file(Bashrc, Content),
        ?assertEqual({error, malformed_completion_block},
                     retinue_completion:install(Dir, Bashrc)),
        ?assertEqual({ok, Content}, file:read_file(Bashrc)),
        ?assertEqual({ok, false}, retinue_completion:installed(Dir))
    after
        cleanup_completion_dir(Dir),
        _ = file:delete(Bashrc)
    end.

symlinked_startup_file_is_preserved_test() ->
    Dir = temp_path("symlink_dir"),
    Target = temp_path("target"),
    Link = temp_path("link"),
    Clean = <<"export WFCLI_TEST=1\n">>,
    try
        ok = file:make_dir(Dir),
        ok = file:write_file(Target, legacy_startup(Clean)),
        ok = file:make_symlink(Target, Link),
        ok = retinue_completion:install(Dir, Link),
        ?assertEqual({ok, Target}, file:read_link(Link)),
        ?assertEqual({ok, true}, retinue_completion:installed(Dir)),
        ?assertEqual({ok, Clean}, file:read_file(Target)),
        ok = retinue_completion:uninstall(Dir, Link),
        ?assertEqual({ok, Target}, file:read_link(Link)),
        ?assertEqual({ok, Clean}, file:read_file(Target))
    after
        cleanup_completion_dir(Dir),
        _ = file:delete(Link),
        _ = file:delete(Target)
    end.

legacy_startup(Prefix) ->
    <<Prefix/binary,
      "# >>> wfcli completion >>>\n"
      "if command -v wfcli >/dev/null 2>&1; then\n"
      "  eval \"$(wfcli completion bash)\"\n"
      "fi\n"
      "# <<< wfcli completion <<<\n">>.

cleanup_completion_dir(Dir) ->
    [_ = file:delete(filename:join(Dir, Name ++ ".bash")) || Name <- retinue_completion:names()],
    _ = file:del_dir(Dir),
    ok.

temp_path(Name) ->
    filename:join(
      "/tmp",
      "wf_completion_" ++ Name ++ "_" ++
      integer_to_list(erlang:unique_integer([positive]))).

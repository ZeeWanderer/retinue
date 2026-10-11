%%%-------------------------------------------------------------------
%% Bash completion generation and managed shell startup integration.
%%%-------------------------------------------------------------------
-module(retinue_completion).

-include_lib("kernel/include/file.hrl").

-export([
    command/0,
    command/1,
    candidates/1,
    candidates/2,
    script/0,
    names/0,
    install/1,
    uninstall/1,
    installed/1,
    completion_dir/0,
    bashrc_path/0
]).

-ifdef(TEST).
-export([install/2, uninstall/2]).
-endif.

-define(START_MARKER, <<"# >>> wfcli completion >>>">>).
-define(END_MARKER, <<"# <<< wfcli completion <<<">>).

-type path_result() :: {ok, file:filename_all()} | {error, term()}.

command() -> command([]).

command(Prefix) ->
    #{help => "manage lazy Bash completion",
      commands => #{
        "bash" => #{help => "print completion script", handler => fun(_) ->
            Script = script(),
            retinue_output:emit(#{script => unicode:characters_to_binary(Script)},
                               fun() -> io:put_chars(Script) end) end},
        "candidates" => #{help => hidden,
            arguments => [#{name => words, long => "-", nargs => all, required => true}],
            handler => fun(#{words := Words}) ->
                Candidates = candidates(Prefix ++ case Words of [] -> [""]; _ -> Words end),
                retinue_output:emit(#{candidates => Candidates}, fun() ->
                    [io:format("~s~n", [Word]) || Word <- Candidates], ok end) end},
        "install" => directory_command(install),
        "uninstall" => directory_command(uninstall),
        "status" => directory_command(status)}}.

directory_command(Action) ->
    #{help => atom_to_list(Action) ++ " Bash completion",
      arguments => [retinue_cli_args:option(directory, "dir", string, "completion directory")],
      handler => fun(Args) ->
          Result = case maps:find(directory, Args) of
                       {ok, Dir} -> {ok, filename:absname(Dir)};
                       error -> completion_dir()
                   end,
          case Result of
              {ok, Directory} -> directory_action(Action, Directory);
              {error, Reason} -> fail(Reason)
          end
      end}.

directory_action(status, Dir) ->
    case installed(Dir) of
        {ok, Present} ->
            retinue_output:emit(#{directory => Dir, current => Present},
                fun() -> io:format("Bash completion~n  directory: ~s~n  current: ~s~n", [Dir, yes_no(Present)]) end);
        {error, Reason} -> fail(Reason)
    end;
directory_action(Action, Dir) ->
    Result = case Action of install -> install(Dir); uninstall -> uninstall(Dir) end,
    case Result of
        ok -> retinue_output:emit(#{action => Action, directory => Dir},
            fun() -> io:format("Bash completion ~s~n  directory: ~s~n", [action_text(Action), Dir]) end);
        {error, Reason} -> fail(Reason)
    end.

action_text(install) -> "installed";
action_text(uninstall) -> "uninstalled".

candidates(Args) -> candidates(Args, maps:get(tree, retinue_cli_runtime:context())).

candidates(["help" | Args], Tree) -> candidates(Args, Tree);
candidates([], Tree) -> candidates([""], Tree);
candidates(Args, Tree) ->
    Before = lists:droplast(Args),
    Current = lists:last(Args),
    Choices = case lists:member("--", Before) of
        true -> [];
        false ->
    case argparse:parse(Before, Tree, #{progname => "retinue"}) of
                {ok, #{topic := Topic}, [_ | HelpPath], _} ->
                    candidates(lists:droplast(HelpPath) ++ Topic ++ [Current], Tree);
                {ok, Parsed, [_ | Path], _} ->
                    current_choices(retinue_cli_args:node(Path, Tree), Parsed, Current);
                {error, {[_ | Path], Expected, undefined, Detail}} ->
                    case is_map(Expected) andalso Detail =:= <<"expected argument">> of
                        true -> retinue_cli_args:choices(Expected);
                        false -> current_choices(retinue_cli_args:node(Path, Tree), #{}, Current)
                    end;
                _ -> []
            end
    end,
    lists:usort([Choice || Choice <- Choices, lists:prefix(Current, Choice)]).

current_choices(Node, Parsed, Current) ->
    case string:split(Current, "=") of
        ["--" ++ _ = Flag, _] ->
            [Flag ++ "=" ++ Value || Arg <- maps:get(arguments, Node, []),
              lists:member(Flag, retinue_cli_args:options(#{arguments => [Arg]})),
              Value <- retinue_cli_args:choices(Arg)];
        _ -> context_choices(Node, Parsed)
    end.

context_choices(Node, Parsed) ->
    Commands = [Name || {Name, Child} <- maps:to_list(maps:get(commands, Node, #{})),
                        maps:get(help, Child, "") =/= hidden],
    Positional = lists:append([retinue_cli_args:choices(Arg)
                              || #{name := Name} = Arg <- maps:get(arguments, Node, []),
                                 not is_option(Arg), not maps:is_key(Name, Parsed)]),
    Commands ++ Positional ++ retinue_cli_args:options(Node) ++ ["help", "--help", "-h"].

positional_choices(Node) ->
    [retinue_cli_args:choices(Arg) || Arg <- maps:get(arguments, Node, []), not is_option(Arg)].

is_option(Arg) -> maps:is_key(long, Arg) orelse maps:is_key(short, Arg).

contexts() -> contexts(retinue_cli:command(), [], []).

contexts(Node, Path, Inherited) ->
    Args = Inherited ++ maps:get(arguments, Node, []),
    [{context_key(Path), Node#{arguments => Args}} |
     lists:append([contexts(Child, Path ++ [Name], Args)
                   || {Name, Child} <- lists:sort(maps:to_list(maps:get(commands, Node, #{})))])].

context_key([]) -> "__root__";
context_key(Path) -> string:join(Path, " ").

script() ->
    Contexts = contexts(),
    Options = [{Key ++ " " ++ Spelling, Arg}
               || {Key, Node} <- Contexts, Arg <- maps:get(arguments, Node, []),
                  Spelling <- retinue_cli_args:options(#{arguments => [Arg]})],
    [bash_map("_WFCLI_COMPLETION_CONTEXTS",
              [{Key, context_choices(Node,
                        maps:from_list([{maps:get(name, A), present}
                                        || A <- maps:get(arguments, Node, []), not is_option(A)]))}
               || {Key, Node} <- Contexts]),
     bash_map("_WFCLI_COMPLETION_POSITIONAL",
              [{Key ++ " " ++ integer_to_list(Index), Choices}
               || {Key, Node} <- Contexts,
                  {Index, Choices} <- lists:enumerate(0, positional_choices(Node))]),
     bash_map("_WFCLI_COMPLETION_ARITY", [{Key, [arity(Arg)]} || {Key, Arg} <- Options]),
     bash_map("_WFCLI_COMPLETION_VALUES",
              [{Key, retinue_cli_args:choices(Arg)} || {Key, Arg} <- Options]),
     bash_map("_WFCLI_COMPLETION_RAW",
              [{Key, ["1"]} || {Key, Node} <- Contexts,
                              lists:any(fun(Arg) -> not is_option(Arg) andalso
                                            maps:get(nargs, Arg, one) =:= all end,
                                        maps:get(arguments, Node, []))]),
     bash_function()].

arity(#{nargs := all}) -> "all";
arity(#{action := {store, _}}) -> "flag";
arity(#{action := {append, _}}) -> "flag";
arity(_) -> "value".

bash_function() ->
    "_retinue_complete() {\n"
    "  local current=\"\${COMP_WORDS[COMP_CWORD]}\" key=__root__ pending='' literal=0 position=0\n"
    "  local word child option choices='' prefix='' i\n"
    "  case \"\${COMP_WORDS[0]##*/}\" in\n"
    "    wfcli|wfclid) key=warframe ;;\n"
    "    poe2cli|poe2clid) key=poe2 ;;\n"
    "    arccli|arcclid) key=arc ;;\n"
    "  esac\n"
    "  for ((i=1; i<COMP_CWORD; i++)); do\n"
    "    word=\"\${COMP_WORDS[i]}\"\n"
    "    if [[ -n $pending ]]; then\n"
    "      [[ $word == = ]] || pending=''\n"
    "      continue\n"
    "    fi\n"
    "    if [[ $word == -- ]]; then literal=1; break; fi\n"
    "    option=\"$key \${word%%=*}\"\n"
    "    case \"\${_WFCLI_COMPLETION_ARITY[$option]-}\" in\n"
    "      flag) continue ;;\n"
    "      all) literal=1; break ;;\n"
    "      value) [[ $word == *=* ]] || pending=$option; continue ;;\n"
    "    esac\n"
    "    if [[ $key == __root__ ]]; then child=$word; else child=\"$key $word\"; fi\n"
    "    if ((position == 0)) && [[ $word == help && \${_WFCLI_COMPLETION_CONTEXTS[$child]+set} ]]; then continue; fi\n"
    "    if ((position == 0)) && [[ \${_WFCLI_COMPLETION_CONTEXTS[$child]+set} ]]; then\n"
    "      key=$child\n"
    "    elif [[ \${_WFCLI_COMPLETION_RAW[$key]-} ]]; then\n"
    "      literal=1; break\n"
    "    else\n"
    "      ((position+=1))\n"
    "    fi\n"
    "  done\n"
    "  COMPREPLY=()\n"
    "  if ((literal)); then compopt -o default; return; fi\n"
    "  if [[ -n $pending ]]; then\n"
    "    [[ $current != = ]] || current=''\n"
    "    choices=\"\${_WFCLI_COMPLETION_VALUES[$pending]-}\"\n"
    "  elif [[ $current == --*=* ]]; then\n"
    "    option=\"$key \${current%%=*}\"\n"
    "    prefix=\"\${current%%=*}=\"; current=\"\${current#*=}\"\n"
    "    choices=\"\${_WFCLI_COMPLETION_VALUES[$option]-}\"\n"
    "  else\n"
    "    option=\"$key $position\"\n"
    "    choices=\"\${_WFCLI_COMPLETION_CONTEXTS[$key]-} \${_WFCLI_COMPLETION_POSITIONAL[$option]-}\"\n"
    "  fi\n"
    "  compgen -V COMPREPLY -W \"$choices\" -- \"$current\" || true\n"
    "  if [[ -n $prefix ]]; then\n"
    "    for i in \"\${!COMPREPLY[@]}\"; do COMPREPLY[i]=\"$prefix\${COMPREPLY[i]}\"; done\n"
    "  fi\n"
    "  if ((\${#COMPREPLY[@]} == 0)); then compopt -o default; fi\n"
    "}\n"
    "complete -F _retinue_complete wfcli wfclid retinue retinued poe2cli poe2clid arccli arcclid\n".

bash_map(Name, Entries) ->
    [
        "declare -gA ", Name, "=(\n",
        [["  [", shell_quote(Key), "]=", shell_quote(string:join(lists:usort(Choices), " ")),
          "\n"] || {Key, Choices} <- Entries],
        ")\n"
    ].

shell_quote(Text) ->
    [$', string:replace(Text, "'", "'\"'\"'", all), $'].

-doc "Install current scripts in the user bash-completion directory.".
-spec install(file:filename_all()) -> ok | {error, term()}.
install(Dir) ->
    case bashrc_path() of
        {ok, Bashrc} -> install(Dir, Bashrc);
        {error, _Reason} = Error -> Error
    end.

install(Dir, Bashrc) ->
    case read_startup(Bashrc) of
        {ok, Content} ->
            case managed_span(Content) of
                {error, _Reason} = Error -> Error;
                Span ->
                    case write_completion_files(Dir) of
                        ok -> remove_managed_startup(Bashrc, Content, Span);
                        {error, _Reason} = Error -> Error
                    end
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Remove user-installed completion files and the obsolete startup block.".
-spec uninstall(file:filename_all()) -> ok | {error, term()}.
uninstall(Dir) ->
    case bashrc_path() of
        {ok, Bashrc} -> uninstall(Dir, Bashrc);
        {error, _Reason} = Error -> Error
    end.

uninstall(Dir, Bashrc) ->
    case read_startup(Bashrc) of
        {ok, Content} ->
            case managed_span(Content) of
                {error, _Reason} = Error -> Error;
                Span ->
                    case delete_completion_files(Dir) of
                        ok -> remove_managed_startup(Bashrc, Content, Span);
                        {error, _Reason} = Error -> Error
                    end
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Return whether all user completion files match this build.".
-spec installed(file:filename_all()) -> {ok, boolean()} | {error, term()}.
installed(Dir) ->
    completion_files_match(completion_files(Dir), iolist_to_binary(script())).

-doc "Return the default per-user bash-completion directory.".
-spec completion_dir() -> path_result().
completion_dir() ->
    case first_env_path("BASH_COMPLETION_USER_DIR") of
        {ok, Root} -> {ok, filename:join(Root, "completions")};
        not_set ->
            case data_home() of
                {ok, DataHome} ->
                    {ok, filename:join([DataHome, "bash-completion", "completions"])};
                {error, _Reason} = Error -> Error
            end
    end.

-doc "Return the default Bash startup file.".
-spec bashrc_path() -> path_result().
bashrc_path() ->
    case os:getenv("HOME") of
        false -> {error, home_not_set};
        undefined -> {error, home_not_set};
        "" -> {error, home_not_set};
        Home -> {ok, filename:join(Home, ".bashrc")}
    end.

first_env_path(Name) ->
    case os:getenv(Name) of
        false -> not_set;
        undefined -> not_set;
        "" -> not_set;
        Value ->
            case string:lexemes(Value, ":") of
                [Path | _] -> {ok, filename:absname(Path)};
                [] -> not_set
            end
    end.

data_home() ->
    case os:getenv("XDG_DATA_HOME") of
        Value when is_list(Value), Value =/= "" -> {ok, filename:absname(Value)};
        _ ->
            case os:getenv("HOME") of
                Home when is_list(Home), Home =/= "" ->
                    {ok, filename:join(Home, ".local/share")};
                _ -> {error, home_not_set}
            end
    end.

completion_files(Dir) ->
    [filename:join(Dir, Name ++ ".bash") || Name <- names()].

names() -> ["retinue", "retinued", "wfcli", "wfclid", "poe2cli", "poe2clid", "arccli", "arcclid"].

write_completion_files(Dir) ->
    write_completion_files(completion_files(Dir), script()).

write_completion_files([], _Content) -> ok;
write_completion_files([Path | Rest], Content) ->
    case atomic_write(Path, Content) of
        ok -> write_completion_files(Rest, Content);
        {error, _Reason} = Error -> Error
    end.

delete_completion_files(Dir) ->
    delete_completion_paths(completion_files(Dir)).

delete_completion_paths([]) -> ok;
delete_completion_paths([Path | Rest]) ->
    case file:delete(Path) of
        ok -> delete_completion_paths(Rest);
        {error, enoent} -> delete_completion_paths(Rest);
        {error, Reason} -> {error, {completion_delete_failed, Path, Reason}}
    end.

completion_files_match([], _Expected) -> {ok, true};
completion_files_match([Path | Rest], Expected) ->
    case file:read_file(Path) of
        {ok, Expected} -> completion_files_match(Rest, Expected);
        {ok, _Stale} -> {ok, false};
        {error, enoent} -> {ok, false};
        {error, Reason} -> {error, {completion_read_failed, Path, Reason}}
    end.

remove_managed_startup(_Path, _Content, absent) -> ok;
remove_managed_startup(Path, Content, {present, Start, Finish}) ->
    Prefix = binary:part(Content, 0, Start),
    Suffix0 = binary:part(Content, Finish, byte_size(Content) - Finish),
    write_startup(Path, [Prefix, drop_leading_newline(Suffix0)]).

read_startup(Path) ->
    case file:read_file(Path) of
        {ok, Content} -> {ok, Content};
        {error, enoent} -> {ok, <<>>};
        {error, Reason} -> {error, {completion_read_failed, Path, Reason}}
    end.

write_startup(Path, Content) ->
    write_startup(Path, Content, 16).

write_startup(_Path, _Content, 0) ->
    {error, completion_symlink_depth};
write_startup(Path, Content, Depth) ->
    case file:read_link(Path) of
        {ok, Target} ->
            write_startup(filename:absname(Target, filename:dirname(Path)),
                          Content, Depth - 1);
        {error, einval} -> atomic_write(Path, Content);
        {error, enoent} -> atomic_write(Path, Content);
        {error, Reason} -> {error, {completion_link_failed, Path, Reason}}
    end.

atomic_write(Path, Content) ->
    Temp = Path ++ ".tmp." ++ integer_to_list(erlang:unique_integer([positive])),
    case filelib:ensure_dir(Path) of
        ok ->
            case file:write_file(Temp, Content) of
                ok -> replace_file(Path, Temp);
                {error, Reason} ->
                    _ = file:delete(Temp),
                    {error, {completion_write_failed, Path, Reason}}
            end;
        {error, Reason} ->
            {error, {completion_directory_failed, Path, Reason}}
    end.

replace_file(Path, Temp) ->
    case file:read_file_info(Path) of
        {ok, #file_info{mode = Mode}} -> _ = file:change_mode(Temp, Mode);
        {error, _Reason} -> ok
    end,
    case file:rename(Temp, Path) of
        ok -> ok;
        {error, Reason} ->
            _ = file:delete(Temp),
            {error, {completion_install_failed, Path, Reason}}
    end.

managed_span(Content) ->
    Starts = binary:matches(Content, ?START_MARKER),
    Ends = binary:matches(Content, ?END_MARKER),
    case {Starts, Ends} of
        {[], []} -> absent;
        {[{Start, _}], [{End, EndLength}]} when End > Start ->
            {present, Start, End + EndLength};
        _ -> {error, malformed_completion_block}
    end.

drop_leading_newline(<<"\r\n", Rest/binary>>) -> Rest;
drop_leading_newline(<<"\n", Rest/binary>>) -> Rest;
drop_leading_newline(Content) -> Content.

yes_no(true) -> "yes";
yes_no(false) -> "no".

fail(Reason) ->
    io:format(standard_error, "error: completion: ~p~n", [Reason]),
    halt(1).
